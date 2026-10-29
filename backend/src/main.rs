mod annotate;
mod capture;
mod clipboard;
mod media;
mod model;
mod permission;
mod plugin;
mod prefs;
mod reveal;
mod store;
mod util;
mod window_filter;

use std::sync::{Arc, Mutex};

use dbx_plugin_sdk::{PluginMetadata, PluginServer};

use plugin::Plugin;
use store::HistoryStore;
use util::{history_path, log_stderr};

fn main() -> std::io::Result<()> {
    let store = Arc::new(Mutex::new(HistoryStore::load(history_path())));
    log_stderr(format!("history path: {}", history_path().display()));

    let metadata = PluginMetadata::new("io.dbx.screenshot", env!("CARGO_PKG_VERSION"));
    let plugin = Plugin::new(store);
    PluginServer::new(metadata, plugin).serve()
}
