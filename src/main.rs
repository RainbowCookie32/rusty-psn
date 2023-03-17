// This hides the console window that's created on Windows.
// This is a win for egui users, but it breaks CLI mode in ugly not-very-intuitive ways.
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
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

        if cfg!(windows) {
            print!("hi. this is an ugly window that shouldn't be here (and wasn't here in previous versions) but if I try to nuke it, cli mode breaks ");
            println!("so the compromise is you getting a cmd window. if you launched this from cmd and wanted to have a cmd window open, then don't mind me. have fun :)");
        }

        eframe::run_native(
            "rusty-psn",
            eframe::NativeOptions::default(),
            Box::new(|cc| Box::new(egui::UpdatesApp::new(cc)))
        ).expect("Failed to run egui app");
    }
}
