// On release builds, this hides the console window that's created on Windows.
#![cfg_attr(all(not(debug_assertions), feature = "egui"), windows_subsystem = "windows")]

#[cfg(feature = "cli")]
use std::path::PathBuf;
use flexi_logger::Logger;
use clap::Parser;

#[macro_use] extern crate log;
mod psn;
mod utils;
#[cfg(feature = "cli")]
mod cli;
#[cfg(feature = "egui")]
mod egui;

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Args {
    #[cfg(feature = "cli")]
    #[clap(short, long, required = true, help = "The serial(s) you want to search for, in quotes and separated by spaces")]
    titles: Vec<String>,
    #[cfg(feature = "cli")]
    #[clap(short, long, help = "Downloads all available updates printing only errors, without needing user intervention.")]
    silent: bool,
    #[cfg(feature = "cli")]
    #[clap(short, long, help = "Target folder to save the downloaded update files to.")]
    destination_path: Option<PathBuf>,
    #[clap(long, help = "Disables writing the program's log to a .log file. Don't use if you need help.")]
    no_log_file: bool
}

fn main() {
    let args = Args::parse();

    let mut logger = Logger::try_with_str("info")
        .expect("Failed to create logger");

    if args.no_log_file {
        logger = logger.do_not_log();
    } else {
        logger = logger.log_to_file(flexi_logger::FileSpec::default());
    }

    logger
        .duplicate_to_stdout(flexi_logger::Duplicate::Error)
        .start()
        .expect("Failed to start logger!");

    #[cfg(feature = "cli")]
    {
        info!("starting cli app");
        cli::start_app(args);
    }
    
    #[cfg(feature = "egui")]
    {
        info!("starting egui app");

        eframe::run_native(
            "rusty-psn",
            eframe::NativeOptions::default(),
            Box::new(|cc| Ok(Box::new(egui::UpdatesApp::new(cc))))
        ).expect("Failed to run egui app");
    }
}
