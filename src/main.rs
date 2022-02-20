mod error;

use std::io::{self, Write};

use crossterm::cursor::{MoveTo, MoveToColumn};
use crossterm::terminal::{Clear, ClearType};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut serial = String::new();

    println!("Enter your game's serial (e.g. BLUS30035):");
    io::stdin().read_line(&mut serial).unwrap();
    
    serial = serial.trim().to_string();
    serial.make_ascii_uppercase();

    println!("Searching for updates...");

    let result = libupdates::get_updates(serial).await;

    if let Ok(data) = result {
        crossterm::execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0)).unwrap();

        let title = {
            if let Some(pkg) = data.get_update_tag().get_packages().last() {
                if let Some(paramsfo) = pkg.get_paramsfo() {
                    paramsfo.get_titles()[0].clone()
                }
                else {
                    String::new()
                }
            }
            else {
                String::new()
            }
        };

        let title_id = data.get_title_id();
        let title_tag = data.get_update_tag();

        if let Err(e) = std::fs::create_dir_all(&title_id) {
            match e.kind() {
                io::ErrorKind::AlreadyExists => {},
                _ => panic!("Failed to create output directory for updates: {}", e.to_string())
            }
        }

        println!("Found {} update(s) for {} ({})\n", title_tag.get_packages().len(), &title, &title_id);

        for patch in title_tag.get_packages() {
            println!("Downloading {} - {} ({})\n    {}", title_id, patch.get_version(), format_size(patch.get_size()), patch.get_url());
            download_update(&title_id, patch).await.unwrap();
        }
    }
    else if let Err(error) = result {
        println!("Found an error while checking for updates: {}", error);
    }

    println!("All done! Press Enter to exit.");
    io::stdin().read_line(&mut String::new()).unwrap();
}

async fn download_update(serial: &str, package: &libupdates::Package) -> Result<(), error::DownloadError> {
    let url = package.get_url();

    let mut response = reqwest::get(url)
        .await
        .map_err(error::DownloadError::Reqwest)?
    ;
    
    let filename = response
        .url()
        .path_segments()
        .and_then(|s| s.last())
        .and_then(|n| if n.is_empty() { None } else { Some(n.to_string()) })
        .unwrap_or_else(|| String::from("update.pkg"))
    ;

    let path = format!("{}/{}", serial, filename);

    if std::path::Path::new(&path).exists() && check_update(&path, package).await {
        println!();
        return Ok(());
    }

    let mut file = tokio::fs::File::create(&path)
        .await
        .map_err(error::DownloadError::Io)?
    ;

    let mut downloaded = 0;

    while let Some(chunk) = response.chunk().await.map_err(error::DownloadError::Reqwest)? {
        let mut chunk = chunk.as_ref();

        downloaded += chunk.len();
        crossterm::execute!(io::stdout(), Clear(ClearType::CurrentLine), MoveToColumn(0)).unwrap();
        print!("Downloaded {}/{}", format_size(downloaded.to_string()), format_size(package.get_size()));
        io::stdout().flush().unwrap();

        tokio::io::copy(&mut chunk, &mut file)
            .await
            .map_err(error::DownloadError::Io)?
        ;
    }

    println!();

    if check_update(&path, package).await {
        println!();
        Ok(())
    }
    else {
        Err(error::DownloadError::HashMismatch)
    }
}

async fn check_update(path: &str, package: &libupdates::Package) -> bool {
    print!("Verifying download...");
    io::stdout().flush().unwrap();

    if let Ok(file) = tokio::fs::read(path).await {
        // Last 0x20 bytes of the file are the hash itself and isn't
        // a part of the hash provided by the API.
        let file = &file[0..file.len() - 0x20];
        let mut sha1 = sha1::Sha1::default();
        sha1.update(file);

        let pkg_hash = package.get_hash();
        let download_hash = sha1.digest().to_string();

        if pkg_hash == download_hash {
            println!("ok!");
            io::stdout().flush().unwrap();
            
            true
        }
        else {
            println!("fail!");
            io::stdout().flush().unwrap();

            false
        }
    }
    else {
        println!("fail!");
        io::stdout().flush().unwrap();

        false
    }
}

fn format_size(size: String) -> String {
    let mut bytes = size.parse::<u64>().unwrap_or(0);

    if bytes > 1024 {
        bytes /= 1024;

        if bytes < 1024 {
            format!("{}KB", bytes)
        }
        else {
            bytes /= 1024;

            if bytes < 1024 {
                format!("{}MB", bytes)
            }
            else {
                bytes /= 1024;
                
                format!("{}GB", bytes)
            }
        }
    }
    else {
        format!("{}B", bytes)
    }
}
