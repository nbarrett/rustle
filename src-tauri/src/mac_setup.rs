use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use rustle_core::install_location::{
    path_is_a_stable_app_install, path_looks_like_a_transient_install,
};
use rustle_core::mac_ax::{process_is_trusted, request_accessibility_prompt};
use rustle_core::mac_hotkey::{
    listen_event_access_is_granted, request_listen_event_access, request_post_event_access,
};
use rustle_core::mac_mic::{
    microphone_access_is_granted, microphone_access_still_needs_a_prompt,
    microphone_access_was_refused, prompt_for_microphone_access,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::reveal_settings_window;

#[derive(Clone, Serialize)]
pub struct MacosSetupStatus {
    pub in_applications: bool,
    pub listen: bool,
    pub accessibility: bool,
    pub microphone: bool,
}

pub fn running_app_bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|path| path.extension().is_some_and(|ext| ext == "app"))
        .map(|path| path.to_path_buf())
}

pub fn current_setup_status() -> MacosSetupStatus {
    let bundle = running_app_bundle_path();
    let path = bundle
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    MacosSetupStatus {
        in_applications: path_is_a_stable_app_install(&path)
            && !path_looks_like_a_transient_install(&path),
        listen: listen_event_access_is_granted(),
        accessibility: process_is_trusted(),
        microphone: microphone_access_is_granted(),
    }
}

fn open_privacy_pane(modern: &str, legacy: &str) {
    let _ = Command::new("open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?{modern}"
        ))
        .status();
    let _ = Command::new("open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{legacy}"
        ))
        .status();
}

pub fn open_microphone_settings() {
    open_privacy_pane("Privacy_Microphone", "Privacy_Microphone");
}

pub fn open_listen_event_settings() {
    open_privacy_pane("Privacy_ListenEvent", "Privacy_ListenEvent");
}

pub fn open_accessibility_settings() {
    open_privacy_pane("Privacy_Accessibility", "Privacy_Accessibility");
}

pub fn open_missing_permission_settings() {
    let status = current_setup_status();
    if microphone_access_was_refused() {
        open_microphone_settings();
        return;
    }
    if !status.listen {
        open_listen_event_settings();
        return;
    }
    if !status.accessibility {
        open_accessibility_settings();
        return;
    }
    if !status.microphone {
        open_microphone_settings();
    }
}

pub fn open_dictation_permission_settings() {
    open_missing_permission_settings();
}

pub fn open_permission_pane(pane: &str) {
    match pane {
        "microphone" => open_microphone_settings(),
        "listen" => open_listen_event_settings(),
        "accessibility" => open_accessibility_settings(),
        _ => open_missing_permission_settings(),
    }
}

pub fn request_dictation_permissions() {
    let _ = request_listen_event_access();
    let _ = request_post_event_access();
    let _ = request_accessibility_prompt();
    if microphone_access_still_needs_a_prompt() || !microphone_access_is_granted() {
        prompt_for_microphone_access();
    }
    let _ = rustle_core::audio::list_input_device_names();
}

fn applications_destination() -> PathBuf {
    let system = PathBuf::from("/Applications/Rustle.app");
    if parent_allows_copy(&system) {
        return system;
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Applications"))
        .join("Applications/Rustle.app")
}

fn parent_allows_copy(destination: &Path) -> bool {
    destination
        .parent()
        .map(|parent| parent.is_dir() && !parent.metadata().map(|meta| meta.permissions().readonly()).unwrap_or(true))
        .unwrap_or(false)
}

pub fn copy_running_bundle_into_applications() -> Result<PathBuf, String> {
    let source = running_app_bundle_path().ok_or_else(|| {
        "Rustle is not running from an app bundle, so it cannot install itself.".to_string()
    })?;
    let destination = applications_destination();
    if source == destination {
        return Ok(destination);
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
    }
    let copied = Command::new("ditto")
        .arg(&source)
        .arg(&destination)
        .status()
        .map_err(|error| error.to_string())?;
    if !copied.success() {
        return Err("could not copy Rustle into Applications".to_string());
    }
    let _ = Command::new("xattr")
        .args(["-cr"])
        .arg(&destination)
        .status();
    Ok(destination)
}

pub fn relaunch_bundle_then_exit(app: &AppHandle, bundle: &Path) {
    let path = bundle.to_path_buf();
    let _ = Command::new("/bin/sh")
        .args([
            "-c",
            &format!(
                "sleep 0.5; open \"{}\"",
                path.to_string_lossy().replace('"', "")
            ),
        ])
        .spawn();
    app.exit(0);
}

pub fn install_into_applications_and_relaunch(app: &AppHandle) -> Result<(), String> {
    let destination = copy_running_bundle_into_applications()?;
    relaunch_bundle_then_exit(app, &destination);
    Ok(())
}

pub fn follow_permission_grants_and_relaunch(app: AppHandle) {
    let prompt_app = app.clone();
    let _ = prompt_app.run_on_main_thread(|| {
        request_dictation_permissions();
    });
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        let initial = current_setup_status();
        if !initial.listen || !initial.accessibility || microphone_access_was_refused() {
            open_missing_permission_settings();
            reveal_settings_window(&app);
        }
        if initial.listen && initial.accessibility {
            return;
        }
        loop {
            thread::sleep(Duration::from_secs(1));
            let status = current_setup_status();
            let _ = app.emit("macos-setup", &status);
            if status.listen && status.accessibility {
                if let Some(bundle) = running_app_bundle_path() {
                    relaunch_bundle_then_exit(&app, &bundle);
                }
                return;
            }
        }
    });
}
