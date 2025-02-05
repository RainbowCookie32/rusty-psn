mod manifest_parser;
mod parser;
pub mod utils;

use std::{path::PathBuf, str::FromStr};

use reqwest::Url;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::sync::mpsc::Sender;
use utils::{copy_pkg_file, get_platform_variant, get_update_info_url, PlaformVariant};

use crate::utils::create_new_pkg_path;

#[derive(Debug)]
pub enum DownloadStatus {
    Progress(u64),

    Verifying,
    DownloadSuccess,
    DownloadFailure,
}

#[derive(Debug)]
pub enum MergeStatus {
    PartProgress(usize),

    MergeSuccess,
    MergeFailure,
}

#[derive(Debug)]
pub enum MergeError {
    FilepathMismatch(String),
    FileMergeFailure,
    PackagesUnmergable(String),
}

#[derive(Debug)]
pub enum DownloadError {
    // bool represents whether we received less data than expected.
    // Sony's servers like to drop out before the transfer is actually completed.
    HashMismatch(bool),
    Tokio(tokio::io::Error),
    Reqwest(reqwest::Error),
}

#[derive(Debug)]
pub enum UpdateError {
    InvalidSerial,
    NoUpdatesAvailable,
    UnhandledErrorResponse(String),
    Reqwest(reqwest::Error),
    XmlParsing(quick_xml::Error),
    ManifestParsing(serde_json::Error),
}

#[derive(Clone)]
pub struct UpdateInfo {
    pub title_id: String,
    pub tag_name: String,

    pub titles: Vec<String>,
    pub packages: Vec<PackageInfo>,
    pub platform_variant: PlaformVariant,
}

impl UpdateInfo {
    fn empty(platform_variant: PlaformVariant) -> UpdateInfo {
        UpdateInfo {
            title_id: String::new(),
            tag_name: String::new(),

            titles: Vec::new(),
            packages: Vec::new(),
            platform_variant,
        }
    }

    pub fn title(&self) -> String {
        if let Some(title) = self.titles.first() {
            title.clone()
        } else {
            String::new()
        }
    }

    pub async fn get_info(title_id: String) -> Result<UpdateInfo, UpdateError> {
        let title_id = parse_title_id(&title_id);
        let platform_variant = match get_platform_variant(&title_id) {
            Some(variant) => variant,
            None => return Err(UpdateError::InvalidSerial),
        };
        let url = match get_update_info_url(&title_id, platform_variant) {
            Ok(url) => url,
            Err(err) => return Err(err),
        };
        let client = reqwest::ClientBuilder::default()
            // Sony has funky certificates, so this needs to be enabled.
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(UpdateError::Reqwest)?;

        info!("Querying for updates for serial: {}", title_id);

        let response = client.get(url).send().await.map_err(UpdateError::Reqwest)?;
        let response_txt = response.text().await.map_err(UpdateError::Reqwest)?;

        if response_txt.is_empty() {
            return Err(UpdateError::NoUpdatesAvailable);
        }

        if response_txt.contains("Not found") {
            return Err(UpdateError::InvalidSerial);
        }

        let mut info = UpdateInfo::empty(platform_variant);
        match parser::parse_response(response_txt, &mut info) {
            Ok(()) => {
                if info.title_id.is_empty() || info.packages.is_empty() {
                    return Err(UpdateError::NoUpdatesAvailable);
                }

                // This abomination comes courtesy of BCUS98233.
                // For some ungodly reason, the title has a newline (/n), which of course causes issues
                // both when displaying the title and when trying to create a folder to put the files in.
                let titles = &info.titles;
                info.titles = titles
                    .iter()
                    .map(|title| title.replace("\n", " "))
                    .collect();
            }
            Err(e) => match e {
                parser::ParseError::ErrorCode(reason) => {
                    if reason == "NoSuchKey" {
                        return Err(UpdateError::InvalidSerial);
                    }

                    return Err(UpdateError::UnhandledErrorResponse(reason));
                }
                parser::ParseError::XmlParsing(reason) => {
                    return Err(UpdateError::XmlParsing(reason))
                }
            },
        }

        if platform_variant != PlaformVariant::PS4 {
            return Ok(info);
        }

        let mut parent_manifest_packages = info.packages;
        info.packages = Vec::new(); // previously fetched manifest packages are moved out of packages list and a new list of part packages will be filled-in instead

        for package in parent_manifest_packages.drain(..) {
            let manifest_response = client
                .get(&package.manifest_url)
                .send()
                .await
                .map_err(UpdateError::Reqwest)?;
            let manifest_response_txt = manifest_response
                .text()
                .await
                .map_err(UpdateError::Reqwest)?;
            match manifest_parser::parse_manifest_response(
                manifest_response_txt,
                &package,
                &mut info,
            ) {
                Ok(()) => {}
                Err(e) => {
                    match e {
                        manifest_parser::ParseError::NoPartsFound => {
                            return Err(UpdateError::NoUpdatesAvailable)
                        }
                        manifest_parser::ParseError::JsonParsing(reason) => {
                            return Err(UpdateError::ManifestParsing(reason))
                        }
                    };
                }
            }
        }

        Ok(info)
    }

    pub async fn merge_parts(
        &self,
        tx: Sender<MergeStatus>,
        download_path: &PathBuf,
    ) -> Result<(), MergeError> {
        if !self.packages.iter().all(|pkg| pkg.part_number.is_some()) {
            return Err(MergeError::PackagesUnmergable(String::from(
                "some packages for the update are not a partial package",
            )));
        }

        let mut packages_sorted_by_part_number = self.packages.clone();
        packages_sorted_by_part_number.sort_by_key(|pkg| pkg.part_number.unwrap());
        let package_download_path =
            create_new_pkg_path(&download_path, &self.title_id, &self.title());

        info!("Starting merge for {}", self.title());

        for package in self.packages.iter() {
            let file_name = match package.file_name() {
                Some(name) => name,
                None => {
                    return Err(MergeError::FilepathMismatch(String::from(
                        "could not deduce filename from a package url",
                    )))
                }
            };

            let part_number = package.part_number.unwrap();
            let expected_end_of_file_name = format!("_{}.pkg", part_number - 1);
            if !file_name.ends_with(&expected_end_of_file_name) {
                return Err(MergeError::FilepathMismatch(String::from(
                    "package name does not end with expected index and extension",
                )));
            }

            let merged_file_name = file_name.replace(&expected_end_of_file_name, ".pkg");
            let mut merged_path = package_download_path.clone();
            merged_path.push(&merged_file_name);
            let mut package_path = package_download_path.clone();
            package_path.push(&file_name);
            match copy_pkg_file(&package_path, &merged_path, package.offset).await {
                Ok(read_length) => {
                    tx.send(MergeStatus::PartProgress(part_number))
                        .await
                        .unwrap();
                    info!(
                        "merged {} bytes from {} to {}",
                        read_length, file_name, merged_file_name
                    );
                }
                Err(err) => {
                    error!("could not merge files: {}", err.to_string());
                    return Err(MergeError::FileMergeFailure);
                }
            };
        }

        tx.send(MergeStatus::MergeSuccess).await.unwrap();
        Ok(())
    }
}

pub fn parse_title_id(title_id: &str) -> String {
    title_id
        .trim()
        .replace("-", "") // strip the dash that some sites put in a title id, eg. BCES-xxxxx
        .to_uppercase()
}

#[derive(Clone)]
pub struct PackageInfo {
    pub url: String,
    pub size: u64,
    pub version: String,
    pub sha1sum: String,
    pub hash_whole_file: bool,
    pub manifest_url: String,
    pub offset: u64,
    pub part_number: Option<usize>,
}

impl PackageInfo {
    fn empty() -> PackageInfo {
        PackageInfo {
            url: String::new(),
            size: 0,
            version: String::new(),
            sha1sum: String::new(),
            hash_whole_file: false,
            manifest_url: String::new(),
            offset: 0,
            part_number: None,
        }
    }

    pub fn id(&self) -> String {
        match self.part_number {
            Some(part_idx) => format!("{0} - Part {1}", self.version, part_idx),
            None => self.version.to_owned(),
        }
    }

    pub async fn start_download(
        &self,
        tx: Sender<DownloadStatus>,
        download_path: PathBuf,
        serial: String,
        title: String,
    ) -> Result<(), DownloadError> {
        info!("Starting download for for {serial} {}", self.version);
        info!("Sending pkg file request to url: {}", &self.url);

        let client = reqwest::ClientBuilder::default()
            // Sony has funky certificates, so this needs to be enabled.
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(DownloadError::Reqwest)?;

        let mut response = client
            .get(&self.url)
            .send()
            .await
            .map_err(DownloadError::Reqwest)?;

        let file_name = response
            .url()
            .path_segments()
            .and_then(|s| s.into_iter().next_back())
            .and_then(|n| {
                if n.is_empty() {
                    None
                } else {
                    Some(n.to_string())
                }
            })
            .unwrap_or_else(|| String::from("update.pkg"));

        info!("Response received, file name is {file_name}");

        let mut pkg_file =
            crate::utils::create_pkg_file(download_path, &serial, &title, &file_name).await?;

        tx.send(DownloadStatus::Verifying).await.unwrap();

        if !crate::utils::hash_file(&mut pkg_file, &self.sha1sum, self.hash_whole_file).await? {
            if let Err(e) = pkg_file.set_len(0).await {
                error!("Failed to set file length to 0: {e}");
                return Err(DownloadError::Tokio(e));
            }

            if let Err(e) = pkg_file.seek(SeekFrom::Start(0)).await {
                error!("Failed to set the package file cursor at position 0: {e}");
                return Err(DownloadError::Tokio(e));
            };

            let mut received_data = 0;

            while let Some(download_chunk) =
                response.chunk().await.map_err(DownloadError::Reqwest)?
            {
                let download_chunk = download_chunk.as_ref();
                let download_chunk_len = download_chunk.len() as u64;

                received_data += download_chunk_len;
                info!(
                    "Received a {} bytes chunk for {serial} {}",
                    download_chunk_len, self.version
                );

                tx.send(DownloadStatus::Progress(download_chunk_len))
                    .await
                    .unwrap();

                if let Err(e) = pkg_file.write_all(download_chunk).await {
                    error!("Failed to write chunk data: {e}");
                    return Err(DownloadError::Tokio(e));
                }
            }

            if let Err(e) = pkg_file.sync_all().await {
                error!("Failed to flush all data to file: {e}");
                return Err(DownloadError::Tokio(e));
            }

            if received_data < self.size {
                warn!("Received less data than expected for pkg file! Expected {} bytes, received {} bytes.", self.size, received_data)
            }

            info!(
                "No more chunks available, hashing received file for {serial} {}",
                self.version
            );

            tx.send(DownloadStatus::Verifying).await.unwrap();

            if crate::utils::hash_file(&mut pkg_file, &self.sha1sum, self.hash_whole_file).await? {
                info!("Hash for {serial} {} matched, wrapping up...", self.version);
                tx.send(DownloadStatus::DownloadSuccess).await.unwrap();

                Ok(())
            } else {
                error!("Hash mismatch for {serial} {}!", self.version);
                tx.send(DownloadStatus::DownloadFailure).await.unwrap();

                Err(DownloadError::HashMismatch(received_data < self.size))
            }
        } else {
            info!(
                "File for {serial} {} already existed and was complete, wrapping up...",
                self.version
            );
            tx.send(DownloadStatus::DownloadSuccess).await.unwrap();

            Ok(())
        }
    }

    pub fn file_name(&self) -> Option<String> {
        let pkg_url = match Url::from_str(&self.url) {
            Ok(url) => url,
            Err(_) => return None,
        };

        let file_name = pkg_url
            .path_segments()
            .and_then(|s| s.into_iter().next_back())
            .and_then(|n| {
                if n.is_empty() {
                    None
                } else {
                    Some(n.to_string())
                }
            });

        file_name
    }
}

mod tests {
    #[tokio::test]
    async fn parse_ac3() {
        match super::UpdateInfo::get_info("NPUB30826".to_string()).await {
            Ok(info) => assert!(info.packages.len() == 1),
            Err(e) => panic!("Failed to get info for NPUB30826: {:?}", e),
        }
    }

    #[tokio::test]
    async fn parse_lpb() {
        match super::UpdateInfo::get_info("BCUS98148".to_string()).await {
            Ok(info) => assert!(info.packages.len() == 13),
            Err(e) => panic!("Failed to get info for BCUS98148: {:?}", e),
        }
    }

    #[tokio::test]
    async fn parse_infamous2() {
        match super::UpdateInfo::get_info("NPUA80638".to_string()).await {
            Ok(info) => assert!(info.packages.len() == 3),
            Err(e) => panic!("Failed to get info for NPUA80638: {:?}", e),
        }
    }

    #[tokio::test]
    async fn parse_tokyo_jungle() {
        match super::UpdateInfo::get_info("NPUA80523".to_string()).await {
            Ok(info) => assert!(info.packages.len() == 1),
            Err(e) => panic!("Failed to get info for NPUA80523: {:?}", e),
        }
    }
}
