use std::path::PathBuf;

use sha1_smol::Sha1;
use reqwest::{ClientBuilder, Response};

use tokio::fs;
use tokio::fs::{File, OpenOptions};

use tokio::io;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use crate::psn::DownloadError;

pub async fn send_pkg_request(url: String) -> Result<(String, Response), DownloadError> {
    info!("Sending pkg file request to url: {}", url);

    let client = ClientBuilder::default()
        // Sony has funky certificates, so this needs to be enabled.
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(DownloadError::Reqwest)?
    ;

    let response = client.get(url)
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

    Ok((file_name, response))
}

pub async fn create_pkg_file(path: PathBuf) -> Result<File, DownloadError> {
    info!("Creating file for pkg at path {:?}", path);

    match fs::create_dir_all(&path.parent().unwrap()).await {
        Ok(_) => info!("Created directory for updates"),
        Err(e) => {
            match e.kind() {
                io::ErrorKind::AlreadyExists => {},
                _ => return Err(DownloadError::Tokio(e))
            }
        }
    }

    let mut options = OpenOptions::default();
        
    options.create(true);
    options.read(true);
    options.write(true);
    options.open(path).await.map_err(DownloadError::Tokio)
}

pub async fn hash_file(file: &mut File, hash: &str) -> Result<bool, DownloadError> {
    let mut buf = Vec::new();
    let mut hasher = Sha1::new();

    file.seek(SeekFrom::Start(0)).await.map_err(DownloadError::Tokio)?;

    if file.read_to_end(&mut buf).await.map_err(DownloadError::Tokio)? < 0x20 {
        return Ok(false);
    }
    
    // Last 0x20 bytes are the SHA1 hash.
    hasher.update(&buf[0..buf.len() - 0x20]);

    Ok(hasher.digest().to_string() == hash)
}
