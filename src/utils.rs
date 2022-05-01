use std::path::PathBuf;

use sha1_smol::Sha1;

use tokio::fs;
use tokio::fs::{File, OpenOptions};

use tokio::io;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use crate::psn::DownloadError;

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
