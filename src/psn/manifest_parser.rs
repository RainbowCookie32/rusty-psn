use serde::{Deserialize, Serialize};

use super::{PackageInfo, UpdateInfo};

#[derive(Serialize, Deserialize)]
struct Piece {
    url: String,
    #[serde(rename = "fileOffset")]
    file_offset: u64,
    #[serde(rename = "fileSize")]
    file_size: u64,
    #[serde(rename = "hashValue")]
    hash_value: String,
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

pub fn parse_manifest_response(
    response: String,
    parent_manifest_package: &PackageInfo,
    info: &mut UpdateInfo,
) -> Result<(), ParseError> {
    let manifest: Manifest =
        serde_json::from_str(response.as_ref()).map_err(ParseError::JsonParsing)?;

    if manifest.pieces.is_empty() {
        return Err(ParseError::NoPartsFound);
    }

    for (idx, piece) in manifest.pieces.iter().enumerate() {
        let part_number = if manifest.number_of_split_files > 1 {
            Some(idx + 1)
        } else {
            None
        };
        let part_package = PackageInfo {
            version: parent_manifest_package.version.to_owned(),
            sha1sum: piece.hash_value.to_owned(),
            url: piece.url.to_owned(),
            size: piece.file_size,
            hash_whole_file: true,
            offset: piece.file_offset,
            manifest_url: String::new(),
            part_number,
        };
        info.packages.push(part_package);
    }

    Ok(())
}
