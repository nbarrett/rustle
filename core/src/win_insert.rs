use anyhow::{anyhow, Result};
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL,
    VK_RETURN, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, SetForegroundWindow,
};

pub fn paste_transcript(text: &str) -> Result<()> {
    let hwnd = unsafe { GetForegroundWindow() };
    write_clipboard(text)?;
    thread::sleep(Duration::from_millis(40));
    focus_window(hwnd);
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
    foreground_pid() == Some(std::process::id())
}

pub fn front_app_name() -> Option<String> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0 == 0 {
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
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0 == 0 {
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
    if hwnd.0 != 0 {
        let _ = unsafe { SetForegroundWindow(hwnd) };
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
