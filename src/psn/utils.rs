use crate::psn::UpdateError;

use core::str;
use std::fmt;

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlaformVariant {
    PS3,
    PS4
}

impl fmt::Display for PlaformVariant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub fn get_platform_variant(title_id: &str) -> Option<PlaformVariant> {
    if ["NP", "BL", "BC"].iter().any(|&prefix| { title_id.starts_with(prefix) }) {
        return Some(PlaformVariant::PS3);
    }

    if title_id.starts_with("CUSA") {
        return Some(PlaformVariant::PS4);
    }

    return None
}

pub fn get_update_info_url(title_id: &str, platform_variant: PlaformVariant) -> Result<String, UpdateError> {
    match platform_variant {
        PlaformVariant::PS3 => {
            Ok(format!("https://a0.ww.np.dl.playstation.net/tpl/np/{0}/{0}-ver.xml", title_id))
        },
        PlaformVariant::PS4 => {
            let key = match hex::decode("AD62E37F905E06BC19593142281C112CEC0E7EC3E97EFDCAEFCDBAAFA6378D84") {
                Ok(key) => key,
                Err(_) => return Err(UpdateError::InvalidSerial),
            };
            let msg = format!("np_{0}", title_id);
            let mut hasher = match HmacSha256::new_from_slice(&key) {
                Ok(hasher) => hasher,
                Err(_) => return Err(UpdateError::InvalidSerial)
            };

            hasher.update(msg.as_ref());
            let hash_bytes = hasher.finalize().into_bytes();

            Ok(format!("https://gs-sec.ww.np.dl.playstation.net/plo/np/{0}/{1:x}/{0}-ver.xml", title_id, hash_bytes))
        }
    }
}