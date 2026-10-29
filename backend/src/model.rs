use serde::{Deserialize, Serialize};

pub const MAX_IMAGE_EDGE: u32 = 2560;
pub const WINDOW_PREVIEW_EDGE: u32 = 340;
pub const MAX_WINDOW_PREVIEWS: usize = 24;
pub const LIGHTBOX_EDGE: u32 = 1600;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotItem {
    pub id: String,
    pub created_at: u64,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Capture kind: `display` | `window` | `region` | `delay`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_mode: Option<String>,
    /// Relative path under data dir, e.g. `blobs/<id>.png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_data_url: Option<String>,
    #[serde(default)]
    pub favorite: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app_name: String,
    pub width: u32,
    pub height: u32,
    pub is_focused: bool,
    pub is_minimized: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_data_url: Option<String>,
}

pub struct CaptureImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub display_index: u32,
    pub display_name: String,
    pub capture_mode: String,
    /// When set, `media` stores/moves this file instead of re-encoding raw RGBA.
    pub file_path: Option<std::path::PathBuf>,
}
