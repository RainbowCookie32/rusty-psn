use std::fmt;

#[derive(Debug)]
pub enum PSNError {
    NotFound,
    NoUpdates,
    ReqwestErr(reqwest::Error),
    SerdeXmlErr(serde_xml_rs::Error)
}

impl fmt::Display for PSNError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PSNError::NotFound => write!(f, "couldn't find this serial"),
            PSNError::NoUpdates => write!(f, "no updates were returned for this serial"),
            PSNError::ReqwestErr(e) => write!(f, "reqwest error: {}", e.to_string()),
            PSNError::SerdeXmlErr(e) => write!(f, "serde-xml-rs error: {}", e.to_string())
        }
    }
}
