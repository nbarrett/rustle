#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

use rustle_core::audio::list_input_device_names;
use rustle_core::config::{load_config, model_catalog, save_config, Config, ModelChoice};
use rustle_core::download::download_model_file;
use rustle_core::engine::{DictationEngine, DictationStatus};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::{AutoLaunchManager, MacosLauncher};

struct EngineState {
    engine: DictationEngine,
}

fn describe_status(status: &DictationStatus) -> serde_json::Value {
    match status {
        DictationStatus::Idle => serde_json::json!({ "kind": "idle" }),
        DictationStatus::Listening => serde_json::json!({ "kind": "listening" }),
        DictationStatus::Transcribing => serde_json::json!({ "kind": "transcribing" }),
        DictationStatus::Partial(text) => serde_json::json!({ "kind": "partial", "text": text }),
        DictationStatus::Typed(text) => serde_json::json!({ "kind": "typed", "text": text }),
        DictationStatus::Failed(message) => {
            serde_json::json!({ "kind": "failed", "text": message })
        }
    }
}

#[tauri::command]
fn get_config(state: State<'_, Mutex<EngineState>>) -> Config {
    state.lock().unwrap().engine.current_config()
}

#[tauri::command]
fn save_and_apply_config(
    app: AppHandle,
    new_config: Config,
    state: State<'_, Mutex<EngineState>>,
) -> Result<(), String> {
    save_config(&new_config).map_err(|error| error.to_string())?;
    apply_launch_at_login(&app, new_config.launch_at_login);
    let engine_state = state
        .lock()
        .map_err(|_| "engine state was unavailable".to_string())?;
    engine_state.engine.apply_config(new_config);
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
fn set_dictation_enabled(enabled: bool, state: State<'_, Mutex<EngineState>>) {
    state.lock().unwrap().engine.set_listening_enabled(enabled);
}

#[tauri::command]
fn get_dictation_enabled(state: State<'_, Mutex<EngineState>>) -> bool {
    state.lock().unwrap().engine.is_listening_enabled()
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
    let chrome_height = outer.height as f64 - inner.height as f64;
    let target_height = ((content_height * scale + chrome_height).round() as u32).max(200);
    let _ = window.set_size(tauri::PhysicalSize::new(outer.width, target_height));
}

#[cfg(target_os = "macos")]
fn request_accessibility_trust() {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        static kAXTrustedCheckOptionPrompt: CFStringRef;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    }

    unsafe {
        let prompt_key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let prompt_value = CFBoolean::true_value();
        let options =
            CFDictionary::from_CFType_pairs(&[(prompt_key.as_CFType(), prompt_value.as_CFType())]);
        let _ = AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
    }
}

fn reveal_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn build_tray_icon(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let open_item = MenuItem::with_id(app, "open", "Open Rustle Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Rustle", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

    let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;

    TrayIconBuilder::with_id("rustle-tray")
        .icon(tray_icon)
        .icon_as_template(true)
        .tooltip("Rustle")
        .menu(&menu)
        .show_menu_on_left_click(false)
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
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                request_accessibility_trust();
            }

            let config = load_config().unwrap_or_default();
            let status_app = app.handle().clone();
            let engine = DictationEngine::start(config, move |status| {
                let _ = status_app.emit("dictation-status", describe_status(&status));
            })
            .map_err(|error| error.to_string())?;

            app.manage(Mutex::new(EngineState { engine }));
            build_tray_icon(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_and_apply_config,
            list_microphones,
            list_models,
            set_dictation_enabled,
            get_dictation_enabled,
            download_model,
            show_settings_window,
            resize_settings_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rustle");
}
