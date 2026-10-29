use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use dbx_plugin_sdk::PluginError;
use image::RgbaImage;
use xcap::{Monitor, Window};

use crate::media::{preview_jpeg_data_url, temp_capture_path};
use crate::model::{
    CaptureImage, DisplayInfo, WindowInfo, MAX_WINDOW_PREVIEWS, WINDOW_PREVIEW_EDGE,
};
use crate::permission;
use crate::util::log_stderr;
use crate::window_filter::should_skip_window;

pub const ERR_CANCELLED: i32 = -32010;
pub const ERR_PERMISSION: i32 = -32011;

pub fn list_displays() -> Result<Vec<DisplayInfo>, PluginError> {
    ensure_permission()?;
    let monitors = Monitor::all().map_err(|err| {
        PluginError::new(
            ERR_PERMISSION,
            format!("Unable to list displays: {err}"),
        )
    })?;

    let mut displays = Vec::with_capacity(monitors.len());
    for (index, monitor) in monitors.into_iter().enumerate() {
        let name = monitor
            .name()
            .unwrap_or_else(|_| format!("Display {}", index + 1));
        let width = monitor.width().unwrap_or(0);
        let height = monitor.height().unwrap_or(0);
        let is_primary = monitor.is_primary().unwrap_or(index == 0);
        displays.push(DisplayInfo {
            index: index as u32,
            name,
            width,
            height,
            is_primary,
        });
    }
    Ok(displays)
}

pub fn capture_display(display_index: Option<u32>, delay_secs: u32) -> Result<CaptureImage, PluginError> {
    ensure_permission()?;
    apply_delay(delay_secs);

    let monitors = Monitor::all().map_err(|err| {
        PluginError::new(ERR_PERMISSION, format!("Unable to access displays: {err}"))
    })?;

    if monitors.is_empty() {
        return Err(PluginError::new(-32000, "No displays found"));
    }

    let index = match display_index {
        Some(i) => i as usize,
        None => monitors
            .iter()
            .position(|m| m.is_primary().unwrap_or(false))
            .unwrap_or(0),
    };

    let monitor = monitors.get(index).ok_or_else(|| {
        PluginError::new(
            -32602,
            format!("Display index {index} is out of range (0..{})", monitors.len()),
        )
    })?;

    let name = monitor
        .name()
        .unwrap_or_else(|_| format!("Display {}", index + 1));

    #[cfg(target_os = "macos")]
    {
        return capture_display_macos(index, name);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let image = monitor.capture_image().map_err(|err| {
            PluginError::new(ERR_PERMISSION, format!("Screen capture failed: {err}"))
        })?;
        rgba_capture(image, index as u32, name)
    }
}

pub fn list_windows() -> Result<Vec<WindowInfo>, PluginError> {
    ensure_permission()?;
    let mut items = collect_candidate_windows()?;
    items.sort_by(|a, b| {
        b.is_focused
            .cmp(&a.is_focused)
            .then_with(|| a.app_name.to_lowercase().cmp(&b.app_name.to_lowercase()))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    if items.len() > MAX_WINDOW_PREVIEWS {
        items.truncate(MAX_WINDOW_PREVIEWS);
    }
    Ok(items)
}

pub fn preview_window(window_id: u32) -> Result<WindowInfo, PluginError> {
    ensure_permission()?;
    let window = find_window(window_id)?;
    let mut info = window_info_from(&window)?;
    if should_skip_window(&info.app_name, &info.title, info.width, info.height) {
        return Err(PluginError::new(-32602, "Window is filtered out"));
    }
    let image = window.capture_image().map_err(|err| {
        PluginError::new(-32000, format!("Window preview failed: {err}"))
    })?;
    info.preview_data_url = Some(preview_jpeg_data_url(
        image.width(),
        image.height(),
        image.as_raw(),
        WINDOW_PREVIEW_EDGE,
    )?);
    Ok(info)
}

pub fn capture_window(window_id: u32, delay_secs: u32) -> Result<CaptureImage, PluginError> {
    ensure_permission()?;
    apply_delay(delay_secs);

    let window = find_window(window_id)?;
    let info = window_info_from(&window)?;
    if should_skip_window(&info.app_name, &info.title, info.width, info.height) {
        return Err(PluginError::new(-32602, "Window is filtered out"));
    }

    if window.is_minimized().unwrap_or(false) {
        return Err(PluginError::new(
            -32000,
            "Window is minimized; restore it before capturing",
        ));
    }

    let label = if !info.title.trim().is_empty() && !info.app_name.trim().is_empty() {
        if info.title == info.app_name {
            info.app_name
        } else {
            format!("{} — {}", info.app_name, info.title)
        }
    } else if !info.title.trim().is_empty() {
        info.title
    } else if !info.app_name.trim().is_empty() {
        info.app_name
    } else {
        format!("Window {window_id}")
    };

    let image = window.capture_image().map_err(|err| {
        PluginError::new(
            ERR_PERMISSION,
            format!("Window capture failed: {err}"),
        )
    })?;

    rgba_capture(image, 0, label)
}

fn collect_candidate_windows() -> Result<Vec<WindowInfo>, PluginError> {
    let windows = Window::all().map_err(|err| {
        PluginError::new(ERR_PERMISSION, format!("Unable to list windows: {err}"))
    })?;

    let mut items = Vec::new();
    for window in windows {
        let info = match window_info_from(&window) {
            Ok(info) => info,
            Err(_) => continue,
        };
        if should_skip_window(&info.app_name, &info.title, info.width, info.height) {
            continue;
        }
        if info.is_minimized {
            continue;
        }
        items.push(info);
    }
    Ok(items)
}

fn find_window(window_id: u32) -> Result<Window, PluginError> {
    let windows = Window::all().map_err(|err| {
        PluginError::new(ERR_PERMISSION, format!("Unable to list windows: {err}"))
    })?;
    windows
        .into_iter()
        .find(|window| window.id().ok() == Some(window_id))
        .ok_or_else(|| PluginError::new(-32602, format!("Window {window_id} not found")))
}

fn window_info_from(window: &Window) -> Result<WindowInfo, PluginError> {
    let width = window.width().unwrap_or(0);
    let height = window.height().unwrap_or(0);
    let title = window.title().unwrap_or_default();
    let app_name = window.app_name().unwrap_or_default();
    let id = window
        .id()
        .map_err(|err| PluginError::new(-32000, format!("Window id unavailable: {err}")))?;
    Ok(WindowInfo {
        id,
        title: if title.trim().is_empty() {
            app_name.clone()
        } else {
            title
        },
        app_name,
        width,
        height,
        is_focused: window.is_focused().unwrap_or(false),
        is_minimized: window.is_minimized().unwrap_or(false),
        preview_data_url: None,
    })
}

pub fn capture_region(delay_secs: u32) -> Result<CaptureImage, PluginError> {
    ensure_permission()?;
    apply_delay(delay_secs);

    #[cfg(target_os = "macos")]
    {
        return capture_region_macos();
    }

    #[cfg(target_os = "windows")]
    {
        return capture_region_windows();
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return capture_region_linux();
    }

    #[allow(unreachable_code)]
    Err(PluginError::new(
        -32000,
        "Region capture is not supported on this platform",
    ))
}

fn ensure_permission() -> Result<(), PluginError> {
    let status = permission::status();
    if status.granted {
        return Ok(());
    }
    Err(PluginError::new(
        ERR_PERMISSION,
        status.message,
    ))
}

fn apply_delay(delay_secs: u32) {
    if delay_secs == 0 {
        return;
    }
    let wait = delay_secs.min(30);
    log_stderr(format!("delaying capture by {wait}s"));
    thread::sleep(Duration::from_secs(u64::from(wait)));
}

fn rgba_capture(image: RgbaImage, display_index: u32, display_name: String) -> Result<CaptureImage, PluginError> {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Err(PluginError::new(
            ERR_PERMISSION,
            "Captured image is empty — check Screen Recording permission",
        ));
    }
    Ok(CaptureImage {
        width,
        height,
        rgba: image.into_raw(),
        display_index,
        display_name,
        capture_mode: String::new(),
        file_path: None,
    })
}

fn capture_from_file(
    path: std::path::PathBuf,
    display_index: u32,
    display_name: String,
) -> Result<CaptureImage, PluginError> {
    let (width, height) = image::image_dimensions(&path).map_err(|err| {
        PluginError::new(-32000, format!("Failed to read capture dimensions: {err}"))
    })?;
    if width == 0 || height == 0 {
        let _ = fs::remove_file(&path);
        return Err(PluginError::new(
            ERR_PERMISSION,
            "Captured image is empty — check Screen Recording permission",
        ));
    }
    Ok(CaptureImage {
        width,
        height,
        rgba: Vec::new(),
        display_index,
        display_name,
        capture_mode: String::new(),
        file_path: Some(path),
    })
}

#[cfg(target_os = "macos")]
fn capture_display_macos(index: usize, name: String) -> Result<CaptureImage, PluginError> {
    // System screencapture is much faster than pulling a full Retina framebuffer through xcap.
    let path = temp_capture_path("display", "jpg")?;
    let path_str = path.to_string_lossy().to_string();
    let display_number = (index + 1).to_string();
    let status = Command::new("screencapture")
        .args(["-x", "-t", "jpg", "-D", &display_number, &path_str])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|err| PluginError::new(-32000, format!("Failed to start screencapture: {err}")))?;

    if !status.success() || !path.exists() {
        let _ = fs::remove_file(&path);
        return Err(PluginError::new(
            ERR_PERMISSION,
            "Screen capture failed — check Screen Recording permission",
        ));
    }
    capture_from_file(path, index as u32, name)
}

#[cfg(target_os = "macos")]
fn capture_region_macos() -> Result<CaptureImage, PluginError> {
    let path = temp_capture_path("region", "jpg")?;
    let path_str = path.to_string_lossy().to_string();

    // Interactive system UI: drag to select, Esc cancels.
    let status = Command::new("screencapture")
        .args(["-i", "-x", "-t", "jpg", &path_str])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|err| PluginError::new(-32000, format!("Failed to start screencapture: {err}")))?;

    if !status.success() {
        let _ = fs::remove_file(&path);
        return Err(PluginError::new(ERR_CANCELLED, "Region capture cancelled"));
    }
    if !path.exists() {
        return Err(PluginError::new(ERR_CANCELLED, "Region capture cancelled"));
    }

    capture_from_file(path, 0, "Region".into())
}

#[cfg(target_os = "windows")]
fn capture_region_windows() -> Result<CaptureImage, PluginError> {
    // Snipping Tool interactive UI (Win10+). Saves via temp path is unreliable across locales,
    // so currently return a clear unsupported message until a native picker lands.
    Err(PluginError::new(
        -32000,
        "Region capture on Windows is not implemented yet; use full-display capture for now",
    ))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn capture_region_linux() -> Result<CaptureImage, PluginError> {
    let path = temp_capture_path("region", "png")?;
    let path_str = path.to_string_lossy().to_string();

    let attempts: Vec<(&str, Vec<String>)> = vec![
        (
            "gnome-screenshot",
            vec!["-a".into(), "-f".into(), path_str.clone()],
        ),
        (
            "spectacle",
            vec![
                "-b".into(),
                "-r".into(),
                "-o".into(),
                path_str.clone(),
            ],
        ),
        ("scrot", vec!["-s".into(), path_str.clone()]),
    ];

    let mut saw_binary = false;
    for (bin, args) in attempts {
        let status = Command::new(bin)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status {
            Ok(code) if code.success() && path.exists() => {
                return capture_from_file(path, 0, "Region".into());
            }
            Ok(code) => {
                saw_binary = true;
                let _ = fs::remove_file(&path);
                if !code.success() {
                    return Err(PluginError::new(ERR_CANCELLED, "Region capture cancelled"));
                }
            }
            Err(_) => continue,
        }
    }

    if saw_binary {
        return Err(PluginError::new(ERR_CANCELLED, "Region capture cancelled"));
    }
    Err(PluginError::new(
        -32000,
        "Region capture needs gnome-screenshot, spectacle, or scrot on PATH",
    ))
}
