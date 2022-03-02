#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
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
        eframe::run_native(Box::new(egui::UpdatesApp::default()), eframe::NativeOptions::default());
    }
}
