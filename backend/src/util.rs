use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::ScreenshotItem;

pub fn data_dir() -> PathBuf {
    env::var_os("DBX_PLUGIN_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::temp_dir()
                .join("dbx-plugin-data")
                .join("io.dbx.screenshot")
        })
}

pub fn history_path() -> PathBuf {
    data_dir().join("history.json")
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn item_matches(item: &ScreenshotItem, needle: &str) -> bool {
    item.label.to_lowercase().contains(needle)
        || item
            .display_name
            .as_ref()
            .map(|name| name.to_lowercase().contains(needle))
            .unwrap_or(false)
}

pub fn remove_media_file(item: &ScreenshotItem) {
    if let Some(rel) = &item.media_file {
        let _ = std::fs::remove_file(data_dir().join(rel));
    }
}

pub fn log_stderr(message: impl AsRef<str>) {
    let _ = writeln!(io::stderr(), "[io.dbx.screenshot] {}", message.as_ref());
}
