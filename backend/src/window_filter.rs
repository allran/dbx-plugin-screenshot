/// Skip system chrome / transient UI that users never want to screenshot as a "window".
pub fn should_skip_window(app_name: &str, title: &str, width: u32, height: u32) -> bool {
    if width < 120 || height < 80 {
        return true;
    }

    let app = app_name.trim();
    let title = title.trim();
    if app.is_empty() && title.is_empty() {
        return true;
    }

    let app_l = app.to_ascii_lowercase();
    let title_l = title.to_ascii_lowercase();

    if SYSTEM_APP_NAMES
        .iter()
        .any(|name| app_l == name.to_ascii_lowercase())
    {
        return true;
    }

    if SYSTEM_APP_PREFIXES
        .iter()
        .any(|prefix| app_l.starts_with(&prefix.to_ascii_lowercase()))
    {
        return true;
    }

    if SYSTEM_TITLE_NAMES
        .iter()
        .any(|name| title_l == name.to_ascii_lowercase())
    {
        return true;
    }

    // Typical nameless SystemUIServer / helper surfaces.
    if app_l.contains("systemui") || app_l.contains("control center") || app_l.contains("控制中心")
    {
        return true;
    }
    if title_l.contains("control center") || title_l.contains("控制中心") {
        return true;
    }
    if title_l.contains("notification center") || title_l.contains("通知中心") {
        return true;
    }

    false
}

const SYSTEM_APP_NAMES: &[&str] = &[
    "Control Center",
    "ControlCentre",
    "控制中心",
    "Notification Center",
    "通知中心",
    "SystemUIServer",
    "Dock",
    "Window Server",
    "Wallpaper",
    "WallpaperAgent",
    "Spotlight",
    "SpotlightUIHelper",
    "TextInputMenuAgent",
    "TextInputSwitcher",
    "AirPlayUIAgent",
    "CoreServicesUIAgent",
    "loginwindow",
    "Login Window",
    "UserNotificationCenter",
    "OSDUIHelper",
    "AXVisualSupportAgent",
    "universalAccessAuthWarn",
    "Accessibility",
    "Universal Control",
    "UniversalControl",
    "FocusProxy",
    "Control Strip",
    "Touch Bar agent",
    "Wi-Fi",
    "BluetoothUIServer",
    "NowPlayingUIServer",
    "Clock",
    "BatteryUIApp",
    "NotificationCenter",
    "com.apple.controlcenter",
    "StatusIndicator",
    "CursorUIViewService",
    "App Store Preview",
    "Software Update",
    "Folder Actions Dispatcher",
    "PowerChime",
    "screencaptureui",
    "Screenshot",
    "ScreenCaptureUI",
];

const SYSTEM_APP_PREFIXES: &[&str] = &[
    "com.apple.controlcenter",
    "com.apple.notificationcenter",
    "com.apple.dock",
    "com.apple.wallpaper",
];

const SYSTEM_TITLE_NAMES: &[&str] = &[
    "Item-0",
    "FocusProxy",
    "StatusIndicator",
    "Menubar",
    "Menu Bar",
];
