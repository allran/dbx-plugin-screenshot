use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use dbx_plugin_sdk::PluginError;
use image::{ImageBuffer, ImageEncoder, RgbaImage};
use minimp4::Mp4Muxer;
use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, UsageType,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use openh264::{OpenH264API, Timestamp};
use uuid::Uuid;
use xcap::Monitor;

use crate::media::{maybe_downscale, rgba_jpeg_data_url, temp_capture_path};
use crate::model::ScreenshotItem;
use crate::permission;
use crate::prefs;
use crate::util::{data_dir, log_stderr, now_ms};

const JPEG_QUALITY: u8 = 72;
const MAX_DURATION: Duration = Duration::from_secs(10 * 60);
/// Abort capture when free space drops below this (still mux whatever we have).
const MIN_FREE_BYTES: u64 = 200 * 1024 * 1024;
const DISK_CHECK_EVERY_FRAMES: u64 = 24;
const ERR_BUSY: i32 = -32013;
const ERR_NOT_RECORDING: i32 = -32014;
pub const ERR_DISK: i32 = -32015;

pub struct RecordingController {
    state: Mutex<ControllerState>,
    pending: Mutex<Option<Result<RecordedClip, PluginError>>>,
}

enum ControllerState {
    Idle,
    Recording(ActiveRecording),
    /// Capture thread has been signaled/joined; waiting to hand off to Finalizing.
    /// Prevents a second `start` while `stop` is joining the capture thread.
    Stopping,
    Finalizing(FinalizingRecording),
}

struct ActiveRecording {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Result<CapturedSession, PluginError>>,
    started_at_ms: u64,
    display_index: u32,
    display_name: String,
}

struct FinalizingRecording {
    handle: JoinHandle<Result<RecordedClip, PluginError>>,
}

struct CapturedSession {
    frames_dir: PathBuf,
    frame_count: u64,
    duration_ms: u64,
    width: u32,
    height: u32,
    display_index: u32,
    display_name: String,
    preview_data_url: Option<String>,
    target_fps: u32,
}

pub struct RecordedClip {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub display_index: u32,
    pub display_name: String,
    pub preview_data_url: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub recording: bool,
    pub finalizing: bool,
    pub started_at: Option<u64>,
    pub elapsed_ms: u64,
    pub display_index: Option<u32>,
    pub display_name: Option<String>,
}

impl RecordingController {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ControllerState::Idle),
            pending: Mutex::new(None),
        }
    }

    /// Promote finished capture / join finished mux, then report status.
    pub fn poll_status(&self) -> RecordingStatus {
        self.promote_finished_capture();
        self.collect_finished_finalize();
        self.status_locked()
    }

    pub fn take_pending(&self) -> Option<Result<RecordedClip, PluginError>> {
        self.promote_finished_capture();
        self.collect_finished_finalize();
        self.pending.lock().ok().and_then(|mut g| g.take())
    }

    fn status_locked(&self) -> RecordingStatus {
        let guard = self.state.lock().ok();
        let Some(guard) = guard else {
            return idle_status();
        };
        match &*guard {
            ControllerState::Idle => idle_status(),
            ControllerState::Recording(active) => RecordingStatus {
                recording: true,
                finalizing: false,
                started_at: Some(active.started_at_ms),
                elapsed_ms: now_ms().saturating_sub(active.started_at_ms),
                display_index: Some(active.display_index),
                display_name: Some(active.display_name.clone()),
            },
            // Stopping / Finalizing both surface as "saving" to the UI.
            ControllerState::Stopping | ControllerState::Finalizing(_) => RecordingStatus {
                recording: false,
                finalizing: true,
                started_at: None,
                elapsed_ms: 0,
                display_index: None,
                display_name: None,
            },
        }
    }

    fn stash_pending(&self, result: Result<RecordedClip, PluginError>) {
        match self.pending.lock() {
            Ok(mut pending) => *pending = Some(result),
            Err(poisoned) => {
                *poisoned.into_inner() = Some(result);
            }
        }
    }

    /// If capture ended on its own (max duration / disk), start mux without waiting for stop().
    fn promote_finished_capture(&self) {
        let Ok(mut guard) = self.state.lock() else {
            return;
        };
        let finished = matches!(
            &*guard,
            ControllerState::Recording(active) if active.handle.is_finished()
        );
        if !finished {
            return;
        }
        let ControllerState::Recording(active) =
            std::mem::replace(&mut *guard, ControllerState::Stopping)
        else {
            return;
        };
        // Drop the lock while joining so other status polls can observe Stopping.
        drop(guard);
        let captured = match active.handle.join() {
            Ok(Ok(session)) => session,
            Ok(Err(err)) => {
                self.stash_pending(Err(err));
                if let Ok(mut g) = self.state.lock() {
                    *g = ControllerState::Idle;
                }
                return;
            }
            Err(_) => {
                self.stash_pending(Err(PluginError::new(
                    -32000,
                    "Recording thread panicked",
                )));
                if let Ok(mut g) = self.state.lock() {
                    *g = ControllerState::Idle;
                }
                return;
            }
        };
        log_stderr(format!(
            "capture finished on its own: {} frames — finalizing in background",
            captured.frame_count
        ));
        let handle = thread::spawn(move || finalize_captured(captured));
        if let Ok(mut g) = self.state.lock() {
            *g = ControllerState::Finalizing(FinalizingRecording { handle });
        }
    }

    fn collect_finished_finalize(&self) {
        let Ok(mut guard) = self.state.lock() else {
            return;
        };
        let finished = matches!(
            &*guard,
            ControllerState::Finalizing(fin) if fin.handle.is_finished()
        );
        if !finished {
            return;
        }
        let ControllerState::Finalizing(fin) =
            std::mem::replace(&mut *guard, ControllerState::Idle)
        else {
            return;
        };
        drop(guard);
        let result = match fin.handle.join() {
            Ok(inner) => inner,
            Err(_) => Err(PluginError::new(-32000, "Finalize thread panicked")),
        };
        self.stash_pending(result);
    }

    pub fn start(&self, display_index: Option<u32>) -> Result<RecordingStatus, PluginError> {
        ensure_permission()?;
        ensure_disk_for_recording()?;

        let mut guard = self
            .state
            .lock()
            .map_err(|_| PluginError::new(-32000, "Recording state is poisoned"))?;
        match &*guard {
            ControllerState::Idle => {}
            ControllerState::Recording(_) => {
                return Err(PluginError::new(ERR_BUSY, "Already recording"));
            }
            ControllerState::Stopping | ControllerState::Finalizing(_) => {
                return Err(PluginError::new(
                    ERR_BUSY,
                    "Still saving the previous recording",
                ));
            }
        }

        let monitors = Monitor::all().map_err(|err| {
            PluginError::new(
                crate::capture::ERR_PERMISSION,
                format!("Unable to access displays: {err}"),
            )
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
                format!(
                    "Display index {index} is out of range (0..{})",
                    monitors.len()
                ),
            )
        })?;
        let display_name = monitor
            .name()
            .unwrap_or_else(|_| format!("Display {}", index + 1));
        let display_index = index as u32;
        let _ = monitor;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let name_thread = display_name.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), PluginError>>();
        let handle = thread::spawn(move || {
            capture_session(stop_thread, display_index, name_thread, ready_tx)
        });

        match ready_rx.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                let _ = handle.join();
                return Err(err);
            }
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                let _ = handle.join();
                return Err(PluginError::new(
                    -32000,
                    "Timed out waiting for screen recorder to start",
                ));
            }
        }

        let started_at_ms = now_ms();
        *guard = ControllerState::Recording(ActiveRecording {
            stop,
            handle,
            started_at_ms,
            display_index,
            display_name: display_name.clone(),
        });

        log_stderr(format!(
            "recording started on display {display_index} ({display_name})"
        ));
        Ok(RecordingStatus {
            recording: true,
            finalizing: false,
            started_at: Some(started_at_ms),
            elapsed_ms: 0,
            display_index: Some(display_index),
            display_name: Some(display_name),
        })
    }

    /// Stop capture and kick off MP4 mux on a background thread (RPC returns quickly).
    pub fn stop(&self) -> Result<RecordingStatus, PluginError> {
        self.promote_finished_capture();
        self.collect_finished_finalize();

        let active = {
            let mut guard = self
                .state
                .lock()
                .map_err(|_| PluginError::new(-32000, "Recording state is poisoned"))?;
            match std::mem::replace(&mut *guard, ControllerState::Stopping) {
                ControllerState::Recording(active) => active,
                ControllerState::Stopping => {
                    return Ok(RecordingStatus {
                        recording: false,
                        finalizing: true,
                        started_at: None,
                        elapsed_ms: 0,
                        display_index: None,
                        display_name: None,
                    });
                }
                ControllerState::Finalizing(fin) => {
                    *guard = ControllerState::Finalizing(fin);
                    return Ok(RecordingStatus {
                        recording: false,
                        finalizing: true,
                        started_at: None,
                        elapsed_ms: 0,
                        display_index: None,
                        display_name: None,
                    });
                }
                ControllerState::Idle => {
                    *guard = ControllerState::Idle;
                    // Mux may have just finished — caller should take_pending.
                    if self.pending.lock().ok().is_some_and(|p| p.is_some()) {
                        return Ok(RecordingStatus {
                            recording: false,
                            finalizing: false,
                            started_at: None,
                            elapsed_ms: 0,
                            display_index: None,
                            display_name: None,
                        });
                    }
                    return Err(PluginError::new(ERR_NOT_RECORDING, "Not recording"));
                }
            }
        };

        active.stop.store(true, Ordering::SeqCst);
        let captured = match active.handle.join() {
            Ok(Ok(session)) => session,
            Ok(Err(err)) => {
                if let Ok(mut g) = self.state.lock() {
                    *g = ControllerState::Idle;
                }
                return Err(err);
            }
            Err(_) => {
                if let Ok(mut g) = self.state.lock() {
                    *g = ControllerState::Idle;
                }
                return Err(PluginError::new(-32000, "Recording thread panicked"));
            }
        };

        log_stderr(format!(
            "capture stopped: {} frames, {}ms — finalizing in background",
            captured.frame_count, captured.duration_ms
        ));

        let handle = thread::spawn(move || finalize_captured(captured));
        let mut guard = self
            .state
            .lock()
            .map_err(|_| PluginError::new(-32000, "Recording state is poisoned"))?;
        *guard = ControllerState::Finalizing(FinalizingRecording { handle });

        Ok(RecordingStatus {
            recording: false,
            finalizing: true,
            started_at: None,
            elapsed_ms: 0,
            display_index: None,
            display_name: None,
        })
    }
}

fn idle_status() -> RecordingStatus {
    RecordingStatus {
        recording: false,
        finalizing: false,
        started_at: None,
        elapsed_ms: 0,
        display_index: None,
        display_name: None,
    }
}

fn ensure_permission() -> Result<(), PluginError> {
    let status = permission::status();
    if status.granted {
        return Ok(());
    }
    Err(PluginError::new(
        crate::capture::ERR_PERMISSION,
        status.message,
    ))
}

fn ensure_disk_for_recording() -> Result<(), PluginError> {
    let prefs = prefs::get();
    let need = estimate_recording_bytes(prefs.record_fps, prefs.record_max_edge);
    let dir = data_dir();
    fs::create_dir_all(&dir)
        .map_err(|err| PluginError::new(-32000, format!("Failed to create data dir: {err}")))?;
    let Some(free) = available_disk_bytes(&dir) else {
        log_stderr("disk preflight: unable to query free space; continuing");
        return Ok(());
    };
    if free < need {
        return Err(PluginError::new(
            ERR_DISK,
            format!(
                "Not enough disk space for recording (need ~{} MB free, have {} MB)",
                need / (1024 * 1024),
                free / (1024 * 1024)
            ),
        ));
    }
    log_stderr(format!(
        "disk preflight ok: ~{} MB free (budget ~{} MB)",
        free / (1024 * 1024),
        need / (1024 * 1024)
    ));
    Ok(())
}

fn estimate_recording_bytes(fps: u32, max_edge: u32) -> u64 {
    let per_frame = match max_edge {
        0..=720 => 100_000u64,
        721..=1080 => 180_000,
        1081..=1440 => 280_000,
        _ => 400_000,
    };
    let frames = u64::from(fps.max(1)) * MAX_DURATION.as_secs();
    let jpeg_budget = frames.saturating_mul(per_frame);
    let mp4_budget = jpeg_budget / 3;
    let margin = 64 * 1024 * 1024;
    jpeg_budget
        .saturating_add(mp4_budget)
        .saturating_add(margin)
}

fn available_disk_bytes(path: &Path) -> Option<u64> {
    fs2::available_space(path).ok()
}

fn capture_session(
    stop: Arc<AtomicBool>,
    display_index: u32,
    display_name: String,
    ready_tx: mpsc::Sender<Result<(), PluginError>>,
) -> Result<CapturedSession, PluginError> {
    let frames_dir = data_dir()
        .join("tmp")
        .join(format!("record-{}", Uuid::new_v4()));
    let cleanup_dir = frames_dir.clone();
    let result = (|| {
        let start_result = (|| -> Result<_, PluginError> {
            fs::create_dir_all(&frames_dir).map_err(|err| {
                PluginError::new(-32000, format!("Failed to create frame dir: {err}"))
            })?;
            let monitors = Monitor::all().map_err(|err| {
                PluginError::new(
                    crate::capture::ERR_PERMISSION,
                    format!("Unable to access displays: {err}"),
                )
            })?;
            let monitor = monitors.get(display_index as usize).ok_or_else(|| {
                PluginError::new(
                    -32602,
                    format!("Display index {display_index} is out of range"),
                )
            })?;
            let (recorder, frames) = monitor.video_recorder().map_err(|err| {
                PluginError::new(
                    crate::capture::ERR_PERMISSION,
                    format!("Unable to start screen recorder: {err}"),
                )
            })?;
            recorder.start().map_err(|err| {
                PluginError::new(-32000, format!("Failed to start recorder: {err}"))
            })?;
            Ok((recorder, frames))
        })();

        let (recorder, frames) = match start_result {
            Ok(value) => {
                let _ = ready_tx.send(Ok(()));
                value
            }
            Err(err) => {
                let _ = ready_tx.send(Err(PluginError::new(err.code, err.message.clone())));
                return Err(err);
            }
        };

        let prefs = prefs::get();
        let target_fps = prefs.record_fps.max(1);
        let max_edge = prefs.record_max_edge.max(2);
        log_stderr(format!(
            "record prefs: fps={target_fps} maxEdge={max_edge}"
        ));

        let started = Instant::now();
        let frame_interval = Duration::from_millis(1000 / u64::from(target_fps));
        let mut out_w = 0u32;
        let mut out_h = 0u32;
        let mut frame_index = 0u64;
        let mut preview_rgba: Option<(u32, u32, Vec<u8>)> = None;
        let mut next_due = Instant::now();
        let mut stopped_for_disk = false;

        while !stop.load(Ordering::SeqCst) {
            if started.elapsed() >= MAX_DURATION {
                log_stderr("recording hit max duration");
                break;
            }

            let Some(frame) = recv_latest_frame(&frames, &stop, next_due)? else {
                break;
            };
            next_due = Instant::now() + frame_interval;

            let prepared = prepare_frame(&frame, max_edge)?;
            out_w = prepared.width;
            out_h = prepared.height;
            if preview_rgba.is_none() {
                preview_rgba = Some((prepared.width, prepared.height, prepared.rgba.clone()));
            }

            let path = frames_dir.join(format!("frame_{frame_index:06}.jpg"));
            write_jpeg_file(&path, prepared.width, prepared.height, &prepared.rgba)?;
            frame_index += 1;

            if frame_index % DISK_CHECK_EVERY_FRAMES == 0 {
                if let Some(free) = available_disk_bytes(&frames_dir) {
                    if free < MIN_FREE_BYTES {
                        log_stderr(format!(
                            "disk low during recording ({} MB free); stopping capture",
                            free / (1024 * 1024)
                        ));
                        stopped_for_disk = true;
                        break;
                    }
                }
            }
        }

        let _ = recorder.stop();
        while frames.try_recv().is_ok() {}

        if frame_index == 0 {
            return Err(PluginError::new(
                if stopped_for_disk {
                    ERR_DISK
                } else {
                    crate::capture::ERR_PERMISSION
                },
                if stopped_for_disk {
                    "Not enough disk space to capture frames"
                } else {
                    "No frames captured — check Screen Recording permission"
                },
            ));
        }

        let duration_ms = started.elapsed().as_millis() as u64;
        log_stderr(format!(
            "captured {frame_index} jpeg frames over {duration_ms}ms → {out_w}×{out_h}"
        ));

        let preview_data_url = match preview_rgba {
            Some((w, h, rgba)) => {
                let prefs = prefs::get();
                Some(rgba_jpeg_data_url(
                    w,
                    h,
                    &rgba,
                    prefs.thumb_edge(),
                    prefs.thumb_jpeg_quality(),
                )?)
            }
            None => None,
        };

        Ok(CapturedSession {
            frames_dir: frames_dir.clone(),
            frame_count: frame_index,
            duration_ms,
            width: out_w,
            height: out_h,
            display_index,
            display_name,
            preview_data_url,
            target_fps,
        })
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&cleanup_dir);
    }
    result
}

fn finalize_captured(session: CapturedSession) -> Result<RecordedClip, PluginError> {
    let cleanup_dir = session.frames_dir.clone();
    let result = (|| {
        let path = assemble_mp4(
            &session.frames_dir,
            session.frame_count,
            session.duration_ms,
            session.width,
            session.height,
            session.target_fps,
        )?;
        Ok(RecordedClip {
            path,
            width: session.width,
            height: session.height,
            duration_ms: session.duration_ms,
            display_index: session.display_index,
            display_name: session.display_name,
            preview_data_url: session.preview_data_url,
        })
    })();
    let _ = fs::remove_dir_all(&cleanup_dir);
    result
}

struct PreparedFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn prepare_frame(frame: &xcap::Frame, max_edge: u32) -> Result<PreparedFrame, PluginError> {
    let expected = (frame.width as usize)
        .saturating_mul(frame.height as usize)
        .saturating_mul(4);
    if frame.raw.len() < expected || frame.width == 0 || frame.height == 0 {
        return Err(PluginError::new(-32000, "Invalid capture frame"));
    }

    let (mut w, mut h, mut rgba) =
        maybe_downscale(frame.width, frame.height, &frame.raw[..expected], max_edge)?;

    let even_w = w & !1;
    let even_h = h & !1;
    if even_w == 0 || even_h == 0 {
        return Err(PluginError::new(-32000, "Frame too small to encode"));
    }
    if even_w != w || even_h != h {
        let img: RgbaImage = ImageBuffer::from_raw(w, h, rgba)
            .ok_or_else(|| PluginError::new(-32000, "Invalid frame buffer"))?;
        let cropped = image::imageops::crop_imm(&img, 0, 0, even_w, even_h).to_image();
        w = even_w;
        h = even_h;
        rgba = cropped.into_raw();
    }

    Ok(PreparedFrame {
        width: w,
        height: h,
        rgba,
    })
}

fn write_jpeg_file(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), PluginError> {
    let img: RgbaImage = ImageBuffer::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| PluginError::new(-32000, "Invalid frame buffer"))?;
    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let mut out = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|err| PluginError::new(-32000, format!("JPEG encode failed: {err}")))?;
    fs::write(path, out.into_inner())
        .map_err(|err| PluginError::new(-32000, format!("Failed to write frame: {err}")))
}

/// Wait until `due`, then return the newest queued frame (drop older backlog).
fn recv_latest_frame(
    frames: &mpsc::Receiver<xcap::Frame>,
    stop: &AtomicBool,
    due: Instant,
) -> Result<Option<xcap::Frame>, PluginError> {
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let now = Instant::now();
        let wait = if due > now {
            due.saturating_duration_since(now)
                .min(Duration::from_millis(200))
        } else {
            Duration::from_millis(0)
        };
        match frames.recv_timeout(wait) {
            Ok(mut frame) => {
                if Instant::now() < due && !stop.load(Ordering::SeqCst) {
                    while Instant::now() < due && !stop.load(Ordering::SeqCst) {
                        match frames.recv_timeout(due.saturating_duration_since(Instant::now())) {
                            Ok(newer) => frame = newer,
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(Some(frame)),
                        }
                    }
                }
                while let Ok(newer) = frames.try_recv() {
                    frame = newer;
                }
                return Ok(Some(frame));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::SeqCst) {
                    return Ok(None);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
        }
    }
}

fn effective_fps(frame_count: u64, duration_ms: u64, target_fps: u32) -> f64 {
    let cap = f64::from(target_fps.max(1));
    if frame_count == 0 || duration_ms == 0 {
        return cap;
    }
    let fps = (frame_count as f64) * 1000.0 / (duration_ms as f64);
    fps.clamp(0.2, cap)
}

fn assemble_mp4(
    frames_dir: &Path,
    frame_count: u64,
    duration_ms: u64,
    width: u32,
    height: u32,
    target_fps: u32,
) -> Result<PathBuf, PluginError> {
    let path = temp_capture_path("record", "mp4")?;
    let fps = effective_fps(frame_count, duration_ms, target_fps);
    let pattern = frames_dir.join("frame_%06d.jpg");

    if mux_jpegs_with_ffmpeg(&pattern, &path, fps)? {
        log_stderr(format!(
            "muxed jpeg sequence with ffmpeg at {fps:.3} fps ({frame_count} frames / {duration_ms}ms)"
        ));
        return Ok(path);
    }

    log_stderr("ffmpeg unavailable; encoding JPEG sequence with OpenH264");
    encode_jpegs_with_openh264(
        frames_dir,
        frame_count,
        duration_ms,
        width,
        height,
        target_fps,
        &path,
    )?;
    Ok(path)
}

fn mux_jpegs_with_ffmpeg(pattern: &Path, out: &Path, fps: f64) -> Result<bool, PluginError> {
    // ffmpeg is happier with forward slashes even on Windows.
    let pattern_arg = pattern.to_string_lossy().replace('\\', "/");
    let out_arg = out.to_string_lossy().replace('\\', "/");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-framerate",
            &format!("{fps:.6}"),
            "-i",
            &pattern_arg,
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-an",
            "-movflags",
            "+faststart",
            &out_arg,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status();

    match status {
        Ok(code) if code.success() && out.exists() => Ok(true),
        Ok(_) | Err(_) => {
            let _ = fs::remove_file(out);
            Ok(false)
        }
    }
}

fn encode_jpegs_with_openh264(
    frames_dir: &Path,
    frame_count: u64,
    duration_ms: u64,
    width: u32,
    height: u32,
    target_fps: u32,
    out: &Path,
) -> Result<(), PluginError> {
    let fps = effective_fps(frame_count, duration_ms, target_fps);
    let int_fps = fps.round().clamp(1.0, f64::from(target_fps.max(1))) as u32;
    let cfg = EncoderConfig::new()
        .usage_type(UsageType::ScreenContentNonRealTime)
        .complexity(Complexity::Low)
        .max_frame_rate(FrameRate::from_hz(int_fps as f32))
        .bitrate(BitRate::from_bps(bitrate_for(width, height)))
        .intra_frame_period(IntraFramePeriod::from_num_frames(int_fps * 2));
    let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), cfg)
        .map_err(|err| PluginError::new(-32000, format!("Failed to create H.264 encoder: {err}")))?;

    let mut encoded = Vec::<u8>::new();
    for i in 0..frame_count {
        let path = frames_dir.join(format!("frame_{i:06}.jpg"));
        let img = image::open(&path)
            .map_err(|err| PluginError::new(-32000, format!("Failed to read frame: {err}")))?
            .to_rgba8();
        let rgb_source = RgbaSliceU8::new(img.as_raw(), (img.width() as usize, img.height() as usize));
        let yuv = YUVBuffer::from_rgb_source(rgb_source);
        let timestamp = Timestamp::from_millis(i * (1000 / u64::from(int_fps.max(1))));
        let bitstream = encoder
            .encode_at(&yuv, timestamp)
            .map_err(|err| PluginError::new(-32000, format!("H.264 encode failed: {err}")))?;
        bitstream.write_vec(&mut encoded);
    }

    if encoded.is_empty() {
        return Err(PluginError::new(-32000, "H.264 encode produced no data"));
    }

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut muxer = Mp4Muxer::new(&mut cursor);
        muxer.init_video(width as i32, height as i32, false, "DBX Screenshot");
        muxer.write_video_with_fps(&encoded, int_fps);
        muxer.close();
    }
    cursor
        .seek(SeekFrom::Start(0))
        .map_err(|err| PluginError::new(-32000, format!("MP4 seek failed: {err}")))?;
    let mut bytes = Vec::new();
    cursor
        .read_to_end(&mut bytes)
        .map_err(|err| PluginError::new(-32000, format!("MP4 read failed: {err}")))?;
    File::create(out)
        .and_then(|mut f| f.write_all(&bytes))
        .map_err(|err| PluginError::new(-32000, format!("Failed to write MP4: {err}")))?;
    Ok(())
}

fn bitrate_for(width: u32, height: u32) -> u32 {
    let pixels = u64::from(width) * u64::from(height);
    let bps = ((pixels * 10 * 2) / 10).clamp(1_000_000, 8_000_000);
    bps as u32
}

pub fn build_recording_item(id: &str, clip: &RecordedClip) -> Result<ScreenshotItem, PluginError> {
    let blobs = data_dir().join("blobs");
    fs::create_dir_all(&blobs)
        .map_err(|err| PluginError::new(-32000, format!("Failed to create blobs dir: {err}")))?;
    let rel = format!("blobs/{id}.mp4");
    let dest = data_dir().join(&rel);
    fs::copy(&clip.path, &dest)
        .map_err(|err| PluginError::new(-32000, format!("Failed to store recording: {err}")))?;
    let _ = fs::remove_file(&clip.path);

    let secs = (clip.duration_ms as f64 / 1000.0).round() as u64;
    Ok(ScreenshotItem {
        id: id.to_string(),
        created_at: now_ms(),
        label: format!(
            "{} · {}×{} · {}s",
            clip.display_name, clip.width, clip.height, secs
        ),
        width: Some(clip.width),
        height: Some(clip.height),
        display_index: Some(clip.display_index),
        display_name: Some(clip.display_name.clone()),
        capture_mode: Some("record".into()),
        media_file: Some(rel),
        preview_data_url: clip.preview_data_url.clone(),
        favorite: false,
        duration_ms: Some(clip.duration_ms),
    })
}

pub fn is_video_item(item: &ScreenshotItem) -> bool {
    item.capture_mode
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("record"))
        || item.media_file.as_deref().is_some_and(|p| {
            p.rsplit('.')
                .next()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
        })
}
