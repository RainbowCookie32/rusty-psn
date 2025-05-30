// On release builds, this hides the console window that's created on Windows.
#![cfg_attr(all(not(debug_assertions), feature = "egui"), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
extern crate dirs;

use clap::Parser;
use flexi_logger::{Logger, LoggerHandle};
#[cfg(feature = "cli")]
use std::path::PathBuf;
use std::sync::Arc;
use tokio::{runtime::Runtime, sync::Notify};

#[macro_use]
extern crate log;
#[cfg(feature = "cli")]
mod cli;
#[cfg(feature = "egui")]
mod egui;
mod psn;
mod utils;

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Args {
    #[cfg(feature = "cli")]
    #[clap(
        short,
        long,
        required = true,
        help = "The serial(s) you want to search for, in quotes and separated by spaces"
    )]
    titles: Vec<String>,
    #[cfg(feature = "cli")]
    #[clap(
        short,
        long,
        help = "Downloads all available updates printing only errors, without needing user intervention."
    )]
    silent: bool,
    #[cfg(feature = "cli")]
    #[clap(short, long, help = "Target folder to save the downloaded update files to.")]
    destination_path: Option<PathBuf>,
    #[clap(
        long,
        help = "Disables writing the program's log to a .log file. Don't use if you need help."
    )]
    no_log_file: bool,
}

fn main() {
    let args = Args::parse();
    let _logger_handle = init_log(args.no_log_file);

    #[cfg(feature = "cli")]
    {
        info!("starting cli app");
        cli::start_app(args);
    }

    #[cfg(feature = "egui")]
    {
        info!("starting egui app");

        // Execute tokio runtime in its own thread.
        // Prevents egui blocking the same thread that tokio runtime is running on,
        // which can lead to network and io tasks being blocked when the application
        // is minimised or otherwise suspended by egui.
        let rt = Runtime::new().unwrap();
        let rt_handle = rt.handle().clone();
        let notify_main = Arc::new(Notify::new());
        let notify_thread = notify_main.clone();
        let rt_thread = std::thread::spawn(move || {
            rt.block_on(async {
                notify_thread.notified().await; // Wait for a shutdown signal.
            })
        });

        eframe::run_native(
            "rusty-psn",
            eframe::NativeOptions::default(),
            Box::new(|cc| Ok(Box::new(egui::UpdatesApp::new(cc, rt_handle)))),
        )
        .expect("Failed to run egui app");

        // Signal runtime on a separate thread to shutdown gracefully.
        notify_main.notify_one();
        let _ = rt_thread.join();
    }
}

#[cfg(target_os = "macos")]
fn init_log(no_log_file: bool) -> LoggerHandle {
    let mut logger = Logger::try_with_str("info").expect("Failed to create logger");

    if no_log_file {
        logger = logger.do_not_log();
    } else {
        let mut logs_dir = dirs::data_local_dir().unwrap();
        logs_dir.push("rusty-psn");

        match std::fs::create_dir_all(&logs_dir) {
            Ok(_) => info!("Created directory for updates"),
            Err(e) => match e.kind() {
                std::io::ErrorKind::AlreadyExists => {}
                _ => panic!("{}", e),
            },
        }

        logger = logger.log_to_file(flexi_logger::FileSpec::default().directory(logs_dir));
    }

    logger
        .duplicate_to_stdout(flexi_logger::Duplicate::Error)
        .start()
        .expect("Failed to start logger!")
}

#[cfg(not(target_os = "macos"))]
fn init_log(no_log_file: bool) -> LoggerHandle {
    let mut logger = Logger::try_with_str("info").expect("Failed to create logger");

    if no_log_file {
        logger = logger.do_not_log();
    } else {
        logger = logger.log_to_file(flexi_logger::FileSpec::default());
    }

    logger
        .duplicate_to_stdout(flexi_logger::Duplicate::Error)
        .start()
        .expect("Failed to start logger!")
}
