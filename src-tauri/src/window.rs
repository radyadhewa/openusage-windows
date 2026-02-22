use tauri::{AppHandle, Manager};

/// Show the popup window positioned above the system tray.
pub fn show_window(app_handle: &AppHandle) {
    let Some(window) = app_handle.get_webview_window("main") else {
        log::error!("window: main window not found");
        return;
    };

    position_bottom_right(&window);

    if let Err(e) = window.show() {
        log::error!("window: failed to show: {}", e);
        return;
    }
    if let Err(e) = window.set_focus() {
        log::warn!("window: failed to set focus: {}", e);
    }
}

/// Position the window at the bottom-right of the primary monitor's work area (above taskbar).
fn position_bottom_right(window: &tauri::WebviewWindow) {
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten();

    let Some(monitor) = monitor else {
        log::warn!("window: no monitor found, skipping positioning");
        return;
    };

    let scale = monitor.scale_factor();
    let work_area = monitor.position();
    let monitor_size = monitor.size();

    let win_w = 400.0_f64;
    let win_h = 500.0_f64;

    let win_w_phys = (win_w * scale) as i32;
    let win_h_phys = (win_h * scale) as i32;

    let taskbar_h_phys = (48.0 * scale) as i32;
    let margin_phys = (8.0 * scale) as i32;

    let x = work_area.x + monitor_size.width as i32 - win_w_phys - margin_phys;
    let y = work_area.y + monitor_size.height as i32 - win_h_phys - taskbar_h_phys - margin_phys;

    log::debug!(
        "window: positioning to ({}, {}) [scale={}, monitor={}x{}]",
        x, y, scale, monitor_size.width, monitor_size.height
    );

    if let Err(e) = window.set_position(tauri::PhysicalPosition::new(x, y)) {
        log::warn!("window: set_position failed: {}", e);
    }
}

/// Hide the popup window.
pub fn hide_window(app_handle: &AppHandle) {
    let Some(window) = app_handle.get_webview_window("main") else {
        return;
    };
    if let Err(e) = window.hide() {
        log::warn!("window: failed to hide: {}", e);
    }
}

/// Toggle the popup window.
pub fn toggle_window(app_handle: &AppHandle) {
    let Some(window) = app_handle.get_webview_window("main") else {
        log::error!("window: main window not found for toggle");
        return;
    };

    let is_visible = window.is_visible().unwrap_or(false);
    if is_visible {
        log::debug!("toggle_window: hiding");
        hide_window(app_handle);
    } else {
        log::debug!("toggle_window: showing");
        show_window(app_handle);
    }
}
