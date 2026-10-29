use std::fs;
use std::path::PathBuf;

use dbx_plugin_sdk::PluginError;
use uuid::Uuid;

use std::path::Path;

use crate::annotate::replace_item_media;
use crate::media::build_screenshot_item;
use crate::model::{CaptureImage, ScreenshotItem};
use crate::prefs;
use crate::util::{data_dir, item_matches, log_stderr, remove_media_file};

pub struct HistoryStore {
    pub items: Vec<ScreenshotItem>,
    path: PathBuf,
}

impl HistoryStore {
    pub fn load(path: PathBuf) -> Self {
        let items = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<ScreenshotItem>>(&raw).ok())
            .unwrap_or_default();
        Self { items, path }
    }

    pub fn persist(&self) -> Result<(), PluginError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                PluginError::new(-32000, format!("Failed to create data dir: {err}"))
            })?;
        }
        let raw = serde_json::to_string_pretty(&self.items).map_err(|err| {
            PluginError::new(-32000, format!("Failed to serialize history: {err}"))
        })?;
        fs::write(&self.path, raw)
            .map_err(|err| PluginError::new(-32000, format!("Failed to write history: {err}")))?;
        Ok(())
    }

    pub fn list(&self, query: Option<&str>) -> Vec<ScreenshotItem> {
        let Some(query) = query.map(str::trim).filter(|q| !q.is_empty()) else {
            return self.items.clone();
        };
        let needle = query.to_lowercase();
        self.items
            .iter()
            .filter(|item| item_matches(item, &needle))
            .cloned()
            .collect()
    }

    pub fn push_capture(&mut self, capture: CaptureImage) -> Result<ScreenshotItem, PluginError> {
        let id = Uuid::new_v4().to_string();
        let item = build_screenshot_item(&id, &capture).map_err(|err| {
            log_stderr(format!("build screenshot failed: {err:?}"));
            err
        })?;
        self.items.insert(0, item.clone());
        self.trim_to_limit(prefs::get().history_limit())?;
        self.persist()?;
        Ok(item)
    }

    pub fn trim_to_limit(&mut self, limit: usize) -> Result<(), PluginError> {
        let limit = limit.max(1);
        while self.items.len() > limit {
            if let Some(pos) = self.items.iter().rposition(|existing| !existing.favorite) {
                let removed = self.items.remove(pos);
                remove_media_file(&removed);
            } else if let Some(removed) = self.items.pop() {
                remove_media_file(&removed);
            } else {
                break;
            }
        }
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<bool, PluginError> {
        let Some(pos) = self.items.iter().position(|item| item.id == id) else {
            return Ok(false);
        };
        let removed = self.items.remove(pos);
        remove_media_file(&removed);
        self.persist()?;
        Ok(true)
    }

    pub fn clear(&mut self) -> Result<(), PluginError> {
        for item in &self.items {
            remove_media_file(item);
        }
        self.items.clear();
        let _ = fs::remove_dir_all(data_dir().join("blobs"));
        let _ = fs::remove_dir_all(data_dir().join("tmp"));
        self.persist()
    }

    pub fn get(&self, id: &str) -> Option<&ScreenshotItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn toggle_favorite(&mut self, id: &str) -> Result<Option<bool>, PluginError> {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return Ok(None);
        };
        item.favorite = !item.favorite;
        let favorite = item.favorite;
        self.persist()?;
        Ok(Some(favorite))
    }

    pub fn replace_media_from_file(
        &mut self,
        id: &str,
        source: &Path,
        width: u32,
        height: u32,
    ) -> Result<ScreenshotItem, PluginError> {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return Err(PluginError::new(-32602, "Screenshot not found"));
        };
        replace_item_media(item, source, width, height)?;
        let updated = item.clone();
        self.persist()?;
        Ok(updated)
    }
}
