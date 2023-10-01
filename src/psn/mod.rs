mod parser;

use std::path::PathBuf;

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub enum DownloadStatus {
    Progress(u64),
    
    Verifying,
    DownloadSuccess,
    DownloadFailure
}

#[derive(Debug)]
pub enum DownloadError {
    HashMismatch,
    Tokio(tokio::io::Error),
    Reqwest(reqwest::Error)
}

#[derive(Debug)]
pub enum UpdateError {
    InvalidSerial,
    NoUpdatesAvailable,
    Reqwest(reqwest::Error),
    XmlParsing(quick_xml::Error),
}

#[derive(Clone)]
pub struct UpdateInfo {
    pub title_id: String,
    pub tag_name: String,

    pub titles: Vec<String>,
    pub packages: Vec<PackageInfo>,
}

impl UpdateInfo {
    fn empty() -> UpdateInfo {
        UpdateInfo {
            title_id: String::new(),
            tag_name: String::new(),

            titles: Vec::new(),
            packages: Vec::new(),
        }
    }

    pub async fn get_info(title_id: String) -> Result<UpdateInfo, UpdateError> {
        let title_id = title_id.trim().to_uppercase();
        let url = format!("https://a0.ww.np.dl.playstation.net/tpl/np/{0}/{0}-ver.xml", title_id);
        let client = reqwest::ClientBuilder::default()
            // Sony has funky certificates, so this needs to be enabled.
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(UpdateError::Reqwest)?
        ;

        info!("Querying for updates for serial: {}", title_id);
    
        let response = client.get(url).send().await.map_err(UpdateError::Reqwest)?;
        let response_txt = response.text().await.map_err(UpdateError::Reqwest)?;

        if response_txt.is_empty() {
            Err(UpdateError::NoUpdatesAvailable)
        }
        else if response_txt.contains("Not found") {
            Err(UpdateError::InvalidSerial)
        }
        else {
            parser::parse_response(response_txt)
                .map_err(UpdateError::XmlParsing)
        }
    }
}

#[derive(Clone)]
pub struct PackageInfo {
    pub url: String,
    pub size: u64,
    pub version: String,
    pub sha1sum: String
}

impl PackageInfo {
    fn empty() -> PackageInfo {
        PackageInfo {
            url: String::new(),
            size: 0,
            version: String::new(),
            sha1sum: String::new()
        }
    }

    pub async fn start_download(&self, tx: Sender<DownloadStatus>, serial: String, mut download_path: PathBuf) -> Result<(), DownloadError> {
        info!("Starting download for for {serial} {}", self.version);
        info!("Sending pkg file request to url: {}", &self.url);

        let client = reqwest::ClientBuilder::default()
            // Sony has funky certificates, so this needs to be enabled.
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(DownloadError::Reqwest)?
        ;

        let mut response = client.get(&self.url)
            .send()
            .await
            .map_err(DownloadError::Reqwest)?
        ;

        let file_name = response
            .url()
            .path_segments()
            .and_then(|s| s.last())
            .and_then(|n| if n.is_empty() { None } else { Some(n.to_string()) })
            .unwrap_or_else(|| String::from("update.pkg"))
        ;

        info!("Response received, file name is {file_name}");
        download_path.push(format!("{serial}/{file_name}"));

        let mut pkg_file = crate::utils::create_pkg_file(download_path).await?;

        tx.send(DownloadStatus::Verifying).await.unwrap();

        if !crate::utils::hash_file(&mut pkg_file, &self.sha1sum).await? {
            if let Err(e) = pkg_file.set_len(0).await {
                error!("Failed to set file lenght to 0: {e}");
                return Err(DownloadError::Tokio(e));
            }

            while let Some(download_chunk) = response.chunk().await.map_err(DownloadError::Reqwest)? {
                let download_chunk = download_chunk.as_ref();
                let download_chunk_len = download_chunk.len() as u64;

                info!("Received a {} bytes chunk for {serial} {}", download_chunk_len, self.version);

                tx.send(DownloadStatus::Progress(download_chunk_len)).await.unwrap();

                if let Err(e) = pkg_file.write_all(download_chunk).await {
                    error!("Failed to write chunk data: {e}");
                    return Err(DownloadError::Tokio(e));
                }
            }

            info!("No more chunks available, hashing received file for {serial} {}", self.version);

            tx.send(DownloadStatus::Verifying).await.unwrap();
                                            
            if crate::utils::hash_file(&mut pkg_file, &self.sha1sum).await? {
                info!("Hash for {serial} {} matched, wrapping up...", self.version);
                tx.send(DownloadStatus::DownloadSuccess).await.unwrap();

                Ok(())
            }
            else {
                error!("Hash mismatch for {serial} {}!", self.version);
                tx.send(DownloadStatus::DownloadFailure).await.unwrap();

                Err(DownloadError::HashMismatch)
            }
        }
        else {
            info!("File for {serial} {} already existed and was complete, wrapping up...", self.version);
            tx.send(DownloadStatus::DownloadSuccess).await.unwrap();

            Ok(())
        }
    }
}

mod tests {
    #[tokio::test]
    async fn parse_ac3() {
        match super::UpdateInfo::get_info("NPUB30826".to_string()).await {
            Ok(info) => assert!(info.packages.len() == 1),
            Err(e) => panic!("Failed to get info for NPUB30826: {:?}", e)
        }
    }

    #[tokio::test]
    async fn parse_lpb() {
        match super::UpdateInfo::get_info("BCUS98148".to_string()).await {
            Ok(info) => assert!(info.packages.len() == 13),
            Err(e) => panic!("Failed to get info for BCUS98148: {:?}", e)
        }
    }

    #[tokio::test]
    async fn parse_infamous2() {
        match super::UpdateInfo::get_info("NPUA80638".to_string()).await {
            Ok(info) => assert!(info.packages.len() == 3),
            Err(e) => panic!("Failed to get info for NPUA80638: {:?}", e)
        }
    }
    
    #[tokio::test]
    async fn parse_tokyo_jungle() {
        match super::UpdateInfo::get_info("NPUA80523".to_string()).await {
            Ok(info) => assert!(info.packages.len() == 1),
            Err(e) => panic!("Failed to get info for NPUA80523: {:?}", e)
        }
    }
}
