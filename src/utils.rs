use std::convert::TryInto;
use std::path::{Path, PathBuf};

use sha1_smol::Sha1;

use tokio::fs;
use tokio::fs::{File, OpenOptions};

use tokio::io::{self, AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom};

use crate::psn::DownloadError;

#[cfg(target_family = "windows")]
const INVALID_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

#[cfg(target_family = "unix")]
const INVALID_CHARS: [char; 1] = ['/'];

fn sanitize_title(title: &str) -> String {
    //replace invalid characters with underscores or anything we want lol
    title.replace(|c| INVALID_CHARS.contains(&c), "_")
}

fn create_old_pkg_path<P>(download_path: P, serial: &str) -> PathBuf
where
    P: AsRef<Path>,
{
    download_path.as_ref().join(serial)
}

pub fn create_new_pkg_path<P>(download_path: &P, serial: &str, title: &str) -> PathBuf
where
    P: AsRef<Path>,
{
    let target_path = download_path.as_ref();
    let sanitized_title = sanitize_title(title);
    target_path.join(format!("{} - {}", serial, sanitized_title))
}

pub async fn create_pkg_file(
    download_path: PathBuf,
    serial: &str,
    title: &str,
    pkg_name: &str,
) -> Result<File, DownloadError> {
    let mut target_path = create_new_pkg_path(&download_path, serial, title);

    // Check for the old path format.
    let old_path = create_old_pkg_path(&download_path, serial);
    if old_path.exists() {
        info!("Found a folder with the old name format, trying to rename to current one.");

        if let Err(e) = fs::rename(&old_path, &target_path).await {
            error!("Failed to rename folder: {e}");
        }
    }

    target_path.push(pkg_name);
    info!("Creating file for pkg at path {:?}", target_path);

    if let Some(parent) = target_path.parent() {
        match fs::create_dir_all(parent).await {
            Ok(_) => info!("Created directory for updates"),
            Err(e) => match e.kind() {
                io::ErrorKind::AlreadyExists => {}
                _ => return Err(DownloadError::Tokio(e)),
            },
        }
    } else {
        return Err(DownloadError::Tokio(io::Error::new(
            io::ErrorKind::Other,
            "Target path has no parent directory",
        )));
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

const CHUNK_SIZE: usize = 1024 * 1024 * 128;
pub async fn hash_file(
    file: &mut File,
    hash: &str,
    hash_whole_file: bool,
) -> Result<bool, DownloadError> {
    let mut hasher = Sha1::new();

    // Last 0x20 bytes are the SHA1 hash for PS3 updates. PS4 updates don't include hash suffix.
    let suffix_size = if hash_whole_file { 0 } else { 0x20 };

    // If the file size is below the length of the embedded sha1-hash suffix,
    // don't bother hashing the contents. Download's borked.
    let file_length = file.metadata().await.map_err(DownloadError::Tokio)?.len();
    if file_length <= suffix_size {
        return Ok(false);
    }

    let file_length_without_suffix: usize = (file_length - suffix_size)
        .try_into()
        .map_err(|_| DownloadError::HashMismatch(true))?;

    // Write operations during the download move the internal seek pointer.
    // Resetting it to 0 makes reader actually read the whole thing.
    file.seek(SeekFrom::Start(0))
        .await
        .map_err(DownloadError::Tokio)?;

    let mut reader = BufReader::with_capacity(CHUNK_SIZE, file);
    let mut processed_length = 0;
    loop {
        let chunk_buffer = reader.fill_buf().await.map_err(DownloadError::Tokio)?;
        let chunk_length = chunk_buffer.len();
        if chunk_length == 0 {
            break;
        }

        let previously_processed_length: usize = processed_length;
        processed_length += chunk_length;
        // While iterating through the file a chunk being processed may already include some hash suffix bits which should not be hashed.
        // In such case file chunk is stripped of those extra suffix bits.
        let suffix_part_in_chunk = processed_length > file_length_without_suffix;
        let hashable_buffer = if suffix_part_in_chunk {
            let last_before_suffix = file_length_without_suffix
                .checked_sub(previously_processed_length)
                .ok_or_else(|| DownloadError::HashMismatch(true))?;
            &chunk_buffer[..last_before_suffix]
        } else {
            chunk_buffer
        };

        hasher.update(hashable_buffer);
        reader.consume(chunk_length);
        if suffix_part_in_chunk {
            break; // Since unhashable suffix has already been encountered, either in part or in full, there's no need to read rest of the file anymore.
        }
    }

    Ok(hasher.digest().to_string() == hash)
}
