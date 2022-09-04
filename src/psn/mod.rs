use std::path::PathBuf;

use serde::Deserialize;
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
    Serde,
    InvalidSerial,
    NoUpdatesAvailable,
    Reqwest(reqwest::Error)
}

#[derive(Clone, Deserialize)]
pub struct UpdateInfo {
    #[serde(rename = "titleid")]
    pub title_id: String,
    pub tag: UpdateTag
}

impl UpdateInfo {
    pub async fn get_info(title_id: String) -> Result<UpdateInfo, UpdateError> {
        let title_id = title_id.to_uppercase();
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
            serde_xml_rs::from_str(&response_txt).map_err(|_| UpdateError::Serde)
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct UpdateTag {
    pub name: String,
    #[serde(rename = "package")]
    pub packages: Vec<PackageInfo>
}

#[derive(Clone, Deserialize)]
pub struct PackageInfo {
    pub url: String,
    pub size: u64,
    pub version: String,
    pub sha1sum: String,

    pub paramsfo: Option<ParamSfo>
}

impl PackageInfo {
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
            pkg_file.set_len(0).await.map_err(DownloadError::Tokio)?;

            while let Some(download_chunk) = response.chunk().await.map_err(DownloadError::Reqwest)? {
                let download_chunk = download_chunk.as_ref();
                let download_chunk_len = download_chunk.len() as u64;

                info!("Received a {} bytes chunk for {serial} {}", download_chunk_len, self.version);

                tx.send(DownloadStatus::Progress(download_chunk_len)).await.unwrap();
                pkg_file.write_all(download_chunk).await.map_err(DownloadError::Tokio)?;
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

#[derive(Clone, Deserialize)]
pub struct ParamSfo {
    #[serde(rename = "$value")]
    pub titles: Vec<String>
}
