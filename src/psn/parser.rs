use quick_xml::events::Event;
use quick_xml::Reader;

use super::{PackageInfo, UpdateInfo};

#[derive(Debug)]
pub enum ParseError {
    ErrorCode(String),
    XmlParsing(quick_xml::Error),
    XmlEncodingError(quick_xml::encoding::EncodingError),
}

pub fn parse_response(response: String, info: &mut UpdateInfo) -> Result<(), ParseError> {
    let mut reader = Reader::from_str(&response);
    // No per-event trimming: a title arrives as several events split at each entity
    // ("Ratchet ", "&amp;", " Clank"), and trimming each piece would drop the spaces.
    reader.config_mut().trim_text(false);

    let mut depth = 0;
    let mut title_element = false;
    let mut title_text = String::new();
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
                            title_text.clear();
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                depth -= 1;

                if title_element {
                    title_element = false;
                    let title = title_text.trim();
                    if !title.is_empty() {
                        info.titles.push(title.to_string());
                    }
                }
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
                    title_text.push_str(&e.decode().map_err(ParseError::XmlEncodingError)?);
                } else if err_code_encountered {
                    let err_code_text = e.decode().map_err(ParseError::XmlEncodingError)?;
                    return Err(ParseError::ErrorCode(err_code_text.trim().to_string()));
                }
            }
            // An entity such as &amp; inside a title arrives as its own event.
            Ok(Event::GeneralRef(e)) => {
                if title_element {
                    if let Ok(Some(c)) = e.resolve_char_ref() {
                        title_text.push(c);
                    } else if let Ok(name) = e.decode() {
                        if let Some(text) = quick_xml::escape::resolve_predefined_entity(&name) {
                            title_text.push_str(text);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ParseError::XmlParsing(e)),
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
    fn parse_response_keeps_a_title_with_an_entity_whole() {
        // The reply for BCUS98124 carries "Ratchet &amp; Clank Future: A Crack In Time".
        let xml = r#"<titlepatch titleid="BCUS98124"><tag name="x"><package version="01.01" size="1" sha1sum="a" url="https://a0.ww.np.dl.playstation.net/x.pkg"><paramsfo><TITLE>Ratchet &amp; Clank Future: A Crack In Time</TITLE></paramsfo></package></tag></titlepatch>"#;
        let mut info = UpdateInfo::empty(PlaformVariant::PS3);
        parse_response(xml.to_string(), &mut info).unwrap();
        assert_eq!(info.titles, vec!["Ratchet & Clank Future: A Crack In Time".to_string()]);
    }

    #[test]
    fn parse_response_returns_reader_errors() {
        // A damaged reply must fail, not give a half-filled update list.
        let xml = r#"<titlepatch titleid="BLUS31147"><tag name="x"><package version="01.01" size="1" sha1sum="a" url="https://a0.ww.np.dl.playstation.net/x.pkg"/></wrong></titlepatch>"#;
        let mut info = UpdateInfo::empty(PlaformVariant::PS3);
        assert!(matches!(
            parse_response(xml.to_string(), &mut info),
            Err(ParseError::XmlParsing(_))
        ));
    }
}
