#![windows_subsystem = "windows"]

mod annotator;
mod caps_lock;
mod constants;
mod hotkeys;
mod icon_helper;
mod image_ops;
mod liquid_glass;
mod overlay;
mod paths;
mod settings_store;
mod single_instance;
mod ui;

use single_instance::{notify_existing_instance, start_instance_server};
use std::env;
use std::sync::mpsc;

fn command_from_args() -> &'static str {
    for arg in env::args().skip(1) {
        match arg.to_lowercase().as_str() {
            "--capture-full" => return "CAPTURE_FULL",
            "--capture" => return "CAPTURE",
            _ => {}
        }
    }
    "SHOW"
}

fn main() -> eframe::Result<()> {
    let cmd = command_from_args();
    if notify_existing_instance(cmd) {
        return Ok(());
    }

    let (tx, rx) = mpsc::channel::<String>();
    if cmd != "SHOW" {
        let _ = tx.send(cmd.to_string());
    }
    start_instance_server(tx.clone());

    ui::run_app(tx, rx)
}
