use std::fmt;

#[derive(Debug)]
pub enum DownloadError {
    Io(tokio::io::Error),
    Reqwest(reqwest::Error)
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DownloadError::Io(e) => write!(f, "io error: {}", e.to_string()),
            DownloadError::Reqwest(e) => write!(f, "reqwest error: {}", e.to_string())
        }
    }
}
