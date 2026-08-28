#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::panic::{catch_unwind, AssertUnwindSafe};
#[cfg(target_os = "ios")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rustle_core::audio::list_input_device_names;
use rustle_core::config::{load_config, model_catalog, save_config, Config, ModelChoice};
use rustle_core::download::download_model_file;
use rustle_core::engine::{DictationEngine, DictationStatus};
use rustle_core::hotkey::{HotkeyChoice, HotkeyOption};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri::menu::{Menu, MenuItem};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri_plugin_autostart::{AutoLaunchManager, MacosLauncher};

const LAUNCHED_AT_LOGIN_ARGUMENT: &str = "--launched-at-login";

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod overlay;
#[cfg(target_os = "macos")]
mod mac_setup;
#[cfg(target_os = "ios")]
mod phone_keyboard;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use overlay::DictationOverlay;

struct EngineState {
    engine: DictationEngine,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct FeedbackState {
    overlay: DictationOverlay,
    tray: TrayIcon,
}

#[cfg(target_os = "ios")]
struct PhoneDictateLaunch {
    start_recording: AtomicBool,
}

#[cfg(target_os = "ios")]
static KEYBOARD_APP: Mutex<Option<AppHandle>> = Mutex::new(None);

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
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = (app, enabled);
        return;
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let Some(manager) = app.try_state::<AutoLaunchManager>() else {
        return;
    };
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
                "accessibility": false,
                "microphone": false
            })
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        serde_json::json!({
            "in_applications": true,
            "listen": true,
            "accessibility": true,
            "microphone": true
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
    open_permission_settings("accessibility".to_string());
}

#[tauri::command]
fn open_permission_settings(pane: String) {
    #[cfg(target_os = "macos")]
    {
        mac_setup::open_permission_pane(&pane);
    }
    #[cfg(target_os = "windows")]
    {
        let _ = pane;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:privacy-microphone"])
            .status();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = pane;
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

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn keep_settings_window_above_full_screen_apps(window: &tauri::WebviewWindow) {
    let _ = window.set_always_on_top(true);
    let _ = window.set_visible_on_all_workspaces(true);
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{
            NSStatusWindowLevel, NSWindowCollectionBehavior, NSWindowStyleMask,
        };
        let Ok(raw) = window.ns_window() else {
            return;
        };
        if objc2::MainThreadMarker::new().is_none() {
            return;
        }
        let ns_window: &objc2_app_kit::NSWindow = unsafe { &*raw.cast() };
        ns_window.setStyleMask(
            ns_window
                .styleMask()
                .union(NSWindowStyleMask::NonactivatingPanel),
        );
        ns_window.setHidesOnDeactivate(false);
        ns_window.setLevel(NSStatusWindowLevel);
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                .union(NSWindowCollectionBehavior::FullScreenAuxiliary)
                .union(NSWindowCollectionBehavior::IgnoresCycle),
        );
        ns_window.orderFrontRegardless();
    }
}

pub(crate) fn reveal_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        keep_settings_window_above_full_screen_apps(&window);
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let _ = window.unminimize();
        let _ = window.show();
        #[cfg(all(
            not(target_os = "macos"),
            not(target_os = "ios"),
            not(target_os = "android")
        ))]
        {
            let _ = window.set_focus();
        }
        #[cfg(target_os = "macos")]
        {
            keep_settings_window_above_full_screen_apps(&window);
        }
    }
}

fn conceal_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.hide();
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        builder = builder
            .plugin(tauri_plugin_autostart::init(
                MacosLauncher::LaunchAgent,
                Some(vec![LAUNCHED_AT_LOGIN_ARGUMENT]),
            ))
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }
    builder
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

            #[cfg(target_os = "ios")]
            app.manage(PhoneDictateLaunch {
                start_recording: AtomicBool::new(false),
            });
            let status_app = app.handle().clone();
            let engine = DictationEngine::start(config, move |status| {
                eprintln!("[rustle] {status:?}");
                #[cfg(target_os = "ios")]
                apply_phone_keyboard_status(&status_app, &status);
                let _ = status_app.emit("dictation-status", describe_status(&status));
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                {
                    let app = status_app.clone();
                    let ui_app = app.clone();
                    let status = status.clone();
                    overlay::run_on_ui_thread(&app, move || {
                        if let Some(feedback) = ui_app.try_state::<FeedbackState>() {
                            feedback.overlay.apply(&ui_app, &feedback.tray, &status);
                        }
                    });
                }
            })
            .map_err(|error| error.to_string())?;
            app.manage(Mutex::new(EngineState { engine }));
            #[cfg(target_os = "ios")]
            {
                *KEYBOARD_APP.lock().unwrap() = Some(app.handle().clone());
                phone_keyboard::listen_for_stop(on_ios_keyboard_stop);
                drain_phone_dictate_launch(app.handle());
            }

            #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
            create_hud_window(app);
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            {
                let overlay = DictationOverlay::create();
                let tray = build_tray_icon(app)?;
                app.manage(FeedbackState {
                    overlay,
                    tray: tray.clone(),
                });
            }
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            if let Some(window) = app.get_webview_window("settings") {
                keep_settings_window_above_full_screen_apps(&window);
            }
            #[cfg(target_os = "android")]
            if let Some(window) = app.get_webview_window("settings") {
                let _ = window.show();
            }
            #[cfg(target_os = "macos")]
            mac_setup::follow_permission_grants_and_relaunch(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    api.prevent_close();
                    conceal_settings_window(window.app_handle());
                }
            }
            #[cfg(any(target_os = "ios", target_os = "android"))]
            let _ = (window, event);
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
            open_permission_settings,
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
            #[cfg(target_os = "ios")]
            if let RunEvent::Opened { urls } = &event {
                if urls
                    .iter()
                    .any(|url| phone_keyboard::url_asks_to_dictate(url.as_str()))
                {
                    start_phone_dictate_from_keyboard(app);
                }
            }
            if let RunEvent::Ready = event {
                #[cfg(target_os = "ios")]
                if phone_keyboard::keyboard_session() {
                    return;
                }
                if !process_was_started_as_login_item() {
                    reveal_settings_window(app);
                }
            }
        });
}

#[cfg(target_os = "ios")]
extern "C" fn on_ios_keyboard_stop() {
    let app = KEYBOARD_APP
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().cloned());
    let Some(app) = app else {
        return;
    };
    phone_keyboard::set_phase("transcribing");
    phone_keyboard::begin_transcribe_background_task();
    if let Some(state) = app.try_state::<Mutex<EngineState>>() {
        if let Ok(guard) = state.lock() {
            guard.engine.notify_hotkey_edge(false);
        }
    }
}

#[cfg(target_os = "ios")]
fn apply_phone_keyboard_status(app: &AppHandle, status: &DictationStatus) {
    match status {
        DictationStatus::Listening => {
            phone_keyboard::set_phase("listening");
            if phone_keyboard::keyboard_session() {
                if let Some(window) = app.get_webview_window("settings") {
                    let _ = window.hide();
                }
                phone_keyboard::return_to_host_app();
            }
        }
        DictationStatus::Transcribing => {
            phone_keyboard::set_phase("transcribing");
            phone_keyboard::begin_transcribe_background_task();
        }
        DictationStatus::Typed(text) => {
            phone_keyboard::publish_transcript(text);
            phone_keyboard::end_keyboard_session();
        }
        DictationStatus::Failed(_) | DictationStatus::NeedsPermission(_) => {
            phone_keyboard::end_keyboard_session();
        }
        _ => {}
    }
}

#[cfg(target_os = "ios")]
fn start_phone_dictate_from_keyboard(app: &AppHandle) {
    phone_keyboard::mark_keyboard_session();
    phone_keyboard::prepare_audio_session();
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.hide();
    }
    if let Some(state) = app.try_state::<Mutex<EngineState>>() {
        if let Ok(guard) = state.lock() {
            guard.engine.notify_hotkey_edge(true);
            let _ = app.emit("phone-dictate", ());
            return;
        }
    }
    if let Some(launch) = app.try_state::<PhoneDictateLaunch>() {
        launch.start_recording.store(true, Ordering::SeqCst);
    }
}

#[cfg(target_os = "ios")]
fn drain_phone_dictate_launch(app: &AppHandle) {
    let pending = app
        .try_state::<PhoneDictateLaunch>()
        .is_some_and(|launch| launch.start_recording.swap(false, Ordering::SeqCst));
    if pending {
        start_phone_dictate_from_keyboard(app);
    }
}
