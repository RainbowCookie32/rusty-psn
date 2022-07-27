use std::path::PathBuf;

use eframe::egui;
use bytesize::ByteSize;
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use copypasta::{ClipboardContext, ClipboardProvider};

use tokio::sync::mpsc;
use tokio::runtime::Runtime;

use crate::psn::*;

pub struct ActiveDownload {
    id: String,
    version: String,

    size: u64,
    progress: u64,
    // TODO: Can be used to show status of the download on UI.
    last_received_status: DownloadStatus,

    promise: Promise<Result<(), DownloadError>>,
    progress_rx: mpsc::Receiver<DownloadStatus>
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

impl eframe::App for UpdatesApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, | ui | {
            self.draw_search_bar(ui);

            ui.separator();

            self.draw_results_list(ctx, ui);
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
            if let Ok(status) = download.progress_rx.try_recv() {
                if let DownloadStatus::Progress(progress) = status {
                    info!("Received {progress} bytes for active download ({} {})", download.id, download.version);
                    download.progress += progress;
                }

                download.last_received_status = status;
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

        ctx.request_repaint();
    }
}

impl UpdatesApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        }
        else {
            Default::default()
        }
    }

    fn start_download(&self, title_id: String, pkg: PackageInfo) -> ActiveDownload {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let serial = title_id.clone();
        let version = pkg.version.clone();
        let download_size = pkg.size;
        let download_path = self.settings.pkg_download_path.clone();

        let _guard = self.v.rt.enter();

        let download_promise = Promise::spawn_async(
            async move {
                pkg.start_download(tx, serial, download_path).await
            }
        );

        ActiveDownload {
            id: title_id,
            version,

            size: download_size,
            progress: 0,
            last_received_status: DownloadStatus::Verifying,

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

    fn draw_results_list(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let mut new_downloads = Vec::new();

        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, | ui | {
            for update in self.v.update_results.iter() {
                new_downloads.append(&mut self.draw_result_entry(ctx, ui, update));
            }
        });

        for dl in new_downloads {
            self.v.download_queue.push(dl);
        }
    }

    fn draw_result_entry(&self, ctx: &egui::Context, ui: &mut egui::Ui, update: &UpdateInfo) -> Vec<ActiveDownload> {
        let mut new_downloads = Vec::new();

        let total_updates_size = {
            let mut size = 0;

            for pkg in update.tag.packages.iter() {
                size += pkg.size;
            }

            size
        };

        let title_id = &update.title_id;
        let update_count = update.tag.packages.len();

        let id = egui::Id::new(format!("pkg_header_{}", title_id));

        egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, false)
            .show_header(ui, | ui | {
                let collapsing_title = {
                    if let Some(last_pkg) = update.tag.packages.last() {
                        if let Some(param) = last_pkg.paramsfo.as_ref() {
                            format!("{} - {} ({} update(s) - {} total)", title_id, param.titles[0], update_count, ByteSize::b(total_updates_size))
                        }
                        else {
                            title_id.clone()
                        }
                    }
                    else {
                        title_id.clone()
                    }
                };

                ui.strong(collapsing_title);

                ui.separator();
    
                if ui.button("Download all").clicked() {
                    info!("Downloading all updates for serial {} ({})", title_id, update_count);
    
                    for pkg in update.tag.packages.iter() {
                        if !self.v.download_queue.iter().any(| d | &d.id == title_id && d.version == pkg.version) {
                            info!("Downloading update {} for serial {} (group)", pkg.version, title_id);
                            new_downloads.push(self.start_download(title_id.to_string(), pkg.clone()));
                        }
                    }
                }
            })
            .body(| ui | {
                ui.add_space(5.0);

                for pkg in update.tag.packages.iter() {
                    if let Some(download) = self.draw_entry_pkg(ui, pkg, title_id) {
                        new_downloads.push(download);
                    }

                    ui.add_space(5.0);
                }
            })
        ;

        ui.separator();
        ui.add_space(5.0);

        new_downloads
    }

    fn draw_entry_pkg(&self, ui: &mut egui::Ui, pkg: &PackageInfo, title_id: &str) -> Option<ActiveDownload> {
        let mut download = None;

        ui.group(| ui | {
            ui.strong(format!("Package Version: {}", pkg.version));
            ui.label(format!("Size: {}", ByteSize::b(pkg.size)));
            ui.label(format!("SHA-1 hashsum: {}", pkg.sha1sum));
    
            ui.separator();
    
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
                    let progress = download.progress as f32 / download.size as f32;
                    ui.add(egui::ProgressBar::new(progress).show_percentage());
                }
                else if self.v.completed_downloads.iter().any(| (id, version) | id == title_id && version == &pkg.version) {
                    ui.label(egui::RichText::new("Completed").color(egui::color::Rgba::from_rgb(0.0, 1.0, 0.0)));
                }
                else if self.v.failed_downloads.iter().any(| (id, version) | id == title_id && version == &pkg.version) {
                    ui.label(egui::RichText::new("Failed").color(egui::color::Rgba::from_rgb(1.0, 0.0, 0.0)));
                }
            
                let remaining_space = ui.available_size_before_wrap();
                ui.add_space(remaining_space.x);
            });
        });

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
