mod psn;
#[cfg(feature = "cli")]
mod cli;
#[cfg(feature = "egui")]
mod egui;

fn main() {
    #[cfg(feature = "cli")]
    todo!();
    #[cfg(feature = "egui")]
    eframe::run_native(Box::new(egui::UpdatesApp::default()), eframe::NativeOptions::default());
}
