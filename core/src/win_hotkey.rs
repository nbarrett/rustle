use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_UP, MSG,
    PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::config::Config;
use crate::hotkey::HotkeyEdge;

const WIN_VK_F1: u32 = 0x70;
const WIN_VK_F12: u32 = 0x7B;

struct WinHotkeyState {
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    on_edge: Box<dyn Fn(HotkeyEdge) + Send>,
    pressed: AtomicBool,
}

static STATE: Mutex<Option<WinHotkeyState>> = Mutex::new(None);

pub fn run_hotkey_listener(
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    on_edge: Box<dyn Fn(HotkeyEdge) + Send>,
) -> Result<(), String> {
    {
        let mut state = STATE.lock().map_err(|error| error.to_string())?;
        *state = Some(WinHotkeyState {
            shared_config,
            listening_enabled,
            on_edge,
            pressed: AtomicBool::new(false),
        });
    }
    write_hotkey_log("windows hotkey hook installing");
    unsafe {
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            HINSTANCE::default(),
            0,
        )
        .map_err(|error| error.to_string())?;
        write_hotkey_log("windows hotkey hook listening");
        loop {
            let status = GetMessageW(&mut message, None, 0, 0);
            if status.0 == 0 {
                break;
            }
            if status.0 == -1 {
                write_hotkey_log("windows GetMessageW failed");
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let _ = UnhookWindowsHookEx(hook);
    }
    Ok(())
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam.0 as usize != 0 {
        let info = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        let message = wparam.0 as u32;
        let up = info.flags.contains(LLKHF_UP) || message == WM_KEYUP || message == WM_SYSKEYUP;
        let extended = info.flags.contains(LLKHF_EXTENDED);
        if matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP) {
            deliver_if_hotkey(info.vkCode, info.scanCode, extended, up);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn deliver_if_hotkey(vk: u32, scan: u32, extended: bool, up: bool) {
    let Ok(state) = STATE.try_lock() else {
        return;
    };
    let Some(state) = state.as_ref() else {
        return;
    };
    let Ok(config) = state.shared_config.try_lock() else {
        return;
    };
    let choice = config.hotkey.effective();
    drop(config);
    if interesting_win_vk(vk) {
        write_hotkey_log(&format!(
            "win vk={vk:#x} scan={scan:#x} extended={extended} up={up} choice={choice:?}"
        ));
    }
    if !choice.matches_win_vk(vk, extended) {
        return;
    }
    if !up {
        if state.pressed.swap(true, Ordering::SeqCst) {
            return;
        }
        if state.listening_enabled.load(Ordering::SeqCst) {
            write_hotkey_log("hotkey press");
            (state.on_edge)(HotkeyEdge::Press);
        } else {
            write_hotkey_log("hotkey press ignored; listening is off");
        }
    } else if state.pressed.swap(false, Ordering::SeqCst) {
        write_hotkey_log("hotkey release");
        (state.on_edge)(HotkeyEdge::Release);
    }
}

fn interesting_win_vk(vk: u32) -> bool {
    use crate::hotkey::{
        WIN_VK_CONTROL, WIN_VK_LCONTROL, WIN_VK_LMENU, WIN_VK_MEDIA_NEXT_TRACK,
        WIN_VK_MEDIA_PLAY_PAUSE, WIN_VK_MENU, WIN_VK_RCONTROL, WIN_VK_RMENU,
    };
    matches!(
        vk,
        WIN_VK_CONTROL
            | WIN_VK_MENU
            | WIN_VK_LCONTROL
            | WIN_VK_RCONTROL
            | WIN_VK_LMENU
            | WIN_VK_RMENU
            | WIN_VK_MEDIA_NEXT_TRACK
            | WIN_VK_MEDIA_PLAY_PAUSE
    ) || (WIN_VK_F1..=WIN_VK_F12).contains(&vk)
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
