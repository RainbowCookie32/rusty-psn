use std::fmt;

#[derive(Debug)]
pub enum DownloadError {
    HashMismatch,
    Io(tokio::io::Error),
    Reqwest(reqwest::Error)
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DownloadError::HashMismatch => write!(f, "sha1 hash mismatch"),
            DownloadError::Io(e) => write!(f, "io error: {}", e.to_string()),
            DownloadError::Reqwest(e) => write!(f, "reqwest error: {}", e.to_string())
        }
    }
}
