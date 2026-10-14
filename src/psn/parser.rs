use quick_xml::events::Event;
use quick_xml::Reader;

use super::{PackageInfo, UpdateInfo};

#[derive(Debug)]
pub enum ParseError {
    ErrorCode(String),
    XmlParsing(quick_xml::Error),
    XmlEncodingError(quick_xml::encoding::EncodingError),
    InvalidTitleId(String),
}

pub fn parse_response(response: String, info: &mut UpdateInfo) -> Result<(), ParseError> {
    let mut reader = Reader::from_str(&response);
    reader.config_mut().trim_text(true);

    let mut depth = 0;
    let mut title_element = false;
    let mut event_buf = Vec::new();

    let mut err_encountered = false;
    let mut err_code_encountered = false;

    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(e)) => {
                depth += 1;

                match e.name().as_ref() {
                    b"titlepatch" => {
                        for attribute in e.attributes().filter_map(|a| a.ok()) {
                            if attribute.key.as_ref() == b"titleid" {
                                if let Ok(value) = attribute.unescape_value() {
                                    // The title id becomes part of the download folder's name, so it must not
                                    // be able to point anywhere else.
                                    if value.is_empty() || !value.chars().all(|c| c.is_ascii_alphanumeric()) {
                                        return Err(ParseError::InvalidTitleId(value.to_string()));
                                    }
                                    info.title_id = value.to_string();
                                }
                            }
                        }
                    }
                    b"tag" => {
                        for attribute in e.attributes().filter_map(|a| a.ok()) {
                            if attribute.key.as_ref() == b"name" {
                                if let Ok(value) = attribute.unescape_value() {
                                    info.tag_name = value.to_string();
                                }
                            }
                        }
                    }
                    b"package" => {
                        for attribute in e.attributes().filter_map(|a| a.ok()) {
                            match attribute.key.as_ref() {
                                b"version" => {
                                    let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;

                                    let mut package = PackageInfo::empty();
                                    package.version = value.to_string();

                                    info.packages.push(package);
                                }
                                b"size" => {
                                    if let Some(last) = info.packages.last_mut() {
                                        let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;
                                        let parsed_value = value.parse::<u64>().unwrap_or_default();

                                        last.size = parsed_value;
                                    }
                                }
                                b"sha1sum" => {
                                    if let Some(last) = info.packages.last_mut() {
                                        let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;
                                        last.sha1sum = value.to_string();
                                    }
                                }
                                b"url" => {
                                    if let Some(last) = info.packages.last_mut() {
                                        let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;
                                        last.url = value.to_string();
                                    }
                                }
                                b"manifest_url" => {
                                    if let Some(last) = info.packages.last_mut() {
                                        let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;
                                        last.manifest_url = value.to_string();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    b"Error" => {
                        err_encountered = true;
                    }
                    b"Code" => {
                        if !err_encountered {
                            warn!("Code tag encountered without a preceeding Error tag, skipping it");
                            continue;
                        }

                        err_code_encountered = true;
                    }
                    _ => {
                        let name = e.name();
                        let name = String::from_utf8_lossy(name.as_ref());

                        if name.to_lowercase().starts_with("title") {
                            title_element = true;
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                depth -= 1;
            }
            Ok(Event::Empty(e)) => {
                if let b"package" = e.name().as_ref() {
                    for attribute in e.attributes().filter_map(|a| a.ok()) {
                        match attribute.key.as_ref() {
                            b"version" => {
                                let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;

                                let mut package = PackageInfo::empty();
                                package.version = value.to_string();

                                info.packages.push(package);
                            }
                            b"size" => {
                                if let Some(last) = info.packages.last_mut() {
                                    let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;
                                    let parsed_value = value.parse::<u64>().unwrap_or_default();

                                    last.size = parsed_value;
                                }
                            }
                            b"sha1sum" => {
                                if let Some(last) = info.packages.last_mut() {
                                    let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;
                                    last.sha1sum = value.to_string();
                                }
                            }
                            b"url" => {
                                if let Some(last) = info.packages.last_mut() {
                                    let value = attribute.unescape_value().map_err(ParseError::XmlParsing)?;
                                    last.url = value.to_string();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if title_element {
                    let title = e.decode().map_err(ParseError::XmlEncodingError)?;

                    title_element = false;
                    info.titles.push(title.to_string());
                } else if err_code_encountered {
                    let err_code_text = e.decode().map_err(ParseError::XmlEncodingError)?;
                    return Err(ParseError::ErrorCode(err_code_text.into()));
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
    }

    if err_encountered {
        warn!("Error tag encountered without a following Code tag");
    }

    if depth != 0 {
        warn!("Finished parsing xml with non-zero depth {depth}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psn::PlaformVariant;

    #[test]
    fn parse_response_rejects_unsafe_title_id() {
        // The title id in the reply becomes a folder name, so it must not be able to leave the download folder.
        let xml = r#"<titlepatch titleid="../../evil"><tag name="x"><package version="01.01" size="1" sha1sum="a" url="https://a0.ww.np.dl.playstation.net/x.pkg"/></tag></titlepatch>"#;
        let mut info = UpdateInfo::empty(PlaformVariant::PS3);
        assert!(parse_response(xml.to_string(), &mut info).is_err());
    }
}
