use std::path::PathBuf;

use sha1_smol::Sha1;

use tokio::fs;
use tokio::fs::{File, OpenOptions};

use tokio::io;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use crate::psn::DownloadError;

fn sanitize_title(title: &str) -> String {
   //replace invalid characters with underscores or anything we want lol
   title.replace(|c: char| !c.is_alphanumeric() && c != ' ' && c != '-', "_")

}

pub async fn create_pkg_file(download_path: PathBuf, serial: &str, title: &str, pkg_name: &str) -> Result<File, DownloadError> {
    let sanitized_title = sanitize_title(title);
    let mut target_path = download_path;
    target_path.push(serial);

    // Check for the old path format.
    if target_path.exists() {
        let old_path = target_path.clone();
        target_path.pop();
        target_path.push(format!("{} - {}", serial, sanitized_title));

        info!("Found a folder with the old name format, trying to rename to current one.");

        if let Err(e) = fs::rename(&old_path, &target_path).await {
            error!("Failed to rename folder: {e}");
        }
    }
    
    target_path.pop();
    target_path.push(format!("{} - {}", serial, sanitized_title));
    target_path.push(pkg_name);

    info!("Creating file for pkg at path {:?}", target_path);

    if let Some(parent) = target_path.parent() {
        match fs::create_dir_all(parent).await {
            Ok(_) => info!("Created directory for updates"),
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::AlreadyExists => {},
                    _ => return Err(DownloadError::Tokio(e)),
                }
            }
        }
    } else {
        return Err(DownloadError::Tokio(io::Error::new(io::ErrorKind::Other, "Target path has no parent directory")));
    }

    // Using OpenOptions to avoid the file getting truncated if it already exists
    // .create(true) preserves an existing file's contents.
    OpenOptions::default()
        .create(true)
        .read(true)
        .write(true)
        .open(target_path)
        .await
        .map_err(DownloadError::Tokio)
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