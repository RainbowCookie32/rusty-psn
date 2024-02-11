// On release builds, this hides the console window that's created on Windows.
#![cfg_attr(all(not(debug_assertions), feature = "egui"), windows_subsystem = "windows")]

use flexi_logger::Logger;

#[macro_use] extern crate log;
mod psn;
mod utils;
#[cfg(feature = "cli")]
mod cli;
#[cfg(feature = "egui")]
mod egui;

fn main() {
    Logger::try_with_str("info")
        .expect("Failed to create logger")
        .log_to_file(flexi_logger::FileSpec::default())
        .duplicate_to_stdout(flexi_logger::Duplicate::Error)
        .start()
        .expect("Failed to start logger!")
    ;

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
