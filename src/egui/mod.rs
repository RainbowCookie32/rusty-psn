use std::path::PathBuf;

use bytesize::ByteSize;
use eframe::{egui, epi};
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use copypasta::{ClipboardContext, ClipboardProvider};

use tokio::sync::mpsc;
use tokio::runtime::Runtime;
use tokio::io::AsyncWriteExt;

use crate::utils;
use crate::psn::{DownloadError, UpdateError, UpdateInfo, PackageInfo};

pub struct ActiveDownload {
    id: String,
    version: String,

    size: u64,
    progress: u64,

    promise: Promise<Result<(), DownloadError>>,
    progress_rx: mpsc::Receiver<u64>
}

#[derive(Clone, Deserialize, Serialize)]
struct AppSettings {
    pkg_download_path: PathBuf
}

impl Default for AppSettings {
    fn default() -> AppSettings {
        AppSettings {
            pkg_download_path: PathBuf::from("pkgs/")
        }
    }
}

// Values that shouldn't be persisted from run to run.
struct VolatileData {
    rt: Runtime,
    
    clipboard: Option<Box<dyn ClipboardProvider>>,

    serial_query: String,
    update_results: Vec<UpdateInfo>,

    error_msg: String,
    show_error_window: bool,
    show_settings_window: bool,

    settings_dirty: bool,
    modified_settings: AppSettings,

    download_queue: Vec<ActiveDownload>,
    failed_downloads: Vec<(String, String)>,
    completed_downloads: Vec<(String, String)>,

    search_promise: Option<Promise<Result<UpdateInfo, UpdateError>>>
}

impl Default for VolatileData {
    fn default() -> VolatileData {
        let clipboard: Option<Box<dyn ClipboardProvider>> = {
            match ClipboardContext::new() {
                Ok(clip) => Some(Box::new(clip)),
                Err(e) => {
                    error!("Failed to init clipboard: {}", e.to_string());
                    None
                }
            }
        };

        VolatileData {
            rt: Runtime::new().unwrap(),

            clipboard,

            serial_query: String::new(),
            update_results: Vec::new(),

            error_msg: String::new(),
            show_error_window: false,
            show_settings_window: false,

            settings_dirty: false,
            modified_settings: AppSettings::default(),

            download_queue: Vec::new(),
            failed_downloads: Vec::new(),
            completed_downloads: Vec::new(),

            search_promise: None
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
pub struct UpdatesApp {
    #[serde(skip)]
    v: VolatileData,
    settings: AppSettings
}

impl epi::App for UpdatesApp {
    fn name(&self) -> &str {
        "rusty-psn"
    }

    fn save(&mut self, storage: &mut dyn epi::Storage) {
        epi::set_value(storage, epi::APP_KEY, self);
    }

    fn setup(&mut self, _ctx: &egui::Context, _frame: &epi::Frame, storage: Option<&dyn epi::Storage>) {
        if let Some(storage) = storage {
            *self = epi::get_value(storage, epi::APP_KEY).unwrap_or_default()
        }
    }

    fn update(&mut self, ctx: &egui::Context, frame: &epi::Frame) {
        egui::CentralPanel::default().show(ctx, | ui | {
            self.draw_search_bar(ui);

            ui.separator();

            self.draw_results_list(ui);
        });

        if !self.v.error_msg.is_empty() && self.v.show_error_window {
            let label = self.v.error_msg.clone();
            // There was an attempt to properly center it :)
            let position = ctx.available_rect().center();
            let mut acknowledged = false;

            let error_window = egui::Window::new("An error ocurred")
                .collapsible(false)
                .open(&mut self.v.show_error_window)
                .resizable(false)
                .default_pos(position)
            ;

            error_window.show(ctx, | ui | {
                ui.label(label);

                if ui.button("Ok").clicked() {
                    acknowledged = true;
                }
            });

            if acknowledged {
                self.v.show_error_window = false;
                self.v.error_msg = String::new();
            }
        }

        if self.v.show_settings_window {
            self.draw_settings_window(ctx);
        }

        // Go through search promises and handle their results if ready.
        if let Some(promise) = self.v.search_promise.as_ref() {
            if let Some(result) = promise.ready() {
                if let Ok(update_info) = result {
                    info!("Received search results for serial {}", update_info.title_id);
                    self.v.update_results.push(update_info.clone());
                }
                else if let Err(e) = result {
                    self.v.show_error_window = true;

                    match e {
                        UpdateError::Serde => {
                            self.v.error_msg = "Error parsing response from Sony, try again later.".to_string();
                        }
                        UpdateError::InvalidSerial => {
                            self.v.error_msg = "The provided serial didn't give any results, double-check your input.".to_string();
                        }
                        UpdateError::NoUpdatesAvailable => {
                            self.v.error_msg = "The provided serial doesn't have any available updates.".to_string();
                        }
                        UpdateError::Reqwest(e) => {
                            self.v.error_msg = format!("There was an error on the request: {}", e);
                        }
                    }

                    error!("Error received from updates query: {}", self.v.error_msg);
                }
                
                self.v.search_promise = None;
            }
        }

        let mut entries_to_remove = Vec::new();

        // Check in on active downloads.
        for (i, download) in self.v.download_queue.iter_mut().enumerate() {
            // Some new bytes were downloaded, add to the total download progress.
            if let Ok(progress) = download.progress_rx.try_recv() {
                info!("Recieved {progress} bytes for active download ({} {})", download.id, download.version);
                download.progress += progress;
            }

            // Check if the download promise is resolved (finished or failed).
            if let Some(r) = download.promise.ready() {
                // Queue up for removal.
                entries_to_remove.push(i);

                match r {
                    Ok(_) => {
                        // Add this download to the happy list of successful downloads.
                        self.v.completed_downloads.push((download.id.clone(), download.version.clone()));
                        info!("Download completed! ({} {})", download.id, download.version);
                    }
                    Err(e) => {
                        // Add this download to the sad list of failed downloads and show the error window.
                        self.v.show_error_window = true;
                        self.v.failed_downloads.push((download.id.clone(), download.version.clone()));

                        match e {
                            DownloadError::HashMismatch => {
                                self.v.error_msg = format!("There was an error downloading the {} update file for {}: The hash for the downloaded file doesn't match.", download.version, download.id);
                            }
                            DownloadError::Tokio(e) => {
                                self.v.error_msg = format!("There was an error downloading the {} update file for {}: {e}", download.version, download.id);
                            }
                            DownloadError::Reqwest(e) => {
                                self.v.error_msg = format!("There was an error downloading the {} update file for {}: {e}", download.version, download.id);
                            }
                        }

                        error!("Error received from pkg download ({} {}): {}", download.id, download.version, self.v.error_msg);
                    }
                }
            }
        }

        for (removed_entries, entry) in entries_to_remove.into_iter().enumerate() {
            self.v.download_queue.remove(entry - removed_entries);
        }

        frame.request_repaint();
    }
}

impl UpdatesApp {
    fn start_download(&self, title_id: String, pkg: PackageInfo) -> ActiveDownload {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let serial = title_id.clone();
        let version = pkg.version.clone();
        let download_size = pkg.size;
        let base_path = self.settings.pkg_download_path.clone();

        let _guard = self.v.rt.enter();

        let download_promise = Promise::spawn_async(async move {
            let serial = serial;

            info!("Hello from a promise for {serial} {}", pkg.version);

            let tx = tx;
            let pkg = pkg;
            let (file_name, mut response) = utils::send_pkg_request(pkg.url).await?;

            let mut download_path = base_path;
            download_path.push(format!("{serial}/{file_name}"));

            let mut file = utils::create_pkg_file(download_path).await?;

            if !utils::hash_file(&mut file, &pkg.sha1sum).await? {
                file.set_len(0).await.map_err(DownloadError::Tokio)?;

                while let Some(download_chunk) = response.chunk().await.map_err(DownloadError::Reqwest)? {
                    let download_chunk = download_chunk.as_ref();

                    info!("Received a {} bytes chunk for {serial} {}", download_chunk.len(), pkg.version);
    
                    tx.send(download_chunk.len() as u64).await.unwrap();
                    file.write_all(download_chunk).await.map_err(DownloadError::Tokio)?;
                }

                info!("No more chunks available, hashing received file for {serial} {}", pkg.version);
                                                
                if utils::hash_file(&mut file, &pkg.sha1sum).await? {
                    info!("Hash for {serial} {} matched, wrapping up...", pkg.version);
                    Ok(())
                }
                else {
                    error!("Hash mismatch for {serial} {}!", pkg.version);
                    Err(DownloadError::HashMismatch)
                }
            }
            else {
                info!("File for {serial} {} already existed and was complete, wrapping up...", pkg.version);
                tx.send(pkg.size).await.unwrap();

                Ok(())
            }
        });

        ActiveDownload {
            id: title_id,
            version,

            size: download_size,
            progress: 0,

            promise: download_promise,
            progress_rx: rx
        }
    }

    fn draw_search_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(| ui | {
            ui.label("Title Serial:");

            let serial_input = ui.text_edit_singleline(&mut self.v.serial_query);
            let input_submitted = serial_input.lost_focus() && ui.input().key_pressed(egui::Key::Enter);

            serial_input.context_menu(| ui | {
                ui.add_enabled_ui(self.v.clipboard.is_some(), | ui | {
                    if let Some(clip_ctx) = self.v.clipboard.as_mut() {
                        if ui.button("Paste").clicked() {
                            match clip_ctx.get_contents(){
                                Ok(contents) => self.v.serial_query.push_str(&contents),
                                Err(e) => warn!("Failed to paste clipboard contents: {}", e.to_string())
                            }

                            ui.close_menu();
                        }

                        ui.add_enabled_ui(!self.v.serial_query.is_empty(), |ui| {
                            if ui.button("Clear").clicked() {
                                self.v.serial_query = String::new();
                                ui.close_menu();
                            }
                        });
                    }
                });
            });

            ui.separator();
            
            ui.add_enabled_ui(!self.v.serial_query.is_empty() && self.v.search_promise.is_none(), | ui | {
                let already_searched = self.v.update_results.iter().any(|e| e.title_id == self.v.serial_query);

                if (input_submitted || ui.button("Search for updates").clicked()) && !already_searched {
                    info!("Fetching updates for '{}'", self.v.serial_query);

                    let _guard = self.v.rt.enter();
                    let promise = Promise::spawn_async(UpdateInfo::get_info(self.v.serial_query.clone()));
                    
                    self.v.search_promise = Some(promise);
                }
            });

            ui.add_enabled_ui(!self.v.update_results.is_empty(), | ui | {
                if ui.button("Clear results").clicked() {
                    self.v.update_results = Vec::new();
                }
            });

            ui.separator();

            if ui.button("⚙").clicked() {
                self.v.modified_settings = self.settings.clone();
                self.v.show_settings_window = true;
            }
        });
    }

    fn draw_results_list(&mut self, ui: &mut egui::Ui) {
        let mut new_downloads = Vec::new();

        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, | ui | {
            for update in self.v.update_results.iter() {
                new_downloads.append(&mut self.draw_result_entry(ui, update));
            }
        });

        for dl in new_downloads {
            self.v.download_queue.push(dl);
        }
    }

    fn draw_result_entry(&self, ui: &mut egui::Ui, update: &UpdateInfo) -> Vec<ActiveDownload> {
        let mut new_downloads = Vec::new();

        let title_id = &update.title_id;
        let collapsing_title = {
            if let Some(last_pkg) = update.tag.packages.last() {
                if let Some(param) = last_pkg.paramsfo.as_ref() {
                    format!("{} - {}", update.title_id.clone(), param.titles[0])
                }
                else {
                    update.title_id.clone()
                }
            }
            else {
                update.title_id.clone()
            }
        };

        ui.collapsing(collapsing_title, | ui | {
            let total_updates_size = {
                let mut size = 0;

                for pkg in update.tag.packages.iter() {
                    size += pkg.size;
                }

                size
            };

            if ui.button(format!("Download all ({})", ByteSize::b(total_updates_size))).clicked() {
                info!("Downloading all updates for serial {} ({})", title_id, update.tag.packages.len());

                for pkg in update.tag.packages.iter() {
                    if !self.v.download_queue.iter().any(| d | &d.id == title_id && d.version == pkg.version) {
                        info!("Downloading update {} for serial {} (group)", pkg.version, title_id);
                        new_downloads.push(self.start_download(title_id.to_string(), pkg.clone()));
                    }
                }
            }

            ui.separator();

            for pkg in update.tag.packages.iter() {
                if let Some(download) = self.draw_entry_pkg(ui, pkg, title_id) {
                    new_downloads.push(download);
                }
            }
        });

        new_downloads
    }

    fn draw_entry_pkg(&self, ui: &mut egui::Ui, pkg: &PackageInfo, title_id: &str) -> Option<ActiveDownload> {
        let mut download = None;

        let bytes = pkg.size;
                    
        ui.strong(format!("Package Version: {}", pkg.version));
        ui.label(format!("Size: {}", ByteSize::b(bytes)));
        ui.label(format!("SHA-1 hashsum: {}", pkg.sha1sum));

        ui.horizontal(| ui | {
            let existing_download = self.v.download_queue
                .iter()
                .find(| d | d.id == title_id && d.version == pkg.version)
            ;

            if ui.add_enabled(existing_download.is_none(), egui::Button::new("Download file")).clicked() {
                info!("Downloading update {} for serial {} (individual)", pkg.version, title_id);
                download = Some(self.start_download(title_id.to_string(), pkg.clone()));
            }

            if let Some(download) = existing_download {
                let progress = egui::ProgressBar::new(download.progress as f32 / download.size as f32)
                    .show_percentage()
                ;

                ui.add(progress);
            }
            else if self.v.completed_downloads.iter().any(| (id, version) | id == title_id && version == &pkg.version) {
                ui.label(egui::RichText::new("Completed").color(egui::color::Rgba::from_rgb(0.0, 1.0, 0.0)));
            }
            else if self.v.failed_downloads.iter().any(| (id, version) | id == title_id && version == &pkg.version) {
                ui.label(egui::RichText::new("Failed").color(egui::color::Rgba::from_rgb(1.0, 0.0, 0.0)));
            }
        });

        ui.separator();

        download
    }

    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        let mut show_window = self.v.show_settings_window;
        let mut current_download_path = self.v.modified_settings.pkg_download_path.to_string_lossy().to_string();

        egui::Window::new("Setings").open(&mut show_window).resizable(true).show(ctx, | ui | {
            ui.label("Download Path");
            ui.horizontal(| ui | {
                ui.add_enabled_ui(false, | ui | {
                    ui.text_edit_singleline(&mut current_download_path);
                });

                if ui.button("Pick folder").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.v.settings_dirty = true;
                        self.v.modified_settings.pkg_download_path = path;
                    }
                }

                if ui.button("Reset").clicked() {
                    self.v.settings_dirty = true;
                    self.v.modified_settings.pkg_download_path = PathBuf::from("/pkgs");
                }
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::TOP), | ui | {
                ui.horizontal(| ui | {
                    if ui.button("Save settings").clicked() {
                        self.v.settings_dirty = false;
                        self.v.show_settings_window = false;

                        self.settings = self.v.modified_settings.clone();
                    }

                    if ui.add_enabled(self.v.settings_dirty, egui::Button::new("Discard changes")).clicked() {
                        self.v.settings_dirty = false;
                        self.v.show_settings_window = false;

                        self.v.modified_settings = self.settings.clone();
                    }

                    if ui.button("Restore to defaults").clicked() {
                        self.v.settings_dirty = false;
                        self.v.show_settings_window = false;
                        
                        self.settings = AppSettings::default();
                        self.v.modified_settings = AppSettings::default();
                    }
                });

                ui.separator();
            });
        });

        if !show_window {
            self.v.show_settings_window = false;
        }
    }
}
