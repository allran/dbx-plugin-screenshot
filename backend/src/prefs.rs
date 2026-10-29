use std::fs;
use std::sync::{Mutex, OnceLock};

use dbx_plugin_sdk::PluginError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::util::{data_dir, log_stderr};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    /// Delayed-capture countdown: 3 | 5 | 10.
    #[serde(default = "default_delay_secs")]
    pub delay_secs: u32,
    /// Open annotator after a successful capture.
    #[serde(default = "default_true")]
    pub open_annotator_after_capture: bool,
    /// Copy image to clipboard after save / capture (when annotator is skipped).
    #[serde(default = "default_true")]
    pub copy_after_save: bool,
    /// Remembered display index when multiple monitors exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_display_index: Option<u32>,
    /// Stored screenshot format: `jpeg` | `png`.
    #[serde(default = "default_file_format")]
    pub file_format: String,
    /// Encode quality: `high` | `medium` | `low`.
    #[serde(default = "default_quality")]
    pub quality: String,
    /// List thumbnail edge: `small` | `medium` | `large`.
    #[serde(default = "default_thumb_size")]
    pub thumb_size: String,
    /// Max screenshots kept in history.
    #[serde(default = "default_history_max")]
    pub history_max: u32,
}

fn default_delay_secs() -> u32 {
    5
}
fn default_true() -> bool {
    true
}
fn default_file_format() -> String {
    "jpeg".into()
}
fn default_quality() -> String {
    "high".into()
}
fn default_thumb_size() -> String {
    "medium".into()
}
fn default_history_max() -> u32 {
    200
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            delay_secs: default_delay_secs(),
            open_annotator_after_capture: true,
            copy_after_save: true,
            default_display_index: None,
            file_format: default_file_format(),
            quality: default_quality(),
            thumb_size: default_thumb_size(),
            history_max: default_history_max(),
        }
    }
}

impl Preferences {
    pub fn normalize(mut self) -> Self {
        self.delay_secs = match self.delay_secs {
            3 | 10 => self.delay_secs,
            _ => 5,
        };
        self.file_format = match self.file_format.trim().to_ascii_lowercase().as_str() {
            "png" => "png".into(),
            _ => "jpeg".into(),
        };
        self.quality = match self.quality.trim().to_ascii_lowercase().as_str() {
            "low" => "low".into(),
            "medium" => "medium".into(),
            _ => "high".into(),
        };
        self.thumb_size = match self.thumb_size.trim().to_ascii_lowercase().as_str() {
            "small" | "large" => self.thumb_size.trim().to_ascii_lowercase(),
            _ => "medium".into(),
        };
        self.history_max = match self.history_max {
            50 | 100 | 200 | 500 => self.history_max,
            _ => 200,
        };
        self
    }

    pub fn history_limit(&self) -> usize {
        self.history_max as usize
    }

    pub fn is_png(&self) -> bool {
        self.file_format == "png"
    }

    pub fn jpeg_quality(&self) -> u8 {
        match self.quality.as_str() {
            "low" => 68,
            "medium" => 82,
            _ => 92,
        }
    }

    pub fn thumb_jpeg_quality(&self) -> u8 {
        match self.quality.as_str() {
            "low" => 48,
            "medium" => 55,
            _ => 58,
        }
    }

    pub fn max_image_edge(&self) -> u32 {
        match self.quality.as_str() {
            "low" => 1600,
            "medium" => 2048,
            _ => 2560,
        }
    }

    pub fn thumb_edge(&self) -> u32 {
        match self.thumb_size.as_str() {
            "small" => 120,
            "large" => 320,
            _ => 200,
        }
    }

    pub fn ext(&self) -> &'static str {
        if self.is_png() {
            "png"
        } else {
            "jpg"
        }
    }
}

fn prefs_path() -> std::path::PathBuf {
    data_dir().join("preferences.json")
}

fn cache() -> &'static Mutex<Preferences> {
    static CACHE: OnceLock<Mutex<Preferences>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(load_from_disk()))
}

fn load_from_disk() -> Preferences {
    match fs::read_to_string(prefs_path()) {
        Ok(raw) => serde_json::from_str::<Preferences>(&raw)
            .unwrap_or_default()
            .normalize(),
        Err(_) => Preferences::default(),
    }
}

pub fn get() -> Preferences {
    cache()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| Preferences::default())
}

pub fn save(prefs: Preferences) -> Result<Preferences, PluginError> {
    let prefs = prefs.normalize();
    if let Some(parent) = prefs_path().parent() {
        fs::create_dir_all(parent).map_err(|err| {
            PluginError::new(-32000, format!("Failed to create prefs dir: {err}"))
        })?;
    }
    let raw = serde_json::to_string_pretty(&prefs)
        .map_err(|err| PluginError::new(-32000, format!("Failed to serialize prefs: {err}")))?;
    fs::write(prefs_path(), raw)
        .map_err(|err| PluginError::new(-32000, format!("Failed to write prefs: {err}")))?;
    if let Ok(mut guard) = cache().lock() {
        *guard = prefs.clone();
    }
    log_stderr(format!(
        "prefs saved: format={} quality={} thumb={} delay={}s annotator={} copy={} display={:?}",
        prefs.file_format,
        prefs.quality,
        prefs.thumb_size,
        prefs.delay_secs,
        prefs.open_annotator_after_capture,
        prefs.copy_after_save,
        prefs.default_display_index
    ));
    Ok(prefs)
}

pub fn patch(params: &Value) -> Result<Preferences, PluginError> {
    let mut prefs = get();
    if let Some(v) = params.get("delaySecs").and_then(Value::as_u64) {
        prefs.delay_secs = v as u32;
    }
    if let Some(v) = params
        .get("openAnnotatorAfterCapture")
        .and_then(Value::as_bool)
    {
        prefs.open_annotator_after_capture = v;
    }
    if let Some(v) = params.get("copyAfterSave").and_then(Value::as_bool) {
        prefs.copy_after_save = v;
    }
    if params.get("defaultDisplayIndex").is_some() {
        prefs.default_display_index = params
            .get("defaultDisplayIndex")
            .and_then(Value::as_u64)
            .map(|v| v as u32);
    }
    if let Some(v) = params.get("fileFormat").and_then(Value::as_str) {
        prefs.file_format = v.into();
    }
    if let Some(v) = params.get("quality").and_then(Value::as_str) {
        prefs.quality = v.into();
    }
    if let Some(v) = params.get("thumbSize").and_then(Value::as_str) {
        prefs.thumb_size = v.into();
    }
    if let Some(v) = params.get("historyMax").and_then(Value::as_u64) {
        prefs.history_max = v as u32;
    }
    save(prefs)
}

/// JSON payload for preferences/get including storage meta.
pub fn get_with_meta(item_count: usize) -> Value {
    let prefs = get();
    json!({
        "delaySecs": prefs.delay_secs,
        "openAnnotatorAfterCapture": prefs.open_annotator_after_capture,
        "copyAfterSave": prefs.copy_after_save,
        "defaultDisplayIndex": prefs.default_display_index,
        "fileFormat": prefs.file_format,
        "quality": prefs.quality,
        "thumbSize": prefs.thumb_size,
        "historyMax": prefs.history_max,
        "historyCount": item_count,
    })
}
