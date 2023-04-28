// On release builds, this hides the console window that's created on Windows.
#![cfg_attr(all(not(debug_assertions), feature = "egui"), windows_subsystem = "windows")]

#[macro_use] extern crate log;
mod psn;
mod utils;
#[cfg(feature = "cli")]
mod cli;
#[cfg(feature = "egui")]
mod egui;

fn main() {
    if let Ok(log_file) = std::fs::File::create("session_log.log") {
        let mut config = simplelog::ConfigBuilder::default();
        config.set_location_level(simplelog::LevelFilter::Error);

        if let Err(e) = simplelog::WriteLogger::init(simplelog::LevelFilter::Info, config.build(), log_file) {
            println!("failed to set up logging: {}", e);
        }
    }

    #[cfg(feature = "cli")]
    {
        info!("starting cli app");
        cli::start_app();
    }
    
    #[cfg(feature = "egui")]
    {
        info!("starting egui app");

        eframe::run_native(
            "rusty-psn",
            eframe::NativeOptions::default(),
            Box::new(|cc| Box::new(egui::UpdatesApp::new(cc)))
        ).expect("Failed to run egui app");
    }
}
