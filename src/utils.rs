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

    // Using OpenOptions to avoid the file getting truncated if it already exists
    // .create(true) preserves an existing file's contents.
    OpenOptions::default()
        .create(true)
        .read(true)
        .write(true)
        .open(path).await.map_err(DownloadError::Tokio)
}

pub async fn hash_file(file: &mut File, hash: &str) -> Result<bool, DownloadError> {
    let mut buf = Vec::new();
    let mut hasher = Sha1::new();

    // Write operations during the download move the internal seek pointer.
    // Resetting it to 0 makes .read_to_end actually read the whole thing.
    file.seek(SeekFrom::Start(0)).await.map_err(DownloadError::Tokio)?;

    // If the amount of data read is below the length of the embedded sha1-hash,
    // don't bother hashing the contents. Download's borked.
    if file.read_to_end(&mut buf).await.map_err(DownloadError::Tokio)? < 0x20 {
        return Ok(false);
    }
    
    // Last 0x20 bytes are the SHA1 hash.
    hasher.update(&buf[0..buf.len() - 0x20]);
    Ok(hasher.digest().to_string() == hash)
}
