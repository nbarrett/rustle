#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

use rustle_core::audio::list_input_device_names;
use rustle_core::config::{load_config, model_catalog, save_config, Config, ModelChoice};
use rustle_core::download::download_model_file;
use rustle_core::engine::{DictationEngine, DictationStatus};
use rustle_core::hotkey::{HotkeyChoice, HotkeyOption};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{AutoLaunchManager, MacosLauncher};

const LAUNCHED_AT_LOGIN_ARGUMENT: &str = "--launched-at-login";

mod overlay;
#[cfg(target_os = "macos")]
mod mac_setup;

use overlay::DictationOverlay;

struct EngineState {
    engine: DictationEngine,
}

struct FeedbackState {
    overlay: DictationOverlay,
    tray: TrayIcon,
}

fn describe_status(status: &DictationStatus) -> serde_json::Value {
    match status {
        DictationStatus::Idle => serde_json::json!({ "kind": "idle" }),
        DictationStatus::Listening => serde_json::json!({ "kind": "listening" }),
        DictationStatus::Transcribing => serde_json::json!({ "kind": "transcribing" }),
        DictationStatus::Partial(text) => serde_json::json!({ "kind": "partial", "text": text }),
        DictationStatus::Typed(text) => serde_json::json!({ "kind": "typed", "text": text }),
        DictationStatus::SettingsPreview(text) => {
            serde_json::json!({ "kind": "settings_preview", "text": text })
        }
        DictationStatus::Failed(message) => {
            serde_json::json!({ "kind": "failed", "text": message })
        }
        DictationStatus::NeedsPermission(message) => {
            serde_json::json!({ "kind": "needs_permission", "text": message })
        }
    }
}

#[tauri::command]
fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

fn current_engine_config(app: &AppHandle) -> Config {
    if let Some(state) = app.try_state::<Mutex<EngineState>>() {
        if let Ok(guard) = state.lock() {
            return guard.engine.current_config();
        }
    }
    load_config().unwrap_or_default()
}

#[tauri::command]
fn get_config(app: AppHandle) -> Config {
    current_engine_config(&app)
}

#[tauri::command]
fn save_and_apply_config(
    app: AppHandle,
    new_config: Config,
    hide_window: Option<bool>,
) -> Result<(), String> {
    save_config(&new_config).map_err(|error| error.to_string())?;
    apply_launch_at_login(&app, new_config.launch_at_login);
    if let Some(state) = app.try_state::<Mutex<EngineState>>() {
        if let Ok(guard) = state.lock() {
            guard.engine.apply_config(new_config);
        }
    }
    if hide_window.unwrap_or(true) {
        conceal_settings_window(&app);
    }
    Ok(())
}

fn apply_launch_at_login(app: &AppHandle, enabled: bool) {
    let Some(manager) = app.try_state::<AutoLaunchManager>() else {
        return;
    };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let _ = if enabled {
            manager.enable()
        } else {
            manager.disable()
        };
    }));
}

#[tauri::command]
fn list_microphones() -> Result<Vec<String>, String> {
    list_input_device_names().map_err(|error| error.to_string())
}

#[tauri::command]
fn list_models() -> Vec<ModelChoice> {
    model_catalog()
}

#[tauri::command]
fn set_dictation_enabled(enabled: bool, app: AppHandle) {
    if let Some(state) = app.try_state::<Mutex<EngineState>>() {
        if let Ok(guard) = state.lock() {
            guard.engine.set_listening_enabled(enabled);
        }
    }
}

#[tauri::command]
fn notify_hotkey_edge(pressed: bool, app: AppHandle) {
    if let Some(state) = app.try_state::<Mutex<EngineState>>() {
        if let Ok(guard) = state.lock() {
            guard.engine.notify_hotkey_edge(pressed);
        }
    }
}

#[tauri::command]
fn get_dictation_enabled(app: AppHandle) -> bool {
    if let Some(state) = app.try_state::<Mutex<EngineState>>() {
        if let Ok(guard) = state.lock() {
            return guard.engine.is_listening_enabled();
        }
    }
    true
}

#[tauri::command]
async fn download_model(app: AppHandle, file_name: String, download_url: String) -> Result<(), String> {
    let progress_app = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        download_model_file(&file_name, &download_url, |received, total| {
            let percent = total
                .filter(|total| *total > 0)
                .map(|total| received as f64 / total as f64 * 100.0);
            let _ = progress_app.emit(
                "model-download-progress",
                serde_json::json!({ "received": received, "total": total, "percent": percent }),
            );
        })
    })
    .await
    .map_err(|error| error.to_string())?;

    outcome.map_err(|error| error.to_string())?;
    let _ = app.emit("model-download-complete", ());
    Ok(())
}

#[tauri::command]
fn show_settings_window(app: AppHandle) {
    reveal_settings_window(&app);
}

#[tauri::command]
fn list_hotkey_choices() -> Vec<HotkeyOption> {
    HotkeyChoice::available()
        .into_iter()
        .map(HotkeyChoice::option)
        .collect()
}

#[tauri::command]
fn host_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
fn macos_setup_status() -> serde_json::Value {
    #[cfg(target_os = "macos")]
    {
        serde_json::to_value(mac_setup::current_setup_status()).unwrap_or_else(|_| {
            serde_json::json!({
                "in_applications": false,
                "listen": false,
                "accessibility": false
            })
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        serde_json::json!({
            "in_applications": true,
            "listen": true,
            "accessibility": true
        })
    }
}

#[tauri::command]
fn install_into_applications(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        mac_setup::install_into_applications_and_relaunch(&app)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(())
    }
}

#[tauri::command]
fn request_dictation_permissions() {
    #[cfg(target_os = "macos")]
    {
        mac_setup::request_dictation_permissions();
        mac_setup::open_dictation_permission_settings();
    }
}

#[tauri::command]
fn write_utf8_path(path: String, contents: String) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_utf8_path(path: String) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        mac_setup::open_dictation_permission_settings();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:privacy-microphone"])
            .status();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg("settings://")
            .status();
    }
}

#[tauri::command]
fn resize_settings_window(app: AppHandle, content_height: f64) {
    let Some(window) = app.get_webview_window("settings") else {
        return;
    };
    let (Ok(inner), Ok(outer), Ok(scale)) = (
        window.inner_size(),
        window.outer_size(),
        window.scale_factor(),
    ) else {
        return;
    };
    let title_bar_minimum = 28.0 * scale;
    let chrome_height = (outer.height as f64 - inner.height as f64).max(title_bar_minimum);
    let bottom_margin = 8.0 * scale;
    let target_height =
        ((content_height * scale + chrome_height + bottom_margin).round() as u32).max(200);
    let locked_size = tauri::PhysicalSize::new(outer.width, target_height);
    let _ = window.set_size(locked_size);
    let _ = window.set_min_size(Some(locked_size));
    let _ = window.set_max_size(Some(locked_size));
}

fn process_was_started_as_login_item() -> bool {
    std::env::args().any(|argument| argument == LAUNCHED_AT_LOGIN_ARGUMENT)
}

#[cfg(not(target_os = "macos"))]
fn create_hud_window(app: &mut tauri::App) {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let _ = WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("hud.html".into()))
        .title("")
        .inner_size(520.0, 72.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .focusable(false)
        .resizable(false)
        .build();
    if let Some(hud) = app.handle().get_webview_window("hud") {
        let _ = hud.set_ignore_cursor_events(true);
        #[cfg(target_os = "windows")]
        if let Ok(hwnd) = hud.hwnd() {
            rustle_core::win_insert::prevent_window_activation(hwnd.0 as isize);
        }
    }
}

pub(crate) fn reveal_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn conceal_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.hide();
    }
}

fn build_tray_icon(app: &tauri::App) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let open_item = MenuItem::with_id(app, "open", "Open Rustle Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Rustle", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

    let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;

    let tray_builder = TrayIconBuilder::with_id("rustle-tray")
        .icon(tray_icon)
        .tooltip(&format!("Rustle {}", app.package_info().version))
        .menu(&menu)
        .show_menu_on_left_click(false);
    #[cfg(target_os = "macos")]
    let tray_builder = tray_builder.icon_as_template(true);
    let tray = tray_builder
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => reveal_settings_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal_settings_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(tray)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![LAUNCHED_AT_LOGIN_ARGUMENT]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                let _ = app.handle().set_dock_visibility(false);
            }
            #[cfg(target_os = "macos")]
            {
                if let Some(bundle) = mac_setup::running_app_bundle_path() {
                    if rustle_core::install_location::path_looks_like_a_transient_install(
                        &bundle.to_string_lossy(),
                    ) && mac_setup::install_into_applications_and_relaunch(app.handle()).is_ok()
                    {
                        return Ok(());
                    }
                }
            }
            let config = load_config().unwrap_or_default();
            apply_launch_at_login(app.handle(), config.launch_at_login);

            let status_app = app.handle().clone();
            let engine = DictationEngine::start(config, move |status| {
                eprintln!("[rustle] {status:?}");
                let _ = status_app.emit("dictation-status", describe_status(&status));
                let app = status_app.clone();
                let ui_app = app.clone();
                let status = status.clone();
                overlay::run_on_ui_thread(&app, move || {
                    if let Some(feedback) = ui_app.try_state::<FeedbackState>() {
                        feedback.overlay.apply(&ui_app, &feedback.tray, &status);
                    }
                });
            })
            .map_err(|error| error.to_string())?;
            app.manage(Mutex::new(EngineState { engine }));

            #[cfg(not(target_os = "macos"))]
            create_hud_window(app);
            let overlay = DictationOverlay::create();
            let tray = build_tray_icon(app)?;
            app.manage(FeedbackState {
                overlay,
                tray: tray.clone(),
            });
            #[cfg(target_os = "macos")]
            mac_setup::follow_permission_grants_and_relaunch(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    api.prevent_close();
                    conceal_settings_window(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_version,
            get_config,
            save_and_apply_config,
            list_microphones,
            list_models,
            set_dictation_enabled,
            get_dictation_enabled,
            notify_hotkey_edge,
            download_model,
            show_settings_window,
            open_accessibility_settings,
            resize_settings_window,
            list_hotkey_choices,
            host_platform,
            write_utf8_path,
            read_utf8_path,
            macos_setup_status,
            install_into_applications,
            request_dictation_permissions
        ])
        .build(tauri::generate_context!())
        .expect("error while building Rustle")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = &event {
                reveal_settings_window(app);
                return;
            }
            if let RunEvent::Ready = event {
                if !process_was_started_as_login_item() {
                    reveal_settings_window(app);
                }
            }
        });
}
