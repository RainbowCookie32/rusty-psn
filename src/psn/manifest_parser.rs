use serde::{Deserialize, Serialize};
use serde_json;

use super::{PackageInfo, UpdateInfo};

#[derive(Serialize, Deserialize)]
struct Piece {
    url: String,
    #[serde(rename = "fileOffset")]
    file_offset: u64,
    #[serde(rename = "fileSize")]
    file_size: u64,
    #[serde(rename = "hashValue")]
    hash_value: String
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    #[serde(rename = "originalFileSize")]
    original_file_size: u64,
    #[serde(rename = "packageDigest")]
    package_digest: String,
    #[serde(rename = "numberOfSplitFiles")]
    number_of_split_files: u32,
    pieces: Vec<Piece>,
}

#[derive(Debug)]
pub enum ParseError {
    NoPartsFound,
    JsonParsing(serde_json::Error),
}

pub fn parse_manifest_response(response: String, parent_manifest_package: &PackageInfo, info: &mut UpdateInfo) -> Result<(), ParseError> {
    let manifest: Manifest = serde_json::from_str(response.as_ref()).map_err(ParseError::JsonParsing)?;
    
    if manifest.pieces.is_empty() {
        return Err(ParseError::NoPartsFound)
    }

    for (idx, piece) in manifest.pieces.iter().enumerate() {
        let mut part_package = PackageInfo::empty();
        let version = if manifest.number_of_split_files > 1 {
            format!("{0} - part {1} of {2}", parent_manifest_package.version, idx+1, manifest.number_of_split_files)
        } else {
            parent_manifest_package.version.to_owned()
        };
        part_package.version = version;
        part_package.sha1sum = piece.hash_value.to_owned();
        part_package.url = piece.url.to_owned();
        part_package.size = piece.file_size; 
        part_package.hash_whole_file = true;
        part_package.offset = piece.file_offset;
        info.packages.push(part_package);
    }

    Ok(())
}