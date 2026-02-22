#[cfg(target_os = "macos")]
mod app_nap;
mod plugin_engine;
mod tray;
#[cfg(target_os = "macos")]
mod webkit_config;
mod window;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::Emitter;
use tauri_plugin_log::{Target, TargetKind};
use uuid::Uuid;

#[cfg(desktop)]
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const GLOBAL_SHORTCUT_STORE_KEY: &str = "globalShortcut";
const APP_STARTED_TRACKED_DAY_KEY_PREFIX: &str = "analytics.app_started_day.";

fn app_started_day_key(version: &str) -> String {
    format!("{}{}", APP_STARTED_TRACKED_DAY_KEY_PREFIX, version)
}

fn today_utc_ymd() -> String {
    let date = time::OffsetDateTime::now_utc().date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

fn should_track_app_started(last_tracked_day: Option<&str>, today: &str) -> bool {
    match last_tracked_day {
        Some(day) => day != today,
        None => true,
    }
}

#[cfg(desktop)]
fn track_app_started_once_per_day_per_version(app: &tauri::App) {
    use tauri_plugin_store::StoreExt;

    let version = app.package_info().version.to_string();
    let key = app_started_day_key(&version);
    let today = today_utc_ymd();

    let store = match app.handle().store("settings.json") {
        Ok(store) => store,
        Err(error) => {
            log::warn!("Failed to access settings store for app_started gate: {}", error);
            return;
        }
    };

    let last_tracked_day = store
        .get(&key)
        .and_then(|value| value.as_str().map(|value| value.to_string()));

    if !should_track_app_started(last_tracked_day.as_deref(), &today) {
        return;
    }

    // aptabase removed for Windows ARM64 compatibility

    store.set(&key, serde_json::Value::String(today));
    if let Err(error) = store.save() {
        log::warn!("Failed to save app_started tracked day: {}", error);
    }
}

#[cfg(not(desktop))]
fn track_app_started_once_per_day_per_version(_app: &tauri::App) {
    // aptabase removed
}

#[cfg(desktop)]
fn managed_shortcut_slot() -> &'static Mutex<Option<String>> {
    static SLOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(desktop)]
fn handle_global_shortcut(app: &tauri::AppHandle, event: tauri_plugin_global_shortcut::ShortcutEvent) {
    if event.state == ShortcutState::Pressed {
        log::debug!("Global shortcut triggered");
        crate::window::toggle_window(app);
    }
}

pub struct AppState {
    pub plugins: Vec<plugin_engine::manifest::LoadedPlugin>,
    pub app_data_dir: PathBuf,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMeta {
    pub id: String,
    pub name: String,
    pub icon_url: String,
    pub brand_color: Option<String>,
    pub lines: Vec<ManifestLineDto>,
    pub primary_candidates: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestLineDto {
    #[serde(rename = "type")]
    pub line_type: String,
    pub label: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLinkDto {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeBatchStarted {
    pub batch_id: String,
    pub plugin_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub batch_id: String,
    pub output: plugin_engine::runtime::PluginOutput,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeBatchComplete {
    pub batch_id: String,
}

#[tauri::command]
fn init_panel() {
    // no-op: window hiding is handled by the run loop
}

#[tauri::command]
fn hide_panel(app_handle: tauri::AppHandle) {
    crate::window::hide_window(&app_handle);
}

#[tauri::command]
async fn start_probe_batch(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
    batch_id: Option<String>,
    plugin_ids: Option<Vec<String>>,
) -> Result<ProbeBatchStarted, String> {
    let batch_id = batch_id
        .and_then(|id| {
            let trimmed = id.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let (plugins, app_data_dir, app_version) = {
        let locked = state.lock().map_err(|e| e.to_string())?;
        (
            locked.plugins.clone(),
            locked.app_data_dir.clone(),
            locked.app_version.clone(),
        )
    };

    let selected_plugins = match plugin_ids {
        Some(ids) => {
            let mut by_id: HashMap<String, plugin_engine::manifest::LoadedPlugin> = plugins
                .into_iter()
                .map(|plugin| (plugin.manifest.id.clone(), plugin))
                .collect();
            let mut seen = HashSet::new();
            ids.into_iter()
                .filter_map(|id| {
                    if !seen.insert(id.clone()) {
                        return None;
                    }
                    by_id.remove(&id)
                })
                .collect()
        }
        None => plugins,
    };

    let response_plugin_ids: Vec<String> = selected_plugins
        .iter()
        .map(|plugin| plugin.manifest.id.clone())
        .collect();

    log::info!(
        "probe batch {} starting: {:?}",
        batch_id,
        response_plugin_ids
    );

    if selected_plugins.is_empty() {
        let _ = app_handle.emit(
            "probe:batch-complete",
            ProbeBatchComplete {
                batch_id: batch_id.clone(),
            },
        );
        return Ok(ProbeBatchStarted {
            batch_id,
            plugin_ids: response_plugin_ids,
        });
    }

    let remaining = Arc::new(AtomicUsize::new(selected_plugins.len()));
    for plugin in selected_plugins {
        let handle = app_handle.clone();
        let completion_handle = app_handle.clone();
        let bid = batch_id.clone();
        let completion_bid = batch_id.clone();
        let data_dir = app_data_dir.clone();
        let version = app_version.clone();
        let counter = Arc::clone(&remaining);

        tauri::async_runtime::spawn_blocking(move || {
            let plugin_id = plugin.manifest.id.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                plugin_engine::runtime::run_probe(&plugin, &data_dir, &version)
            }));

            match result {
                Ok(output) => {
                    let has_error = output.lines.iter().any(|line| {
                        matches!(line, plugin_engine::runtime::MetricLine::Badge { label, .. } if label == "Error")
                    });
                    if has_error {
                        log::warn!("probe {} completed with error", plugin_id);
                    } else {
                        log::info!("probe {} completed ok ({} lines)", plugin_id, output.lines.len());
                    }
                    let _ = handle.emit("probe:result", ProbeResult { batch_id: bid, output });
                }
                Err(_) => {
                    log::error!("probe {} panicked", plugin_id);
                }
            }

            if counter.fetch_sub(1, Ordering::SeqCst) == 1 {
                log::info!("probe batch {} complete", completion_bid);
                let _ = completion_handle.emit(
                    "probe:batch-complete",
                    ProbeBatchComplete {
                        batch_id: completion_bid,
                    },
                );
            }
        });
    }

    Ok(ProbeBatchStarted {
        batch_id,
        plugin_ids: response_plugin_ids,
    })
}

#[tauri::command]
fn get_log_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let log_dir = app_handle
        .path()
        .app_log_dir()
        .map_err(|e| format!("no app log dir: {}", e))?;
    let log_file = log_dir.join(format!("{}.log", app_handle.package_info().name));
    Ok(log_file.to_string_lossy().to_string())
}

#[cfg(desktop)]
#[tauri::command]
fn update_global_shortcut(app_handle: tauri::AppHandle, shortcut: Option<String>) -> Result<(), String> {
    let global_shortcut = app_handle.global_shortcut();
    let normalized_shortcut = shortcut.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let mut managed_shortcut = managed_shortcut_slot()
        .lock()
        .map_err(|e| format!("failed to lock managed shortcut state: {}", e))?;

    if *managed_shortcut == normalized_shortcut {
        log::debug!("Global shortcut unchanged");
        return Ok(());
    }

    let previous_shortcut = managed_shortcut.clone();
    if let Some(existing) = previous_shortcut.as_deref() {
        match global_shortcut.unregister(existing) {
            Ok(()) => {
                *managed_shortcut = None;
            }
            Err(e) => {
                log::warn!("Failed to unregister existing shortcut '{}': {}", existing, e);
            }
        }
    }

    if let Some(shortcut) = normalized_shortcut {
        log::info!("Registering global shortcut: {}", shortcut);
        global_shortcut
            .on_shortcut(shortcut.as_str(), |app, _shortcut, event| {
                handle_global_shortcut(app, event);
            })
            .map_err(|e| format!("Failed to register shortcut '{}': {}", shortcut, e))?;
        *managed_shortcut = Some(shortcut);
    } else {
        log::info!("Global shortcut disabled");
        *managed_shortcut = None;
    }

    Ok(())
}

#[tauri::command]
fn list_plugins(state: tauri::State<'_, Mutex<AppState>>) -> Vec<PluginMeta> {
    let plugins = {
        let locked = state.lock().expect("plugin state poisoned");
        locked.plugins.clone()
    };
    log::debug!("list_plugins: {} plugins", plugins.len());

    plugins
        .into_iter()
        .map(|plugin| {
            let mut candidates: Vec<_> = plugin
                .manifest
                .lines
                .iter()
                .filter(|line| line.line_type == "progress" && line.primary_order.is_some())
                .collect();
            candidates.sort_by_key(|line| line.primary_order.unwrap());
            let primary_candidates: Vec<String> =
                candidates.iter().map(|line| line.label.clone()).collect();

            PluginMeta {
                id: plugin.manifest.id,
                name: plugin.manifest.name,
                icon_url: plugin.icon_data_url,
                brand_color: plugin.manifest.brand_color,
                lines: plugin
                    .manifest
                    .lines
                    .iter()
                    .map(|line| ManifestLineDto {
                        line_type: line.line_type.clone(),
                        label: line.label.clone(),
                        scope: line.scope.clone(),
                    })
                    .collect(),
                primary_candidates,
            }
        })
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("PANIC: {}", info);
    }));

    let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _guard = runtime.enter();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .max_file_size(10_000_000)
                .level(log::LevelFilter::Trace)
                .level_for("hyper", log::LevelFilter::Warn)
                .level_for("reqwest", log::LevelFilter::Warn)
                .level_for("tao", log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            init_panel,
            hide_panel,
            start_probe_batch,
            list_plugins,
            get_log_path,
            update_global_shortcut
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            #[cfg(target_os = "macos")]
            {
                app_nap::disable_app_nap();
                webkit_config::disable_webview_suspension(app.handle());
            }

            use tauri::Manager;

            let version = app.package_info().version.to_string();
            log::info!("OpenUsage v{} starting", version);

            track_app_started_once_per_day_per_version(app);

            let app_data_dir = app.path().app_data_dir().expect("no app data dir");
            eprintln!("DEBUG: app_data_dir = {:?}", app_data_dir);

            let resource_dir = match app.path().resource_dir() {
                Ok(d) => { eprintln!("DEBUG: resource_dir = {:?}", d); d }
                Err(e) => { eprintln!("FATAL: resource_dir failed: {:?}", e); return Err(Box::new(e)); }
            };
            log::debug!("app_data_dir: {:?}", app_data_dir);

            eprintln!("DEBUG: calling initialize_plugins");
            let (_, plugins) = plugin_engine::initialize_plugins(&app_data_dir, &resource_dir);
            eprintln!("DEBUG: plugins loaded: {}", plugins.len());
            app.manage(Mutex::new(AppState {
                plugins,
                app_data_dir,
                app_version: app.package_info().version.to_string(),
            }));
            eprintln!("DEBUG: calling tray::create");

            if let Err(e) = tray::create(app.handle()) {
                eprintln!("FATAL: tray::create failed: {:?}", e);
                return Err(Box::new(e));
            }
            eprintln!("DEBUG: setup complete, entering run loop");

            #[cfg(desktop)]
            {
                use tauri_plugin_store::StoreExt;

                if let Ok(store) = app.handle().store("settings.json") {
                    if let Some(shortcut_value) = store.get(GLOBAL_SHORTCUT_STORE_KEY) {
                        if let Some(shortcut) = shortcut_value.as_str() {
                            let shortcut = shortcut.trim();
                            if !shortcut.is_empty() {
                                let handle = app.handle().clone();
                                log::info!("Registering initial global shortcut: {}", shortcut);
                                if let Err(e) = handle.global_shortcut().on_shortcut(
                                    shortcut,
                                    |app, _shortcut, event| {
                                        handle_global_shortcut(app, event);
                                    },
                                ) {
                                    log::warn!("Failed to register initial global shortcut: {}", e);
                                } else if let Ok(mut managed_shortcut) = managed_shortcut_slot().lock()
                                {
                                    *managed_shortcut = Some(shortcut.to_string());
                                } else {
                                    log::warn!("Failed to store managed shortcut in memory");
                                }
                            }
                        }
                    }
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                eprintln!("DEBUG runloop: ExitRequested");
                api.prevent_exit();
            }
            tauri::RunEvent::WindowEvent {
                ref label,
                ref event,
                ..
            } => {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } if label == "main" => {
                        eprintln!("DEBUG runloop: CloseRequested on main");
                        api.prevent_close();
                        use tauri::Manager;
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                    tauri::WindowEvent::Destroyed => {
                        eprintln!("DEBUG runloop: window '{}' Destroyed", label);
                    }
                    _ => {}
                }
            }
            tauri::RunEvent::Exit => {
                eprintln!("DEBUG runloop: Exit");
            }
            tauri::RunEvent::Ready => {
                eprintln!("DEBUG runloop: Ready");
                use tauri::Manager;
                let handle = app.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if let Some(window) = handle.get_webview_window("main") {
                        let _ = window.hide();
                    }
                });
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::{app_started_day_key, should_track_app_started};

    #[test]
    fn should_track_when_no_previous_day() {
        assert!(should_track_app_started(None, "2026-02-12"));
    }

    #[test]
    fn should_not_track_when_same_day() {
        assert!(!should_track_app_started(Some("2026-02-12"), "2026-02-12"));
    }

    #[test]
    fn should_track_when_day_changes() {
        assert!(should_track_app_started(Some("2026-02-11"), "2026-02-12"));
    }

    #[test]
    fn key_is_version_scoped() {
        let v1_key = app_started_day_key("0.6.2");
        let v2_key = app_started_day_key("0.6.3");
        assert_ne!(v1_key, v2_key);
        assert!(v1_key.ends_with("0.6.2"));
        assert!(v2_key.ends_with("0.6.3"));
    }
}
