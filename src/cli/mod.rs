use std::path::PathBuf;

use dialoguer::{Input, MultiSelect};

use bytesize::ByteSize;
use indicatif::{ProgressBar, ProgressStyle};
use poll_promise::Promise;
use tokio::runtime::Runtime;

use crate::psn::*;
use crate::Args;

pub fn start_app(args: Args) {
    let runtime = Runtime::new().unwrap();

    let _guard = runtime.enter();

    let silent_mode = args.silent;
    let destination_path = args.destination_path.unwrap_or_else(|| PathBuf::from("pkgs/"));

    if silent_mode {
        info!("App started in silent mode!");
    }
    else {
        println!("rusty-psn");
        clearscreen::clear().unwrap_or(());
    }

    let titles: Vec<String>;

    if args.titles.is_empty() {
        if silent_mode {
            error!("rusty-psn is running in silent mode, but no titles were provided. Exiting...");
            return;
        }

        let list: String = Input::new()
            .with_prompt("No title serials were provided, please input them as a space-separated list (for example: BLES01275 BLUS31156 BLES01976).")
            .allow_empty(false)
            .report(false)
            .interact_text()
            .expect("Failed to get list input");

        titles = list.split(' ').map(|s| s.to_string()).collect();
        clearscreen::clear().unwrap_or(());
    }
    else {
        titles = args.titles[0].split(' ').map(|s| s.to_string()).collect();
    }

    let update_info = {
        let mut info = Vec::new();

        let promises = titles
            .into_iter()
            .map(|t| (t.to_string(), Promise::spawn_async(UpdateInfo::get_info(t.to_string()))))
            .collect::<Vec<(String, Promise<Result<UpdateInfo, UpdateError>>)>>();

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
                Err(e) => match e {
                    UpdateError::UnhandledErrorResponse(e) => {
                        error!("Unexpected error received in response from PSN: {e}");
                        println!("{id}: PSN returned an unexpected error: {e}.");
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
                    UpdateError::XmlParsing(e) => {
                        error!("Failed to deserialize response for {id}: {e}");
                        println!("{id}: Error parsing response from PSN, try again later ({e}).");
                    }
                    UpdateError::XmlEncodingError(e) => {
                        error!("Failed to deserialize response for {id}: {e}");
                        println!("{id}: Error parsing response from PSN, try again later ({e}).");
                    }
                    UpdateError::ManifestParsing(e) => {
                        error!("Failed to deserialize manifest response for {id}: {e}");
                        println!("{id}: Error parsing manifest response from PSN, try again later ({e}).");
                    }
                },
            }
        }

        info
    };

    for update in update_info {
        let title = {
            if let Some(title) = update.titles.first() {
                title.clone()
            } else {
                warn!("Failed to get update's title: Last pkg's info didn't contain a title");
                String::from("Untitled")
            }
        };

        let mut response = Vec::new();

        if !silent_mode {
            let total_size = ByteSize::b(
                update.packages
                    .iter()
                    .map(|p| p.size)
                    .sum::<u64>()
            );

            println!(
                "[{}] {} - {} - {} update(s) ({})",
                update.platform_variant,
                update.title_id,
                &title,
                update.packages.len(),
                total_size
            );

            let update_options: Vec<String> = update.packages.iter().map( | pkg | {
                format!("{} ({})", pkg.id(), ByteSize::b(pkg.size))
            }).collect();

            info!("Querying user for wanted updates for {}", update.title_id);
            println!();

            let defaults = vec![true; update_options.len()];
            response = MultiSelect::new()
                .with_prompt("Select the updates to download. Use the arrow keys to navigate, and the Spacebar to select updates. Pressing Enter confirms your selection.")
                .items(&update_options)
                .defaults(&defaults)
                .interact()
                .expect("Failed to get user's response");

            info!("User input was '{:?}', moving on to download...", response);
            println!("\n[{}] {} - {} - Downloading {} update(s).", update.platform_variant, update.title_id, title, response.len());
        }

        for (idx, pkg) in update.packages.iter().enumerate() {
            if !response.is_empty() && !response.contains(&idx) {
                continue;
            }

            let (tx, mut rx) = tokio::sync::mpsc::channel(10);
            let promise = {
                let serial = update.title_id.clone();
                let download_path = destination_path.clone();
                let download_pkg = pkg.clone();
                let download_title = title.clone();

                Promise::spawn_async(async move {
                    download_pkg.start_download(tx, download_path, serial, download_title).await
                })
            };

            let progress = if !silent_mode {
                let style = ProgressStyle::with_template("{prefix}  [{elapsed}] [{wide_bar:.cyan/blue}] {msg} {bytes} ({bytes_per_sec} - {eta} left)")
                    .unwrap()
                    .progress_chars("#>-");

                let progress = ProgressBar::new(pkg.size)
                    .with_prefix(format!("{title} v{} ({})", pkg.version, ByteSize::b(pkg.size)));
                progress.set_style(style);

                Some(progress)
            } else {
                None
            };

            loop {
                match promise.ready() {
                    Some(result) => {
                        if let Err(e) = result {
                            match e {
                                DownloadError::HashMismatch(short_on_data) => {
                                    error!(
                                        "Download of {} {} failed: hash mismatch. (short on data: {})",
                                        update.title_id,
                                        pkg.id(),
                                        short_on_data
                                    );
                                    println!("Error downloading update: hash mismatch on downloaded file.");

                                    if *short_on_data {
                                        println!("The downloaded file is smaller than expected. Please try again later, as Sony's servers can sometimes be unreliable");
                                    }
                                }
                                DownloadError::Tokio(e) => {
                                    error!("Download of {} {} failed: {e}", update.title_id, pkg.id());
                                    println!("Error downloading update: {e}.")
                                }
                                DownloadError::Reqwest(e) => {
                                    error!("Download of {} {} failed: {e}", update.title_id, pkg.id());
                                    println!("Error downloading update: {e}.")
                                }
                            }
                        }

                        break;
                    }
                    None => {
                        if let Ok(status) = rx.try_recv() {
                            match status {
                                DownloadStatus::Progress(bytes) => {
                                    if !silent_mode {
                                        progress.as_ref().unwrap().inc(bytes);
                                        progress.as_ref().unwrap().tick();
                                        progress.as_ref().unwrap().set_message("Download in progress...");
                                    }
                                }
                                DownloadStatus::Verifying => {
                                    if !silent_mode {
                                        progress.as_ref().unwrap().set_message("Verifying download...");
                                    }
                                }
                                DownloadStatus::DownloadSuccess => {
                                    if !silent_mode {
                                        progress.as_ref().unwrap().finish_with_message("Download succeeded.");
                                    }
                                }
                                DownloadStatus::DownloadFailure => {
                                    if !silent_mode {
                                        progress.as_ref().unwrap().abandon_with_message("Download failed.");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}
