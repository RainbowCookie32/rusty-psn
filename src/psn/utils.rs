use crate::psn::UpdateError;

use core::str;
use std::{
    fmt,
    io::{Error, SeekFrom},
    path::PathBuf,
};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::{
    fs::OpenOptions,
    io::{copy_buf, AsyncSeekExt, BufReader, BufWriter},
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlaformVariant {
    PS3,
    PS4,
}

impl fmt::Display for PlaformVariant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub fn get_platform_variant(title_id: &str) -> Option<PlaformVariant> {
    if ["NP", "BL", "BC"]
        .iter()
        .any(|&prefix| title_id.starts_with(prefix))
    {
        return Some(PlaformVariant::PS3);
    }

    if title_id.starts_with("CUSA") {
        return Some(PlaformVariant::PS4);
    }

    return None;
}

pub fn get_update_info_url(
    title_id: &str,
    platform_variant: PlaformVariant,
) -> Result<String, UpdateError> {
    match platform_variant {
        PlaformVariant::PS3 => Ok(format!(
            "https://a0.ww.np.dl.playstation.net/tpl/np/{0}/{0}-ver.xml",
            title_id
        )),
        PlaformVariant::PS4 => {
            let key = match hex::decode(
                "AD62E37F905E06BC19593142281C112CEC0E7EC3E97EFDCAEFCDBAAFA6378D84",
            ) {
                Ok(key) => key,
                Err(_) => return Err(UpdateError::InvalidSerial),
            };
            let msg = format!("np_{0}", title_id);
            let mut hasher = match HmacSha256::new_from_slice(&key) {
                Ok(hasher) => hasher,
                Err(_) => return Err(UpdateError::InvalidSerial),
            };

            hasher.update(msg.as_ref());
            let hash_bytes = hasher.finalize().into_bytes();

            Ok(format!(
                "https://gs-sec.ww.np.dl.playstation.net/plo/np/{0}/{1:x}/{0}-ver.xml",
                title_id, hash_bytes
            ))
        }
    }
}

const MERGE_CHUNK_SIZE: usize = 1024 * 1024 * 128;
pub async fn copy_pkg_file(
    src_path: &PathBuf,
    target_path: &PathBuf,
    offset: u64,
) -> Result<u64, Error> {
    let src_file = OpenOptions::default()
        .create(false)
        .read(true)
        .write(false)
        .open(src_path)
        .await?;

    let mut target_file = OpenOptions::default()
        .create(true)
        .read(false)
        .write(true)
        .open(target_path)
        .await?;

    if offset > 0 {
        target_file.seek(SeekFrom::Start(offset)).await?;
    }

    let mut writer = BufWriter::with_capacity(MERGE_CHUNK_SIZE, target_file);
    let mut reader = BufReader::with_capacity(MERGE_CHUNK_SIZE, src_file);
    let read_bytes = copy_buf(&mut reader, &mut writer).await?;
    Ok(read_bytes)
}
