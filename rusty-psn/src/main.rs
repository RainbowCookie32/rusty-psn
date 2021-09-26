mod error;

use std::io;

#[tokio::main]
async fn main() {
    let mut serial = String::new();

    println!("Enter your game's serial:");
    io::stdin().read_line(&mut serial).unwrap();

    let result = libupdates::get_updates(serial.trim()).await;

    if let Ok(data) = result {
        let title_id = data.get_title_id();
        let title_tag = data.get_update_tag();

        if let Err(e) = std::fs::create_dir_all(&title_id) {
            match e.kind() {
                io::ErrorKind::AlreadyExists => {},
                _ => panic!("Failed to create output directory for updates: {}", e.to_string())
            }
        }

        println!();

        for patch in title_tag.get_packages() {
            println!("Downloading {} - {} ({})\n    {}\n", title_id, patch.get_version(), format_size(patch.get_size()), patch.get_url());
            download_update(&title_id, patch).await.unwrap();
        }
    }
    else if let Err(error) = result {
        println!("Found an error while checking for updates: {}", error);
    }
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

    let mut file = tokio::fs::File::create(format!("{}/{}", serial, filename))
        .await
        .map_err(error::DownloadError::Io)?
    ;

    while let Some(chunk) = response.chunk().await.map_err(error::DownloadError::Reqwest)? {
        let mut chunk = chunk.as_ref();

        tokio::io::copy(&mut chunk, &mut file)
            .await
            .map_err(error::DownloadError::Io)?
        ;
    }

    Ok(())
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
