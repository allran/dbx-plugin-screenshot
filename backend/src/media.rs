use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use dbx_plugin_sdk::PluginError;
use image::imageops::FilterType;
use image::{ImageBuffer, RgbaImage};

use crate::model::{CaptureImage, ScreenshotItem, LIGHTBOX_EDGE, MAX_IMAGE_EDGE};
use crate::prefs;
use crate::util::{data_dir, now_ms};

pub fn build_screenshot_item(id: &str, capture: &CaptureImage) -> Result<ScreenshotItem, PluginError> {
    if let Some(path) = &capture.file_path {
        return build_screenshot_item_from_file(id, path, capture);
    }
    build_screenshot_item_from_rgba(id, capture)
}

fn build_screenshot_item_from_file(
    id: &str,
    source: &Path,
    capture: &CaptureImage,
) -> Result<ScreenshotItem, PluginError> {
    let prefs = prefs::get();
    let blobs = data_dir().join("blobs");
    fs::create_dir_all(&blobs)
        .map_err(|err| PluginError::new(-32000, format!("Failed to create blobs dir: {err}")))?;

    // Dimensions only — do not decode the full frame until we need a tiny thumb.
    let (src_w, src_h) = image::image_dimensions(source).map_err(|err| {
        PluginError::new(-32000, format!("Failed to read capture dimensions: {err}"))
    })?;

    let source_is_jpeg = source
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"));

    let max_edge = prefs.max_image_edge();
    let thumb_edge = prefs.thumb_edge();
    let rel = format!("blobs/{id}.{}", prefs.ext());
    let dest = data_dir().join(&rel);

    // Fast path: keep system JPEG when user wants high-quality JPEG and no downscale.
    let keep_source_jpeg = source_is_jpeg
        && !prefs.is_png()
        && prefs.quality == "high"
        && src_w <= max_edge
        && src_h <= max_edge;

    let (store_w, store_h, preview_data_url) = if keep_source_jpeg {
        fs::copy(source, &dest)
            .map_err(|err| PluginError::new(-32000, format!("Failed to store capture: {err}")))?;
        let preview_data_url =
            file_thumbnail_data_url(&dest, thumb_edge, prefs.thumb_jpeg_quality())?;
        (src_w, src_h, preview_data_url)
    } else {
        let img = image::open(source).map_err(|err| {
            PluginError::new(-32000, format!("Failed to open capture file: {err}"))
        })?;
        let needs_downscale = src_w > max_edge || src_h > max_edge;
        let resized = if needs_downscale {
            img.resize(max_edge, max_edge, FilterType::Nearest)
        } else {
            img
        };
        let store_w = resized.width();
        let store_h = resized.height();
        encode_image_file(&resized.to_rgb8(), &dest, &prefs)?;
        let thumb = resized.resize(thumb_edge, thumb_edge, FilterType::Nearest);
        let preview_data_url = encode_jpeg_data_url(&thumb.to_rgb8(), prefs.thumb_jpeg_quality())?;
        (store_w, store_h, preview_data_url)
    };

    let _ = fs::remove_file(source);

    Ok(ScreenshotItem {
        id: id.to_string(),
        created_at: now_ms(),
        label: format!("{} · {}×{}", capture.display_name, store_w, store_h),
        width: Some(store_w),
        height: Some(store_h),
        display_index: Some(capture.display_index),
        display_name: Some(capture.display_name.clone()),
        capture_mode: non_empty_mode(&capture.capture_mode),
        media_file: Some(rel),
        preview_data_url: Some(preview_data_url),
        favorite: false,
    })
}

fn non_empty_mode(mode: &str) -> Option<String> {
    let trimmed = mode.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn file_thumbnail_data_url(
    path: &Path,
    max_edge: u32,
    quality: u8,
) -> Result<String, PluginError> {
    let img = image::open(path)
        .map_err(|err| PluginError::new(-32000, format!("Failed to open image for thumb: {err}")))?;
    let thumb = img.resize(max_edge, max_edge, FilterType::Nearest);
    encode_jpeg_data_url(&thumb.to_rgb8(), quality)
}

fn build_screenshot_item_from_rgba(
    id: &str,
    capture: &CaptureImage,
) -> Result<ScreenshotItem, PluginError> {
    let prefs = prefs::get();
    let max_edge = prefs.max_image_edge();
    let thumb_edge = prefs.thumb_edge();
    let (store_w, store_h, store_rgba) =
        maybe_downscale(capture.width, capture.height, &capture.rgba, max_edge)?;
    let rgb = rgba_to_rgb(store_w, store_h, &store_rgba)?;

    let blobs = data_dir().join("blobs");
    fs::create_dir_all(&blobs)
        .map_err(|err| PluginError::new(-32000, format!("Failed to create blobs dir: {err}")))?;
    let rel = format!("blobs/{id}.{}", prefs.ext());
    encode_image_file(&rgb, &data_dir().join(&rel), &prefs)?;

    let (thumb_w, thumb_h, thumb_rgba) =
        maybe_downscale(store_w, store_h, &store_rgba, thumb_edge)?;
    let thumb_rgb = rgba_to_rgb(thumb_w, thumb_h, &thumb_rgba)?;
    let preview_data_url = encode_jpeg_data_url(&thumb_rgb, prefs.thumb_jpeg_quality())?;

    Ok(ScreenshotItem {
        id: id.to_string(),
        created_at: now_ms(),
        label: format!("{} · {}×{}", capture.display_name, store_w, store_h),
        width: Some(store_w),
        height: Some(store_h),
        display_index: Some(capture.display_index),
        display_name: Some(capture.display_name.clone()),
        capture_mode: non_empty_mode(&capture.capture_mode),
        media_file: Some(rel),
        preview_data_url: Some(preview_data_url),
        favorite: false,
    })
}

pub(crate) fn maybe_downscale(
    width: u32,
    height: u32,
    rgba: &[u8],
    max_edge: u32,
) -> Result<(u32, u32, Vec<u8>), PluginError> {
    maybe_downscale_with_filter(width, height, rgba, max_edge, FilterType::Nearest)
}

fn maybe_downscale_with_filter(
    width: u32,
    height: u32,
    rgba: &[u8],
    max_edge: u32,
    filter: FilterType,
) -> Result<(u32, u32, Vec<u8>), PluginError> {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.len() < expected {
        return Err(PluginError::new(-32000, "Image buffer is truncated"));
    }
    let img: RgbaImage = ImageBuffer::from_raw(width, height, rgba[..expected].to_vec())
        .ok_or_else(|| PluginError::new(-32000, "Invalid image buffer"))?;
    if width <= max_edge && height <= max_edge {
        return Ok((width, height, img.into_raw()));
    }
    let scale = (max_edge as f32 / width.max(height) as f32).min(1.0);
    let new_w = ((width as f32) * scale).round().max(1.0) as u32;
    let new_h = ((height as f32) * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(&img, new_w, new_h, filter);
    Ok((new_w, new_h, resized.into_raw()))
}

/// JPEG data URL for window picker cards — clear enough on ~260px tiles, keep payload small.
pub(crate) fn preview_jpeg_data_url(
    width: u32,
    height: u32,
    rgba: &[u8],
    max_edge: u32,
) -> Result<String, PluginError> {
    let (view_w, view_h, view_rgba) =
        maybe_downscale_with_filter(width, height, rgba, max_edge, FilterType::Triangle)?;
    let rgb = rgba_to_rgb(view_w, view_h, &view_rgba)?;
    encode_jpeg_data_url(&rgb, 68)
}

fn rgba_to_rgb(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<image::RgbImage, PluginError> {
    let img: RgbaImage = ImageBuffer::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| PluginError::new(-32000, "Invalid image buffer"))?;
    Ok(image::DynamicImage::ImageRgba8(img).to_rgb8())
}

fn encode_image_file(
    rgb: &image::RgbImage,
    path: &Path,
    prefs: &prefs::Preferences,
) -> Result<(), PluginError> {
    if prefs.is_png() {
        encode_png_file(rgb, path)
    } else {
        encode_jpeg_file(rgb, path, prefs.jpeg_quality())
    }
}

fn encode_png_file(rgb: &image::RgbImage, path: &Path) -> Result<(), PluginError> {
    let mut out = Cursor::new(Vec::new());
    {
        use image::ImageEncoder;
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        encoder
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|err| PluginError::new(-32000, format!("PNG encode failed: {err}")))?;
    }
    fs::write(path, out.into_inner())
        .map_err(|err| PluginError::new(-32000, format!("Failed to write image blob: {err}")))
}

fn encode_jpeg_file(rgb: &image::RgbImage, path: &Path, quality: u8) -> Result<(), PluginError> {
    let mut out = Cursor::new(Vec::new());
    {
        use image::ImageEncoder;
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
        encoder
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|err| PluginError::new(-32000, format!("JPEG encode failed: {err}")))?;
    }
    fs::write(path, out.into_inner())
        .map_err(|err| PluginError::new(-32000, format!("Failed to write image blob: {err}")))
}

fn encode_jpeg_data_url(rgb: &image::RgbImage, quality: u8) -> Result<String, PluginError> {
    let mut out = Cursor::new(Vec::new());
    {
        use image::ImageEncoder;
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
        encoder
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|err| PluginError::new(-32000, format!("JPEG encode failed: {err}")))?;
    }
    Ok(format!(
        "data:image/jpeg;base64,{}",
        BASE64.encode(out.into_inner())
    ))
}

fn read_image_bytes(path: &Path) -> Result<Vec<u8>, PluginError> {
    fs::read(path).map_err(map_io_missing)
}

pub fn decode_image_file(path: &Path) -> Result<(u32, u32, Vec<u8>), PluginError> {
    let bytes = read_image_bytes(path)?;
    let img = image::load_from_memory(&bytes)
        .map_err(|err| PluginError::new(-32000, format!("Failed to decode image blob: {err}")))?
        .to_rgba8();
    Ok((img.width(), img.height(), img.into_raw()))
}

fn media_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("png") => "image/png",
        _ => "image/jpeg",
    }
}

fn map_io_missing(err: std::io::Error) -> PluginError {
    if err.kind() == std::io::ErrorKind::NotFound {
        PluginError::new(-32000, "Image file missing")
    } else {
        PluginError::new(-32000, format!("Failed to read image blob: {err}"))
    }
}

/// Serve a stored screenshot blob for `filesystem/media/read` (host media URL / Range).
pub fn read_media_blob(path: &Path, params: &serde_json::Value) -> Result<serde_json::Value, PluginError> {
    use serde_json::json;

    let content_type = media_content_type(path);
    let head = params
        .get("head")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if head {
        let meta = fs::metadata(path).map_err(map_io_missing)?;
        if !meta.is_file() {
            return Err(PluginError::new(-32000, "Image file missing"));
        }
        let size = meta.len();
        let etag = format!("\"{size:x}-{size}\"");
        return Ok(json!({
            "size": size,
            "contentType": content_type,
            "etag": etag,
        }));
    }

    let bytes = read_image_bytes(path)?;
    let size = bytes.len() as u64;
    let etag = format!("\"{size:x}-{size}\"");

    let (offset, end_inclusive) = match params.get("range").and_then(serde_json::Value::as_str) {
        Some(range) => match parse_byte_range(range, size)? {
            Some(bounds) => bounds,
            None => {
                return Ok(json!({
                    "size": size,
                    "rangeNotSatisfiable": true,
                    "contentType": content_type,
                    "etag": etag,
                }));
            }
        },
        None => (0, size.saturating_sub(1)),
    };

    if size == 0 {
        return Ok(json!({
            "size": 0,
            "offset": 0,
            "length": 0,
            "dataBase64": "",
            "partial": false,
            "contentType": content_type,
            "etag": etag,
        }));
    }

    let end = end_inclusive.min(size.saturating_sub(1));
    if offset > end || offset >= size {
        return Ok(json!({
            "size": size,
            "rangeNotSatisfiable": true,
            "contentType": content_type,
            "etag": etag,
        }));
    }

    let length = (end - offset + 1) as usize;
    let slice = &bytes[offset as usize..=end as usize];
    Ok(json!({
        "size": size,
        "offset": offset,
        "length": length,
        "dataBase64": BASE64.encode(slice),
        "partial": offset > 0 || end + 1 < size,
        "contentType": content_type,
        "etag": etag,
    }))
}

fn parse_byte_range(range: &str, size: u64) -> Result<Option<(u64, u64)>, PluginError> {
    let trimmed = range.trim();
    let Some(spec) = trimmed.strip_prefix("bytes=") else {
        return Err(PluginError::new(-32602, "Unsupported Range unit"));
    };
    let Some((start_raw, end_raw)) = spec.split_once('-') else {
        return Ok(None);
    };
    if size == 0 {
        return Ok(None);
    }
    if start_raw.is_empty() {
        // suffix: bytes=-N
        let suffix: u64 = end_raw
            .parse()
            .map_err(|_| PluginError::new(-32602, "Invalid Range"))?;
        if suffix == 0 {
            return Ok(None);
        }
        let start = size.saturating_sub(suffix);
        return Ok(Some((start, size - 1)));
    }
    let start: u64 = start_raw
        .parse()
        .map_err(|_| PluginError::new(-32602, "Invalid Range"))?;
    if start >= size {
        return Ok(None);
    }
    let end = if end_raw.is_empty() {
        size - 1
    } else {
        end_raw
            .parse::<u64>()
            .map_err(|_| PluginError::new(-32602, "Invalid Range"))?
            .min(size - 1)
    };
    if end < start {
        return Ok(None);
    }
    Ok(Some((start, end)))
}

/// Build a lightbox JPEG data URL kept under the Host API 2 MiB JSON bridge limit.
pub fn build_lightbox_data_url(path: &Path) -> Result<(u32, u32, String), PluginError> {
    build_media_data_url(path, false)
}

/// `full=true` keeps more resolution for annotation when the Host media URL is unavailable.
pub fn build_media_data_url(path: &Path, full: bool) -> Result<(u32, u32, String), PluginError> {
    let bytes = read_image_bytes(path)?;
    let (width, height) = image::image_dimensions(path).map_err(|err| {
        PluginError::new(-32000, format!("Failed to read image dimensions: {err}"))
    })?;

    let is_jpeg = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"));

    // Fast path: any stored JPEG that fits the bridge — base64 as-is.
    // Do NOT require width/height ≤ LIGHTBOX_EDGE: the browser scales for display,
    // and skipping decode+re-encode is what makes open feel instant.
    const FAST_JPEG_BYTES: usize = 1_400_000;
    if is_jpeg && bytes.len() <= FAST_JPEG_BYTES {
        return Ok((
            width,
            height,
            format!("data:image/jpeg;base64,{}", BASE64.encode(&bytes)),
        ));
    }

    // Slow path only for oversized blobs: cheap resize + tighter JPEG for the bridge.
    // Annotation prefers MAX_IMAGE_EDGE so strokes/mosaic stay usable.
    let max_edge = if full { MAX_IMAGE_EDGE } else { LIGHTBOX_EDGE };
    let img = image::load_from_memory(&bytes)
        .map_err(|err| PluginError::new(-32000, format!("Failed to decode image blob: {err}")))?;
    let resized = if width > max_edge || height > max_edge {
        img.resize(max_edge, max_edge, FilterType::Nearest)
    } else {
        img
    };
    let rgb = resized.to_rgb8();
    let view_w = rgb.width();
    let view_h = rgb.height();
    let quality = if full { 72 } else { 62 };
    let data_url = encode_jpeg_data_url(&rgb, quality)?;
    if data_url.len() <= 1_500_000 {
        return Ok((view_w, view_h, data_url));
    }
    Ok((view_w, view_h, encode_jpeg_data_url(&rgb, if full { 55 } else { 48 })?))
}

#[allow(dead_code)]
pub fn temp_capture_path(prefix: &str, ext: &str) -> Result<PathBuf, PluginError> {
    let dir = data_dir().join("tmp");
    fs::create_dir_all(&dir)
        .map_err(|err| PluginError::new(-32000, format!("Failed to create temp dir: {err}")))?;
    Ok(dir.join(format!(
        "{prefix}-{}.{}",
        uuid::Uuid::new_v4(),
        ext.trim_start_matches('.')
    )))
}
