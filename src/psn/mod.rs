use serde::Deserialize;

pub enum DownloadError {
    HashMismatch,
    Tokio(tokio::io::Error),
    Reqwest(reqwest::Error)
}

#[derive(Debug)]
pub enum UpdateError {
    Serde,
    InvalidSerial,
    NoUpdatesAvailable,
    Reqwest(reqwest::Error)
}

#[derive(Clone, Deserialize)]
pub struct UpdateInfo {
    #[serde(rename = "titleid")]
    pub title_id: String,
    pub tag: UpdateTag
}

impl UpdateInfo {
    pub async fn get_info(title_id: String) -> Result<UpdateInfo, UpdateError> {
        let title_id = title_id.to_uppercase();
        let url = format!("https://a0.ww.np.dl.playstation.net/tpl/np/{0}/{0}-ver.xml", title_id);
        let client = reqwest::ClientBuilder::default()
            // Sony has funky certificates, so this needs to be enabled.
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(UpdateError::Reqwest)?
        ;

        info!("Querying for updates for serial: {}", title_id);
    
        let response = client.get(url).send().await.map_err(UpdateError::Reqwest)?;
        let response_txt = response.text().await.map_err(UpdateError::Reqwest)?;

        if response_txt.is_empty() {
            Err(UpdateError::NoUpdatesAvailable)
        }
        else if response_txt.contains("Not found") {
            Err(UpdateError::InvalidSerial)
        }
        else {
            serde_xml_rs::from_str(&response_txt).map_err(|_| UpdateError::Serde)
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct UpdateTag {
    pub name: String,
    #[serde(rename = "package")]
    pub packages: Vec<PackageInfo>
}

#[derive(Clone, Deserialize)]
pub struct PackageInfo {
    pub url: String,
    pub size: String,
    pub version: String,
    pub sha1sum: String,

    pub paramsfo: Option<ParamSfo>
}

#[derive(Clone, Deserialize)]
pub struct ParamSfo {
    #[serde(rename = "$value")]
    pub titles: Vec<String>
}
