use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rdev::{listen, Event, EventType, Key};

use crate::config::Config;
use crate::hotkey::{HotkeyChoice, HotkeyEdge};

pub fn run_hotkey_listener(
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    on_edge: Box<dyn Fn(HotkeyEdge) + Send>,
) -> Result<(), String> {
    let pressed = AtomicBool::new(false);
    listen(move |event: Event| {
        let choice = shared_config.lock().unwrap().hotkey.effective();
        let matched = match event.event_type {
            EventType::KeyPress(key) if choice.matches_rdev_key(key) => Some(true),
            EventType::KeyRelease(key) if choice.matches_rdev_key(key) => Some(false),
            _ => None,
        };
        let Some(is_press) = matched else {
            return;
        };
        if is_press {
            if pressed.swap(true, Ordering::SeqCst) {
                return;
            }
            if listening_enabled.load(Ordering::SeqCst) {
                write_hotkey_log("hotkey press");
                on_edge(HotkeyEdge::Press);
            } else {
                write_hotkey_log("hotkey press ignored; listening is off");
            }
        } else if pressed.swap(false, Ordering::SeqCst) {
            write_hotkey_log("hotkey release");
            on_edge(HotkeyEdge::Release);
        }
    })
    .map_err(|error| format!("{error:?}"))
}

impl HotkeyChoice {
    fn matches_rdev_key(self, key: Key) -> bool {
        match (self.effective(), key) {
            (HotkeyChoice::RightOption, Key::AltGr | Key::Alt) => true,
            (HotkeyChoice::RightControl, Key::ControlRight) => true,
            (HotkeyChoice::F8, Key::F8) => true,
            (HotkeyChoice::F9, Key::F9) => true,
            _ => false,
        }
    }
}

fn write_hotkey_log(message: &str) {
    let Ok(directory) = crate::config::rustle_directory() else {
        return;
    };
    let path = directory.join("engine.log");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{stamp} {message}")
        });
}
