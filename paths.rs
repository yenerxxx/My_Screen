use chrono::Local;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn default_save_folder() -> PathBuf {
    let home = env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    let p = PathBuf::from(home).join("Desktop").join("MY Screen");
    let _ = fs::create_dir_all(&p);
    p
}

pub fn settings_path() -> PathBuf {
    let appdata = env::var("APPDATA").unwrap_or_else(|_| ".".into());
    let folder = PathBuf::from(appdata).join("MY Screen");
    let _ = fs::create_dir_all(&folder);
    folder.join("settings.json")
}

pub fn open_folder(path: &Path) {
    let _ = fs::create_dir_all(path);
    let _ = Command::new("explorer").arg(path).spawn();
}

pub fn log_error(context: &str, detail: &str) {
    let folder = settings_path()
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let log_file = folder.join("error.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "[{now}] {context}\n{detail}");
    }
}
