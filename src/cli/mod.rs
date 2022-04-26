use std::io::Write;
use std::path::PathBuf;

use clap::Parser;
use bytesize::ByteSize;
use poll_promise::Promise;

use tokio::runtime::Runtime;
use tokio::io::AsyncWriteExt;

use crossterm::cursor;
use crossterm::terminal;

use crate::psn::PackageInfo;
use crate::utils;
use crate::psn::{DownloadError, UpdateError, UpdateInfo};

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(short, long, required = true, help = "The serial(s) you want to search for, in quotes and separated by spaces")]
    titles: Vec<String>,
    #[clap(short, long, help = "Downloads all available updates printing only errors, without needing user intervention.")]
    silent: bool,
    #[clap(short, long, help = "Target folder to save the downloaded update files to.")]
    destination_path: Option<PathBuf>
}

pub fn start_app() {
    let args = Args::parse();
    let runtime = Runtime::new().unwrap();

    let _guard = runtime.enter();

    let titles = args.titles[0].split(' ');
    let silent_mode = args.silent;
    let destination_path = args.destination_path.unwrap_or_else(|| PathBuf::from("pkgs/"));

    if silent_mode {
        info!("App started in silent mode!");
    }

    let update_info = {
        let mut info = Vec::new();

        let promises = titles
            .into_iter()
            .map(| t | (t.to_string(), Promise::spawn_async(UpdateInfo::get_info(t.to_string()))))
            .collect::<Vec<(String, Promise<Result<UpdateInfo, UpdateError>>)>>()
        ;

        if !silent_mode {
            println!("Searching for updates...\n");
        }

        for (id, promise) in promises {
            info!("Checking in on search promises");

            match promise.block_and_take() {
                Ok(i) => {
                    info!("Successfully search for updates for {id}");
                    info.push(i);
                }
                Err(e) => {
                    match e {
                        UpdateError::Serde => {
                            error!("Failed to deserialize response for {id}");
                            println!("{id}: Error parsing response from Sony, try again later.");
                        }
                        UpdateError::InvalidSerial => {
                            error!("Invalid serial for updates query {id}");
                            println!("{id}: The provided serial didn't give any results, double-check your input.");
                        }
                        UpdateError::NoUpdatesAvailable => {
                            warn!("No updates available for serial {id}");
                            println!("{id}: The provided serial doesn't have any available updates.");
                        }
                        UpdateError::Reqwest(e) => {
                            error!("reqwest error on updates query: {e}");
                            println!("{id}: There was an error on the request: {e}.");
                        }
                    }
                }
            }
        }

        info
    };

    for update in update_info {
        let title = {
            if let Some(last_pkg) = update.tag.packages.last() {
                if let Some(param) = last_pkg.paramsfo.as_ref() {
                    param.titles[0].clone()
                }
                else {
                    String::new()
                }
            }
            else {
                String::new()
            }
        };

        if !silent_mode {
            crossterm::execute!(std::io::stdout(), terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0)).unwrap();

            let update_text = {
                let total_size = {
                    let mut total = 0;
    
                    for pkg in update.tag.packages.iter() {
                        total += pkg.size.parse::<u64>().unwrap_or(0);
                    }
    
                    ByteSize::b(total)
                };
    
                let pkgs = update.tag.packages.len();
                let updates_count = format!("{} update{}", pkgs, if pkgs > 1 {"s"} else {""});

                if title.is_empty() {
                    format!("{} - {} ({})", update.title_id, updates_count, total_size)
                }
                else {
                    format!("{} - {} - {} ({})", update.title_id, &title, updates_count, total_size)
                }
            };
    
            println!("{}", update_text);

            for (i, pkg) in update.tag.packages.iter().enumerate() {
                println!("  {i}. {} ({})", pkg.version, ByteSize::b(pkg.size.parse().unwrap_or(0)));
            }
        }

        let mut response = String::new();
        let mut updates_to_fetch = Vec::new();

        if !silent_mode {
            info!("Querying user for wanted updates for {}", update.title_id);
            println!("\nEnter the updates you want to download, separated by a space (ie: 1 3 4 5). An empty input will download all updates.");
            
            std::io::stdin().read_line(&mut response).unwrap();
            response = response.trim().to_string();

            info!("User input was '{}'", response);

            if !response.is_empty() {
                updates_to_fetch = response.split(' ')
                    .filter_map(| s | s.parse::<usize>().ok())
                    .filter(| idx | *idx < update.tag.packages.len())
                    .collect()
                ;

                updates_to_fetch.sort_unstable();
                updates_to_fetch.dedup();
            }

            let updates = {
                let mut updates = String::new();

                if updates_to_fetch.is_empty() {
                    for (i, pkg) in update.tag.packages.iter().enumerate() {
                        updates.push_str(&pkg.version);
    
                        if i < update.tag.packages.len() - 1 {
                            updates.push_str(", ");
                        }
                    }
                }
                else {
                    for (i, update_idx) in updates_to_fetch.iter().enumerate() {
                        updates.push_str(&update.tag.packages[*update_idx].version.to_string());
    
                        if i < updates_to_fetch.len() - 1 {
                            updates.push_str(", ");
                        }
                    }
                }

                updates
            };

            info!("Downloading updates {updates}");

            crossterm::execute!(std::io::stdout(), terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0)).unwrap();
            println!("{}{} - Downloading update(s): {}", update.title_id, if !title.is_empty() { format!(" ({title})") } else {String::new()}, updates);
        }
        
        for (idx, pkg) in update.tag.packages.iter().enumerate() {
            if !updates_to_fetch.is_empty() && !updates_to_fetch.contains(&idx) {
                continue;
            }

            let promise = Promise::spawn_async(download_pkg(destination_path.clone(), title.clone(), update.title_id.clone(), pkg.clone(), silent_mode));

            if let Err(e) = promise.block_and_take() {
                match e {
                    DownloadError::HashMismatch => {
                        error!("Download of {} {} failed: hash mismatch", update.title_id, pkg.version);
                        println!("Error downloading update: hash mismatch on downloaded file.")
                    }
                    DownloadError::Tokio(e) => {
                        error!("Download of {} {} failed: {e}", update.title_id, pkg.version);
                        println!("Error downloading update: {e}.")
                    }
                    DownloadError::Reqwest(e) => {
                        error!("Download of {} {} failed: {e}", update.title_id, pkg.version);
                        println!("Error downloading update: {e}.")
                    }
                }
            }
            else {
                info!("Download of {} {} completed successfully", update.title_id, pkg.version);
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(3));
        
        if !silent_mode {
            crossterm::execute!(std::io::stdout(), terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0)).unwrap();
        }
    }
}

async fn download_pkg(mut pkg_path: PathBuf, title: String, serial: String, pkg: PackageInfo, silent_mode: bool) -> Result<(), DownloadError> {
    let pkg_id = title.clone();
    let pkg_url = pkg.url.clone();
    let pkg_size = pkg.size.parse::<u64>().unwrap_or(0);
    let pkg_hash = pkg.sha1sum.clone();
    let pkg_version = pkg.version.clone();

    let mut stdout = std::io::stdout();

    let (file_name, mut response) = utils::send_pkg_request(pkg_url).await?;
    pkg_path.push(format!("{}/{}", serial, file_name));

    if !silent_mode {
        crossterm::execute!(stdout, cursor::SavePosition).unwrap();

        if pkg_path.exists() {
            print!("    {pkg_version} - {title} | File already exists, verifying checksum... ");
            stdout.flush().unwrap();
        }
    }

    let mut file = utils::create_pkg_file(pkg_path.clone()).await?;
    
    if !utils::hash_file(&mut file, &pkg_hash).await? {
        let mut downloaded = 0;

        file.set_len(0).await.map_err(DownloadError::Tokio)?;

        while let Some(download_chunk) = response.chunk().await.map_err(DownloadError::Reqwest)? {
            let download_chunk = download_chunk.as_ref();

            info!("Received a {} bytes chunk for {pkg_id} {pkg_version}", download_chunk.len());
            downloaded += download_chunk.len() as u64;

            if !silent_mode {
                crossterm::execute!(stdout, cursor::RestorePosition, terminal::Clear(terminal::ClearType::CurrentLine), cursor::SavePosition).unwrap();
                print!("    {pkg_version} - {title} | {} / {}", ByteSize::b(downloaded), ByteSize::b(pkg_size));
                stdout.flush().unwrap();
            }
            
            file.write_all(download_chunk).await.map_err(DownloadError::Tokio)?;
        }

        info!("No more chunks available, hashing received file for {pkg_id} {pkg_version}");

        if !silent_mode {
            println!();
            print!("        {pkg_version} - {title} | Download completed, verifying checksum... ");
            stdout.flush().unwrap();
        }

        if utils::hash_file(&mut file, &pkg_hash).await? {
            info!("Hash for {pkg_id} {pkg_version} matched, wrapping up...");

            if !silent_mode {
                println!("ok");
            }
            
            Ok(())
        }
        else {
            error!("Hash mismatch for {pkg_id} {pkg_version}!");

            if !silent_mode {
                println!("error");
            }

            Err(DownloadError::HashMismatch)
        }
    }
    else {
        info!("File for {pkg_id} {pkg_version} already existed and was complete, wrapping up...");

        if !silent_mode {
            crossterm::execute!(stdout, cursor::RestorePosition, terminal::Clear(terminal::ClearType::CurrentLine), cursor::SavePosition).unwrap();
            println!("    {pkg_version} - {title} | Already downloaded and verified, skipping...");
            stdout.flush().unwrap();
        }
        
        Ok(())
    }
}
