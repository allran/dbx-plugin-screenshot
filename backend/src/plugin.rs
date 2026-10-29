use std::fs;
use std::sync::{Arc, Mutex};

use dbx_plugin_sdk::{PluginEmitter, PluginError, PluginHandler, RequestContext};
use serde_json::{json, Value};

use crate::annotate;
use crate::capture::{
    self, capture_display, capture_region, capture_window, list_displays, list_windows,
    preview_window,
};
use crate::clipboard::write_item_to_clipboard;
use crate::media::{build_lightbox_data_url, build_media_data_url, read_media_blob};
use crate::model::ScreenshotItem;
use crate::permission;
use crate::prefs;
use crate::reveal::open_local_path;
use crate::store::HistoryStore;
use crate::util::data_dir;

pub struct Plugin {
    store: Arc<Mutex<HistoryStore>>,
}

impl Plugin {
    pub fn new(store: Arc<Mutex<HistoryStore>>) -> Self {
        Self { store }
    }

    fn with_store<R>(
        &self,
        f: impl FnOnce(&mut HistoryStore) -> Result<R, PluginError>,
    ) -> Result<R, PluginError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| PluginError::new(-32000, "History store is poisoned"))?;
        f(&mut store)
    }

    fn require_item(&self, id: &str) -> Result<ScreenshotItem, PluginError> {
        self.with_store(|store| {
            store
                .get(id)
                .cloned()
                .ok_or_else(|| PluginError::new(-32602, "Screenshot not found"))
        })
    }
}

fn require_id(params: &Value) -> Result<&str, PluginError> {
    params
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::new(-32602, "Missing id"))
}

impl PluginHandler for Plugin {
    fn handle(
        &self,
        _context: RequestContext,
        method: &str,
        params: Value,
        emitter: &PluginEmitter,
    ) -> Result<Value, PluginError> {
        match method {
            "screenshot/permission" => Ok(json!(permission::status())),
            "screenshot/permission/request" => {
                // Require an explicit UI confirmation flag so hosts never trigger OS prompts silently.
                let confirmed = params
                    .get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !confirmed {
                    return Err(PluginError::new(
                        -32602,
                        "Permission request requires confirmed:true after showing an in-plugin prompt",
                    ));
                }
                Ok(json!(permission::request()?))
            }
            "screenshot/permission/open-settings" => {
                let confirmed = params
                    .get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !confirmed {
                    return Err(PluginError::new(
                        -32602,
                        "Opening settings requires confirmed:true after showing an in-plugin prompt",
                    ));
                }
                permission::open_settings()?;
                Ok(json!({ "success": true }))
            }
            "screenshot/displays" => {
                let displays = list_displays()?;
                Ok(json!({ "displays": displays }))
            }
            "screenshot/windows" => {
                // Metadata only — previews load progressively via screenshot/window/preview.
                let windows = list_windows()?;
                Ok(json!({ "windows": windows }))
            }
            "screenshot/window/preview" => {
                let window_id = params
                    .get("id")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
                    .ok_or_else(|| PluginError::new(-32602, "Missing id"))?;
                let window = preview_window(window_id)?;
                Ok(json!({ "window": window }))
            }
            "screenshot/list" => {
                let query = params.get("query").and_then(Value::as_str);
                self.with_store(|store| {
                    let items = store.list(query);
                    Ok(json!({ "items": items, "total": items.len() }))
                })
            }
            "screenshot/capture" => {
                let mode = params
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("display");
                let delay_secs = params
                    .get("delaySecs")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32)
                    .unwrap_or(0);

                let capture = match mode {
                    // FE countdown then interactive region — mode "delay" skips backend sleep.
                    "region" | "delay" => capture_region(delay_secs),
                    "display" => {
                        let display_index = params
                            .get("displayIndex")
                            .and_then(Value::as_u64)
                            .map(|v| v as u32);
                        capture_display(display_index, delay_secs)
                    }
                    "window" => {
                        let window_id = params
                            .get("windowId")
                            .and_then(Value::as_u64)
                            .map(|v| v as u32)
                            .ok_or_else(|| PluginError::new(-32602, "Missing windowId"))?;
                        capture_window(window_id, delay_secs)
                    }
                    other => Err(PluginError::new(
                        -32602,
                        format!("Unknown capture mode: {other}"),
                    )),
                };

                let mut capture = match capture {
                    Ok(value) => value,
                    Err(err) if err.code == capture::ERR_CANCELLED => {
                        return Ok(json!({
                            "success": false,
                            "cancelled": true,
                            "message": err.message
                        }));
                    }
                    Err(err) => return Err(err),
                };

                capture.capture_mode = if delay_secs > 0 || mode == "delay" {
                    "delay".into()
                } else {
                    mode.into()
                };

                // Interactive selection / system capture finished — UI shows "capturing…".
                let _ = emitter.event(
                    "screenshot/phase",
                    json!({ "phase": "saving", "mode": mode }),
                );

                self.with_store(|store| {
                    let item = store.push_capture(capture)?;
                    Ok(json!({
                        "success": true,
                        "cancelled": false,
                        "item": item,
                        "total": store.items.len()
                    }))
                })
            }
            "screenshot/copy" => {
                let id = require_id(&params)?;
                let item = self.require_item(id)?;
                write_item_to_clipboard(&item)?;
                Ok(json!({ "success": true }))
            }
            "screenshot/delete" => {
                let id = require_id(&params)?;
                let deleted = self.with_store(|store| store.delete(id))?;
                Ok(json!({ "success": deleted }))
            }
            "screenshot/favorite" => {
                let id = require_id(&params)?;
                let favorite = self.with_store(|store| store.toggle_favorite(id))?;
                let Some(favorite) = favorite else {
                    return Err(PluginError::new(-32602, "Screenshot not found"));
                };
                Ok(json!({ "success": true, "favorite": favorite }))
            }
            "screenshot/clear" => {
                self.with_store(|store| store.clear())?;
                Ok(json!({ "success": true }))
            }
            "screenshot/preferences/get" => {
                let count = self.with_store(|store| Ok(store.items.len()))?;
                Ok(prefs::get_with_meta(count))
            }
            "screenshot/preferences/set" => {
                let updated = prefs::patch(&params)?;
                if params.get("historyMax").is_some() {
                    self.with_store(|store| {
                        store.trim_to_limit(updated.history_limit())?;
                        store.persist()
                    })?;
                }
                Ok(json!(updated))
            }
            "screenshot/open-data-folder" => {
                let blobs = data_dir().join("blobs");
                fs::create_dir_all(&blobs).map_err(|err| {
                    PluginError::new(-32000, format!("Failed to create blobs dir: {err}"))
                })?;
                open_local_path(&blobs.to_string_lossy())?;
                Ok(json!({ "success": true, "path": blobs }))
            }
            // Host media URL bridge — browser GETs this; we stream the stored JPEG as-is.
            "filesystem/media/read" => {
                let id = require_id(&params)?;
                let item = self.require_item(id)?;
                let rel = item
                    .media_file
                    .as_ref()
                    .ok_or_else(|| PluginError::new(-32000, "Image file missing"))?;
                read_media_blob(&data_dir().join(rel), &params)
            }
            "screenshot/media" => {
                let id = require_id(&params)?;
                let item = self.require_item(id)?;
                let rel = item
                    .media_file
                    .as_ref()
                    .ok_or_else(|| PluginError::new(-32000, "Image file missing"))?;
                let full = params
                    .get("full")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let (width, height, data_url) = if full {
                    build_media_data_url(&data_dir().join(rel), true)?
                } else {
                    build_lightbox_data_url(&data_dir().join(rel))?
                };
                Ok(json!({
                    "id": item.id,
                    "width": width,
                    "height": height,
                    "dataUrl": data_url
                }))
            }
            "screenshot/reveal" => {
                let id = require_id(&params)?;
                let item = self.require_item(id)?;
                let rel = item
                    .media_file
                    .as_ref()
                    .ok_or_else(|| PluginError::new(-32000, "Image file missing"))?;
                let path = data_dir().join(rel);
                open_local_path(&path.to_string_lossy())?;
                Ok(json!({ "success": true, "path": path }))
            }
            "screenshot/annotate/begin" => {
                let id = require_id(&params)?;
                self.with_store(|store| annotate::begin(store, id))
            }
            "screenshot/annotate/chunk" => {
                let session_id = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::new(-32602, "Missing sessionId"))?;
                let index = params
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| PluginError::new(-32602, "Missing index"))?
                    as u32;
                let data_base64 = params
                    .get("dataBase64")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::new(-32602, "Missing dataBase64"))?;
                annotate::chunk(session_id, index, data_base64)
            }
            "screenshot/annotate/commit" => {
                let session_id = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::new(-32602, "Missing sessionId"))?;
                self.with_store(|store| annotate::commit(store, session_id))
            }
            "screenshot/annotate/abort" => {
                let session_id = params
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::new(-32602, "Missing sessionId"))?;
                annotate::abort(session_id)
            }
            _ => Err(PluginError::method_not_found(method)),
        }
    }
}
