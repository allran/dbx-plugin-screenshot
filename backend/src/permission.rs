#[cfg(target_os = "macos")]
use std::process::Command;

use dbx_plugin_sdk::PluginError;
use serde::Serialize;

#[cfg(target_os = "macos")]
use crate::util::log_stderr;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub granted: bool,
    pub can_request: bool,
    pub platform: String,
    pub message: String,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

pub fn status() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        let granted = unsafe { CGPreflightScreenCaptureAccess() };
        return PermissionStatus {
            granted,
            can_request: true,
            platform: "macos".into(),
            message: if granted {
                "Screen Recording permission is granted.".into()
            } else {
                "Screen Recording permission is required before capturing.".into()
            },
        };
    }

    #[cfg(target_os = "windows")]
    {
        return PermissionStatus {
            granted: true,
            can_request: false,
            platform: "windows".into(),
            message: "Windows does not use a separate Screen Recording toggle for this plugin.".into(),
        };
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return PermissionStatus {
            granted: true,
            can_request: false,
            platform: "linux".into(),
            message: "Linux portals may still prompt on first capture.".into(),
        };
    }

    #[allow(unreachable_code)]
    PermissionStatus {
        granted: true,
        can_request: false,
        platform: "unknown".into(),
        message: String::new(),
    }
}

pub fn request() -> Result<PermissionStatus, PluginError> {
    #[cfg(target_os = "macos")]
    {
        let granted = unsafe { CGRequestScreenCaptureAccess() };
        log_stderr(format!("CGRequestScreenCaptureAccess => {granted}"));
        // Even when the prompt appears, the process may still need a restart after the user toggles.
        let mut current = status();
        if !current.granted {
            // Already denied / unsigned helper: macOS often returns false with no UI.
            // Open Settings so the user can enable DBX (or the sidecar binary) manually.
            if let Err(err) = open_settings() {
                log_stderr(format!("open_settings after request failed: {err:?}"));
            }
            current.message = "no_dialog".into();
        }
        return Ok(current);
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(status())
    }
}

pub fn open_settings() -> Result<(), PluginError> {
    #[cfg(target_os = "macos")]
    {
        // Prefer modern Settings deep link; fall back to classic preference pane URL.
        let modern = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .status();
        match modern {
            Ok(status) if status.success() => return Ok(()),
            other => log_stderr(format!("open Screen Recording settings failed: {other:?}")),
        }
        Command::new("open")
            .arg("/System/Library/PreferencePanes/Security.prefPane")
            .status()
            .map_err(|err| PluginError::new(-32000, format!("Failed to open System Settings: {err}")))?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(PluginError::new(
            -32000,
            "Opening screen-capture settings is only implemented on macOS",
        ))
    }
}
