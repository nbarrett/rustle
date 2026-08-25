use anyhow::{anyhow, Result};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::{FALSE, HWND, TRUE};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL,
    VK_RETURN, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

#[derive(Clone)]
struct RememberedWindow {
    hwnd_bits: isize,
    process_id: u32,
    name: String,
}

static REMEMBERED_WINDOW: Mutex<Option<RememberedWindow>> = Mutex::new(None);

pub fn remember_front_window() {
    if let Ok(mut slot) = REMEMBERED_WINDOW.lock() {
        *slot = capture_front_window();
    }
}

pub fn forget_front_window() {
    if let Ok(mut slot) = REMEMBERED_WINDOW.lock() {
        *slot = None;
    }
}

pub fn paste_transcript(text: &str) -> Result<()> {
    let hwnd = remembered_hwnd().unwrap_or_else(|| unsafe { GetForegroundWindow() });
    write_clipboard(text)?;
    focus_window(hwnd);
    thread::sleep(Duration::from_millis(40));
    send_keys(&[
        (VK_CONTROL, false),
        (VK_V, false),
        (VK_V, true),
        (VK_CONTROL, true),
    ])
}

pub fn post_return_key() -> Result<()> {
    send_keys(&[(VK_RETURN, false), (VK_RETURN, true)])
}

pub fn front_app_is_ours() -> bool {
    remembered_process_id()
        .or_else(foreground_pid)
        .is_some_and(|process_id| process_id == std::process::id())
}

pub fn front_app_name() -> Option<String> {
    if let Ok(slot) = REMEMBERED_WINDOW.lock() {
        if let Some(window) = slot.as_ref() {
            return Some(window.name.clone());
        }
    }
    title_of_hwnd(unsafe { GetForegroundWindow() })
}

pub fn prevent_window_activation(hwnd_bits: isize) {
    let hwnd = hwnd_from_bits(hwnd_bits);
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            current
                | WS_EX_NOACTIVATE.0 as isize
                | WS_EX_TOOLWINDOW.0 as isize
                | WS_EX_TOPMOST.0 as isize,
        );
    }
}

pub fn show_without_activating(hwnd_bits: isize) {
    let hwnd = hwnd_from_bits(hwnd_bits);
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

fn capture_front_window() -> Option<RememberedWindow> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    let process_id = process_id_of_hwnd(hwnd)?;
    Some(RememberedWindow {
        hwnd_bits: hwnd.0 as isize,
        process_id,
        name: title_of_hwnd(hwnd).unwrap_or_else(|| "-".to_string()),
    })
}

fn remembered_hwnd() -> Option<HWND> {
    let slot = REMEMBERED_WINDOW.lock().ok()?;
    let window = slot.as_ref()?;
    let hwnd = hwnd_from_bits(window.hwnd_bits);
    if hwnd.0.is_null() {
        None
    } else {
        Some(hwnd)
    }
}

fn remembered_process_id() -> Option<u32> {
    REMEMBERED_WINDOW
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|window| window.process_id))
}

fn hwnd_from_bits(hwnd_bits: isize) -> HWND {
    HWND(hwnd_bits as *mut core::ffi::c_void)
}

fn title_of_hwnd(hwnd: HWND) -> Option<String> {
    if hwnd.0.is_null() {
        return None;
    }
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if length <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

fn foreground_pid() -> Option<u32> {
    process_id_of_hwnd(unsafe { GetForegroundWindow() })
}

fn process_id_of_hwnd(hwnd: HWND) -> Option<u32> {
    if hwnd.0.is_null() {
        return None;
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id == 0 {
        None
    } else {
        Some(process_id)
    }
}

fn focus_window(hwnd: HWND) {
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let current_foreground = GetForegroundWindow();
        let our_thread = GetCurrentThreadId();
        let target_thread = GetWindowThreadProcessId(hwnd, None);
        let foreground_thread = GetWindowThreadProcessId(current_foreground, None);
        let attached_target = target_thread != 0
            && target_thread != our_thread
            && AttachThreadInput(our_thread, target_thread, TRUE).as_bool();
        let attached_foreground = foreground_thread != 0
            && foreground_thread != our_thread
            && foreground_thread != target_thread
            && AttachThreadInput(our_thread, foreground_thread, TRUE).as_bool();
        let _ = SetForegroundWindow(hwnd);
        if attached_target {
            let _ = AttachThreadInput(our_thread, target_thread, FALSE);
        }
        if attached_foreground {
            let _ = AttachThreadInput(our_thread, foreground_thread, FALSE);
        }
    }
}

fn write_clipboard(text: &str) -> Result<()> {
    arboard::Clipboard::new()?
        .set_text(text.to_string())
        .map_err(|error| anyhow!("{error}"))
}

fn send_keys(keys: &[(VIRTUAL_KEY, bool)]) -> Result<()> {
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|(vk, released)| keyboard_input(*vk, *released))
        .collect();
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(anyhow!("SendInput sent {sent} of {} events", inputs.len()))
    }
}

fn keyboard_input(vk: VIRTUAL_KEY, released: bool) -> INPUT {
    let flags = if released {
        KEYEVENTF_KEYUP
    } else {
        Default::default()
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
