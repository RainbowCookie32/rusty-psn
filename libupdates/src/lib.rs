pub mod error;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UpdateData {
    titleid: String,
    tag: UpdateTag
}

impl UpdateData {
    pub fn get_title_id(&self) -> String {
        self.titleid.clone()
    }

    pub fn get_update_tag(&self) -> &UpdateTag {
        &self.tag
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateTag {
    name: String,
    package: Vec<Package>
}

impl UpdateTag {
    pub fn get_tag_name(&self) -> String {
        self.name.clone()
    }

    pub fn get_packages(&self) -> &Vec<Package> {
        &self.package
    }
}

#[derive(Debug, Deserialize)]
pub struct Package {
    version: String,
    size: String,
    sha1sum: String,
    url: String
}

impl Package {
    pub fn get_version(&self) -> String {
        self.version.clone()
    }

    pub fn get_size(&self) -> String {
        self.size.clone()
    }

    pub fn get_hash(&self) -> String {
        self.sha1sum.clone()
    }

    pub fn get_url(&self) -> String {
        self.url.clone()
    }
}

pub async fn get_updates<S: AsRef<str>>(serial: S) -> Result<UpdateData, error::PSNError> {
    let serial = serial.as_ref().to_ascii_uppercase();

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true).build()
        .map_err(error::PSNError::ReqwestErr)?
    ;

    let query_url = format!("https://a0.ww.np.dl.playstation.net/tpl/np/{0}/{0}-ver.xml", serial);
    let request = client.get(query_url)
        .build()
        .map_err(error::PSNError::ReqwestErr)?
    ;

    let response = client.execute(request)
        .await
        .map_err(error::PSNError::ReqwestErr)?
    ;

    if response.status().as_u16() == 404 {
        Err(error::PSNError::NotFound)
    }
    else {
        let response_body = response.text()
            .await
            .map_err(error::PSNError::ReqwestErr)?
        ;

        if !response_body.is_empty() {
            serde_xml_rs::from_str(&response_body)
                .map_err(error::PSNError::SerdeXmlErr)
        }
        else {
            Err(error::PSNError::NoUpdates)
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn no_patches() {
        if let Err(crate::error::PSNError::NoUpdates) = crate::get_updates("BLUS41044").await {
            
        }
        else {
            panic!("Unexpected updates received!")
        }
    }

    #[tokio::test]
    async fn single_patch() {
        if let Ok(patch_data) = crate::get_updates("BCUS98174").await {
            assert!(patch_data.tag.package.len() == 1);
        }
        else {
            panic!("Failed to get patch data");
        }
    }

    #[tokio::test]
    async fn multiple_patches() {
        if let Ok(patch_data) = crate::get_updates("BCUS98232").await {
            assert!(patch_data.tag.package.len() == 9);
        }
        else {
            panic!("Failed to get patch data");
        }
    }
}
