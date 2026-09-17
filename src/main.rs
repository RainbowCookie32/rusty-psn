// On release builds, this hides the console window that's created on Windows.
#![cfg_attr(all(not(debug_assertions), feature = "egui"), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
extern crate dirs;
#[macro_use]
extern crate log;
#[cfg(feature = "cli")]
mod cli;
#[cfg(feature = "egui")]
mod egui;
mod psn;
mod utils;

#[cfg(feature = "cli")]
use clap::Parser;
use flexi_logger::{Logger, LoggerHandle};

#[cfg(feature = "cli")]
#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(
        short,
        long,
        help = "The serial(s) you want to search for, in quotes and separated by spaces"
    )]
    titles: Vec<String>,
    #[clap(
        short,
        long,
        help = "Downloads all available updates printing only errors, without needing user intervention."
    )]
    silent: bool,
    #[clap(short, long, help = "Target folder to save the downloaded update files to.")]
    destination_path: Option<std::path::PathBuf>,
}

fn main() {
    let _logger_handle = init_log();

    #[cfg(feature = "cli")]
    {
        let args = Args::parse();
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
        let rt = tokio::runtime::Runtime::new().unwrap();
        let rt_handle = rt.handle().clone();
        let notify_main = std::sync::Arc::new(tokio::sync::Notify::new());
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

fn init_log() -> LoggerHandle {
    let mut logger = Logger::try_with_str("info")
        .expect("Failed to create logger");

    if cfg!(target_os = "macos") {
        let mut logs_dir = dirs::data_local_dir().unwrap();
        logs_dir.push("rusty-psn");

        match std::fs::create_dir_all(&logs_dir) {
            Ok(_) => info!("Created directory for logs"),
            Err(e) => match e.kind() {
                std::io::ErrorKind::AlreadyExists => {}
                _ => panic!("{}", e),
            },
        }

        logger = logger.log_to_file(flexi_logger::FileSpec::default().directory(logs_dir));
    }
    else {
        logger = logger.log_to_file(flexi_logger::FileSpec::default())
    }

    logger
        .duplicate_to_stdout(flexi_logger::Duplicate::Error)
        .start()
        .expect("Failed to start logger!")
}
