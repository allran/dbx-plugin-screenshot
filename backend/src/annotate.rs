use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use dbx_plugin_sdk::PluginError;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::media::file_thumbnail_data_url;
use crate::model::ScreenshotItem;
use crate::prefs;
use crate::store::HistoryStore;
use crate::util::data_dir;

struct AnnotateSession {
    item_id: String,
    path: PathBuf,
    next_index: u32,
}

fn sessions() -> &'static Mutex<HashMap<String, AnnotateSession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, AnnotateSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_sessions()
-> Result<std::sync::MutexGuard<'static, HashMap<String, AnnotateSession>>, PluginError> {
    sessions()
        .lock()
        .map_err(|_| PluginError::new(-32000, "Annotate session lock poisoned"))
}

fn session_path(session_id: &str) -> PathBuf {
    data_dir().join("tmp").join(format!("annotate-{session_id}.part"))
}

pub fn begin(store: &HistoryStore, item_id: &str) -> Result<Value, PluginError> {
    let item = store
        .get(item_id)
        .ok_or_else(|| PluginError::new(-32602, "Screenshot not found"))?;
    if item.media_file.is_none() {
        return Err(PluginError::new(-32000, "Image file missing"));
    }

    let tmp = data_dir().join("tmp");
    fs::create_dir_all(&tmp)
        .map_err(|err| PluginError::new(-32000, format!("Failed to create temp dir: {err}")))?;

    let session_id = Uuid::new_v4().to_string();
    let path = session_path(&session_id);
    File::create(&path)
        .map_err(|err| PluginError::new(-32000, format!("Failed to create annotate session: {err}")))?;

    let mut sessions = lock_sessions()?;
    sessions.insert(
        session_id.clone(),
        AnnotateSession {
            item_id: item_id.to_string(),
            path,
            next_index: 0,
        },
    );

    Ok(json!({ "sessionId": session_id }))
}

pub fn chunk(session_id: &str, index: u32, data_base64: &str) -> Result<Value, PluginError> {
    let mut sessions = lock_sessions()?;
    let session = sessions
        .get_mut(session_id)
        .ok_or_else(|| PluginError::new(-32602, "Annotate session not found"))?;

    if index != session.next_index {
        return Err(PluginError::new(
            -32602,
            format!(
                "Unexpected chunk index {index}, expected {}",
                session.next_index
            ),
        ));
    }

    let bytes = BASE64.decode(data_base64.trim()).map_err(|err| {
        PluginError::new(-32602, format!("Invalid annotate chunk base64: {err}"))
    })?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&session.path)
        .map_err(|err| PluginError::new(-32000, format!("Failed to write annotate chunk: {err}")))?;
    file.write_all(&bytes)
        .map_err(|err| PluginError::new(-32000, format!("Failed to write annotate chunk: {err}")))?;

    session.next_index = session.next_index.saturating_add(1);
    Ok(json!({ "success": true, "nextIndex": session.next_index }))
}

pub fn abort(session_id: &str) -> Result<Value, PluginError> {
    let mut sessions = lock_sessions()?;
    if let Some(session) = sessions.remove(session_id) {
        let _ = fs::remove_file(&session.path);
    }
    Ok(json!({ "success": true }))
}

pub fn commit(store: &mut HistoryStore, session_id: &str) -> Result<Value, PluginError> {
    let mut sessions = lock_sessions()?;
    let session = sessions
        .remove(session_id)
        .ok_or_else(|| PluginError::new(-32602, "Annotate session not found"))?;
    drop(sessions);

    let part_path = session.path;
    if !part_path.exists() {
        return Err(PluginError::new(-32000, "Annotate upload missing"));
    }

    // Detect format by magic bytes — temp path ends in `.part`.
    let bytes = fs::read(&part_path).map_err(|err| {
        PluginError::new(-32000, format!("Failed to read annotate upload: {err}"))
    })?;
    let ext = if bytes.len() >= 8
        && bytes[0] == 0x89
        && bytes[1] == b'P'
        && bytes[2] == b'N'
        && bytes[3] == b'G'
    {
        "png"
    } else if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 {
        "jpg"
    } else {
        let _ = fs::remove_file(&part_path);
        return Err(PluginError::new(
            -32602,
            "Annotate upload must be JPEG or PNG",
        ));
    };

    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|err| {
            let _ = fs::remove_file(&part_path);
            PluginError::new(-32000, format!("Invalid annotate image: {err}"))
        })?
        .into_dimensions()
        .map_err(|err| {
            let _ = fs::remove_file(&part_path);
            PluginError::new(-32000, format!("Invalid annotate image: {err}"))
        })?;

    let final_path = part_path.with_extension(ext);
    fs::rename(&part_path, &final_path).map_err(|err| {
        let _ = fs::remove_file(&part_path);
        PluginError::new(-32000, format!("Failed to finalize annotate upload: {err}"))
    })?;

    let item = store
        .replace_media_from_file(&session.item_id, &final_path, width, height)
        .map_err(|err| {
            let _ = fs::remove_file(&final_path);
            err
        })?;
    let _ = fs::remove_file(&final_path);

    Ok(json!({ "success": true, "item": item }))
}

pub fn replace_item_media(
    item: &mut ScreenshotItem,
    source: &std::path::Path,
    width: u32,
    height: u32,
) -> Result<(), PluginError> {
    // Keep blob extension aligned with source bytes (jpg/png).
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| e == "png" || e == "jpg" || e == "jpeg")
        .unwrap_or_else(|| "jpg".into());
    let ext = if ext == "jpeg" { "jpg".into() } else { ext };
    let rel = format!("blobs/{}.{}", item.id, ext);
    let dest = data_dir().join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            PluginError::new(-32000, format!("Failed to create blobs dir: {err}"))
        })?;
    }
    if let Some(old_rel) = item.media_file.as_ref() {
        if old_rel != &rel {
            let old_path = data_dir().join(old_rel);
            let _ = fs::remove_file(&old_path);
        }
    }
    fs::copy(source, &dest)
        .map_err(|err| PluginError::new(-32000, format!("Failed to replace image blob: {err}")))?;

    let prefs = prefs::get();
    let preview = file_thumbnail_data_url(&dest, prefs.thumb_edge(), prefs.thumb_jpeg_quality())?;
    let name = item
        .display_name
        .clone()
        .unwrap_or_else(|| "Screenshot".into());
    item.media_file = Some(rel);
    item.width = Some(width);
    item.height = Some(height);
    item.label = format!("{name} · {width}×{height}");
    item.preview_data_url = Some(preview);
    Ok(())
}
