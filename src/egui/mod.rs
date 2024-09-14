use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;
use egui_notify::{Toast, Toasts, ToastLevel};

use bytesize::ByteSize;
use poll_promise::Promise;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use copypasta::{ClipboardContext, ClipboardProvider};

use tokio::sync::mpsc;
use tokio::runtime::Runtime;

use crate::psn::*;

pub struct ActiveDownload {
    title_id: String,
    pkg_id: String,

    size: u64,
    progress: u64,
    last_received_status: DownloadStatus,

    promise: Promise<Result<(), DownloadError>>,
    progress_rx: mpsc::Receiver<DownloadStatus>
}

#[derive(Clone, Deserialize, Serialize)]
struct AppSettings {
    pkg_download_path: PathBuf,
    show_toasts: bool,
    show_notifications: bool,
}

impl Default for AppSettings {
    fn default() -> AppSettings {
        AppSettings {
            pkg_download_path: PathBuf::from("pkgs/"),
            show_toasts: true,
            show_notifications: false
        }
    }
}

// Values that shouldn't be persisted from run to run.
struct VolatileData {
    rt: Runtime,
    toasts: Toasts,
    
    clipboard: Option<Box<dyn ClipboardProvider>>,

    serial_query: String,
    update_results: Vec<UpdateInfo>,

    show_settings_window: bool,
    show_mismatch_warning_window: bool,

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
            toasts: Toasts::default()
                .reverse(true)
                .with_anchor(egui_notify::Anchor::BottomRight),

            clipboard,

            serial_query: String::new(),
            update_results: Vec::new(),

            show_settings_window: false,
            show_mismatch_warning_window: false,

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

        if self.v.show_settings_window {
            self.draw_settings_window(ctx);
        }

        if self.v.show_mismatch_warning_window {
            self.draw_hash_mismatch_window(ctx);
        }

        let mut toasts = Vec::new();

        // Check the status of the search promise.
        self.handle_search_promise(&mut toasts);
        // Check in on active downloads.
        self.handle_download_promises(&mut toasts);

        for (msg, level) in toasts {
            self.show_notifications(msg, level);
        }

        ctx.request_repaint();
        self.v.toasts.show(ctx);
    }
}

impl UpdatesApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();

        fonts.font_data.insert(
            "noto".to_owned(),
            egui::FontData::from_static(include_bytes!("../../resources/NotoSans-Regular.ttf"))
        );

        fonts.font_data.insert(
            "notojp".to_owned(),
            egui::FontData::from_static(include_bytes!("../../resources/NotoSansJP-Regular.otf"))
        );

        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "noto".to_owned());
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(1, "notojp".to_owned());

        cc.egui_ctx.set_fonts(fonts);

        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        }
        else {
            Default::default()
        }
    }

    fn handle_search_promise(&mut self, toasts: &mut Vec<(String, ToastLevel)>) -> Option<()> {
        let is_ready = {
            let promise = self.v.search_promise.as_ref()?;
            promise.ready().is_some()
        };

        if is_ready {
            let promise = self.v.search_promise.take()?;
            let promise_ready = promise.block_and_take();

            match promise_ready {
                Ok(update_info) => {
                    info!("Received search results for serial {}", update_info.title_id);
                    self.v.update_results.push(update_info);
                }
                Err(ref e) => {
                    match e {
                        UpdateError::UnhandledErrorResponse(e) => {
                            toasts.push((format!("Unexpected error received in a response from PSN ({e})."), ToastLevel::Error));
                        }
                        UpdateError::InvalidSerial => {
                            toasts.push((String::from("The provided serial didn't give any results, double-check your input."), ToastLevel::Error));
                        }
                        UpdateError::NoUpdatesAvailable => {
                            toasts.push((String::from("The provided serial doesn't have any available updates."), ToastLevel::Error));
                        }
                        UpdateError::Reqwest(e) => {
                            toasts.push((format!("There was an error completing the request ({e})."), ToastLevel::Error));
                        }
                        UpdateError::XmlParsing(e) => {
                            toasts.push((format!("Error parsing response from Sony, try again later ({e})."), ToastLevel::Error));
                        }
                    }
        
                    error!("Error received from updates query: {:?}", e);
                }
            }
        }

        Some(())
    }

    fn handle_download_promises(&mut self, toasts: &mut Vec<(String, ToastLevel)>) {
        let mut entries_to_remove = Vec::new();

        for (i, download) in self.v.download_queue.iter_mut().enumerate() {
            if let Ok(status) = download.progress_rx.try_recv() {
                if let DownloadStatus::Progress(progress) = status {
                    // info!("Received {progress} bytes for active download ({} {})", download.id, download.version);
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
                        info!("Download completed! ({} {})", &download.title_id, &download.pkg_id);

                        // Add this download to the happy list of successful downloads.
                        toasts.push((format!("{} v{} downloaded successfully!", &download.title_id, &download.pkg_id), ToastLevel::Success));
                        self.v.completed_downloads.push((download.title_id.clone(), download.pkg_id.clone()));
                    }
                    Err(e) => {
                        // Add this download to the sad list of failed downloads and show the error window.
                        self.v.failed_downloads.push((download.title_id.clone(), download.pkg_id.clone()));

                        match e {
                            DownloadError::HashMismatch(short_on_data) => {
                                toasts.push((format!("Failed to download {} v{}: Hash mismatch.", download.title_id, download.pkg_id), ToastLevel::Error));

                                if *short_on_data {
                                    self.v.show_mismatch_warning_window = true;
                                }
                            }
                            DownloadError::Tokio(_) => {
                                toasts.push((format!("Failed to download {} v{}. Check the log for details.", download.title_id, download.pkg_id), ToastLevel::Error));
                            }
                            DownloadError::Reqwest(_) => {
                                toasts.push((format!("Failed to download {} v{}. Check the log for details.", download.title_id, download.pkg_id), ToastLevel::Error));
                            }
                        }

                        error!("Error received from pkg download ({} {}): {:?}", download.title_id, download.pkg_id, e);
                    }
                }
            }
        }

        for index in entries_to_remove.into_iter().rev() {
            self.v.download_queue.remove(index);
        }
    }

    fn start_download(&self, serial: String, title: String, pkg: PackageInfo) -> ActiveDownload {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let id = serial.clone();
        let pkg_id = pkg.id();
        let download_size = pkg.size;
        let download_path = self.settings.pkg_download_path.clone();

        let _guard = self.v.rt.enter();

        let download_promise = Promise::spawn_async(
            async move {
                pkg.start_download(tx, download_path, serial, title).await
            }
        );

        ActiveDownload {
            title_id: id,
            pkg_id,

            size: download_size,
            progress: 0,
            last_received_status: DownloadStatus::Verifying,

            promise: download_promise,
            progress_rx: rx
        }
    }

    fn show_notifications<S: Into<String>>(&mut self, msg: S, level: ToastLevel) {
        let msg = msg.into();

        if self.settings.show_toasts {
            let mut toast = Toast::basic(&msg);
            toast.set_level(level);
            toast.set_duration(Some(Duration::from_secs(10)));

            self.v.toasts.add(toast);
        }
        else {
            info!("A toast was supposed to be showed, but they are disabled.")
        }

        if self.settings.show_notifications {
            let mut notification = Notification::new();
            notification.summary("rusty-psn");
            notification.body(&msg);

            if let Err(e) = notification.show() {
                error!("Failed to show system notification: {e}");
            }
        }
        else {
            info!("System notifications are disabled in settings, not showing.")
        }
    }

    fn draw_search_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(| ui | {
            ui.label("Title Serial:");

            let serial_input = ui.text_edit_singleline(&mut self.v.serial_query);
            let input_submitted = serial_input.lost_focus() && ui.input(| i | i.key_pressed(egui::Key::Enter));

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
                if !input_submitted && !ui.button("Search for updates").clicked() { return; }

                let already_searched = self.v.update_results.iter().any(|e| e.title_id == parse_title_id(&self.v.serial_query));
                if already_searched { 
                    self.show_notifications("Provided title id results already shown", ToastLevel::Info);
                    return;
                }

                info!("Fetching updates for '{}'", self.v.serial_query);

                let _guard = self.v.rt.enter();
                let promise = Promise::spawn_async(UpdateInfo::get_info(self.v.serial_query.clone()));
                
                self.v.search_promise = Some(promise);
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

        let total_updates_size = update.packages.iter()
            .map(| pkg | pkg.size)
            .sum::<u64>()
        ;

        let title_id = &update.title_id;
        let update_count = update.packages.len();
        let platform_variant = update.platform_variant;

        let id = egui::Id::new(format!("pkg_header_{title_id}"));

        egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, false)
            .show_header(ui, | ui | {
                let title = {
                    if let Some(title) = update.titles.get(0) {
                        title.clone()
                    }
                    else {
                        String::new()
                    }
                };

                let collapsing_title = {
                    if !title.is_empty() {
                        format!("[{platform_variant}] {title_id} - {title} ({update_count} update(s) - {} total)", ByteSize::b(total_updates_size))
                    }
                    else {
                        format!("[{platform_variant}] {title_id} ({update_count} update(s) - {} total)", ByteSize::b(total_updates_size))
                    }
                };

                ui.strong(collapsing_title);

                ui.separator();
    
                if ui.button("Download all").clicked() {
                    info!("Downloading all updates for serial {} ({})", title_id, update_count);
    
                    for pkg in update.packages.iter() {
                        // Avoid duplicates by checking if there's already a download for this serial and version on the queue.
                        if !self.v.download_queue.iter().any(| d | &d.title_id == title_id && d.pkg_id == pkg.id()) {
                            info!("Downloading update {} for serial {title_id} (group)", pkg.id());
                            new_downloads.push(self.start_download(title_id.to_string(), title.clone(), pkg.clone()));
                        }
                    }
                }
            })
            .body(| ui | {
                ui.add_space(5.0);

                for pkg in update.packages.iter() {
                    let title = {
                        if let Some(title) = update.titles.get(0) {
                            title.clone()
                        }
                        else {
                            String::new()
                        }
                    };

                    if let Some(download) = self.draw_entry_pkg(ui, pkg, title_id, title) {
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

    fn draw_entry_pkg(&self, ui: &mut egui::Ui, pkg: &PackageInfo, title_id: &str, title: String) -> Option<ActiveDownload> {
        let mut download = None;

        ui.group(| ui | {
            ui.strong(format!("Package Version: {}", pkg.id()));
            ui.label(format!("Size: {}", ByteSize::b(pkg.size)));
            ui.label(format!("SHA-1 hashsum: {}", pkg.sha1sum));
    
            ui.separator();
    
            ui.horizontal(| ui | {
                let existing_download = self.v.download_queue
                    .iter()
                    .find(| d | d.title_id == title_id && d.pkg_id == pkg.id())
                ;
                
                if ui.add_enabled(existing_download.is_none(), egui::Button::new("Download file")).clicked() {
                    info!("Downloading update {} for serial {} (individual)", pkg.version, title_id);
                    download = Some(self.start_download(title_id.to_string(), title, pkg.clone()));
                }
                
                if let Some(download) = existing_download {
                    match download.last_received_status {
                        DownloadStatus::Progress(_) => {
                            let progress = download.progress as f32 / download.size as f32;
                            ui.add(egui::ProgressBar::new(progress).show_percentage());
                        }
                        DownloadStatus::Verifying => {
                            ui.label(egui::RichText::new("Verifying download...").color(egui::Rgba::from_rgb(1.0, 1.0, 0.6)));
                        }
                        _ => {}
                    }
                }
                else if self.v.completed_downloads.iter().any(| (id, pkg_id) | id == title_id && pkg_id == &pkg.id()) {
                    ui.label(egui::RichText::new("Completed").color(egui::Rgba::from_rgb(0.0, 1.0, 0.0)));
                }
                else if self.v.failed_downloads.iter().any(| (id, pkg_id) | id == title_id && pkg_id == &pkg.id()) {
                    ui.label(egui::RichText::new("Failed").color(egui::Rgba::from_rgb(1.0, 0.0, 0.0)));
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

        // Fixed size avoids a bug that makes the window gradually stretch itself vertically for some reason.
        // See https://github.com/RainbowCookie32/rusty-psn/issues/138
        egui::Window::new("Settings").id(egui::Id::new("cfg_win")).open(&mut show_window).fixed_size([220.0, 200.0]).show(ctx, | ui | {
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

            ui.add_space(5.0);

            if ui.checkbox(&mut self.v.modified_settings.show_toasts, "Show in-app toasts").changed() {
                self.v.settings_dirty = true;
            }

            if ui.checkbox(&mut self.v.modified_settings.show_notifications, "Show system notifications").changed() {
                self.v.settings_dirty = true;
            }

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

    fn draw_hash_mismatch_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("File integrity check failed").collapsible(false).fixed_size([550.0, 100.0]).show(ctx, | ui | {
            ui.vertical_centered(| ui | {
                ui.label(egui::RichText::new("The integrity check for a downloaded file failed.").color(egui::Color32::YELLOW).heading());
                ui.label(egui::RichText::new("Considering the file is smaller than expected, it's likely that Sony's servers are being unreliable.").strong());
                ui.label(egui::RichText::new("You should try to download the file again, or wait for a few hours before retrying. Sony's servers should eventually be able handle a complete download.").strong());

                ui.small("fix your shit already sony, it's been years of unreliable downloads.");
            });

            ui.separator();

            ui.vertical_centered(| ui | {
                if ui.button("Close message").clicked() {
                    self.v.show_mismatch_warning_window = false;
                }
            });
        });
    }
}
