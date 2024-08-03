use quick_xml::{Reader, Error};
use quick_xml::events::Event;

use super::{UpdateInfo, PackageInfo};

pub fn parse_response(response: String) -> Result<UpdateInfo, Error> {
    let mut reader = Reader::from_str(&response);
    reader.config_mut().trim_text(true);

    let mut depth = 0;
    let mut title_element = false;
    let mut event_buf = Vec::new();

    let mut response = UpdateInfo::empty();

    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(e)) => {
                depth += 1;

                match e.name().as_ref() {
                    b"titlepatch" => {
                        for attribute in e.attributes().filter_map(| a | a.ok()) {
                            if attribute.key.as_ref() == b"titleid" {
                                if let Ok(value) = attribute.unescape_value() {
                                    response.title_id = value.to_string();
                                }
                            }
                        }
                    }
                    b"tag" => {
                        for attribute in e.attributes().filter_map(| a | a.ok()) {
                            if attribute.key.as_ref() == b"name" {
                                if let Ok(value) = attribute.unescape_value() {
                                    response.tag_name = value.to_string();
                                }
                            }
                        }
                    }
                    b"package" => {
                        for attribute in e.attributes().filter_map(| a | a.ok()) {
                            match attribute.key.as_ref() {
                                b"version" => {
                                    let value = attribute.unescape_value()?;

                                    let mut package = PackageInfo::empty();
                                    package.version = value.to_string();

                                    response.packages.push(package);
                                }
                                b"size" => {
                                    if let Some(last) = response.packages.last_mut() {
                                        let value = attribute.unescape_value()?;
                                        let parsed_value = value.parse::<u64>().unwrap_or_default();

                                        last.size = parsed_value;
                                    }
                                }
                                b"sha1sum" => {
                                    if let Some(last) = response.packages.last_mut() {
                                        let value = attribute.unescape_value()?;
                                        last.sha1sum = value.to_string();
                                    }
                                }
                                b"url" => {
                                    if let Some(last) = response.packages.last_mut() {
                                        let value = attribute.unescape_value()?;
                                        last.url = value.to_string();
                                    }
                                }
                                _ => {

                                }
                            }
                        }
                    }
                    _ => {
                        let name = e.name();
                        let name = String::from_utf8_lossy(name.as_ref());
                        
                        if name.starts_with("TITLE") {
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
                    for attribute in e.attributes().filter_map(| a | a.ok()) {
                        match attribute.key.as_ref() {
                            b"version" => {
                                let value = attribute.unescape_value()?;

                                let mut package = PackageInfo::empty();
                                package.version = value.to_string();

                                response.packages.push(package);
                            }
                            b"size" => {
                                if let Some(last) = response.packages.last_mut() {
                                    let value = attribute.unescape_value()?;
                                    let parsed_value = value.parse::<u64>().unwrap_or_default();

                                    last.size = parsed_value;
                                }
                            }
                            b"sha1sum" => {
                                if let Some(last) = response.packages.last_mut() {
                                    let value = attribute.unescape_value()?;
                                    last.sha1sum = value.to_string();
                                }
                            }
                            b"url" => {
                                if let Some(last) = response.packages.last_mut() {
                                    let value = attribute.unescape_value()?;
                                    last.url = value.to_string();
                                }
                            }
                            _ => {

                            }
                        }
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if title_element {
                    let title = e.unescape()?;
                    
                    title_element = false;
                    response.titles.push(title.to_string());
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
    }

    if depth != 0 {
        warn!("Finished parsing xml with non-zero depth {depth}");
    }

    Ok(response)
}
