use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RIDEV_INPUTSINK, RID_INPUT, RIM_TYPEKEYBOARD,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PeekMessageW,
    RegisterClassW, SetTimer, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK,
    HWND_MESSAGE, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, LLKHF_UP, MSG, PM_NOREMOVE,
    RI_KEY_BREAK, RI_KEY_E0, WH_KEYBOARD_LL, WM_INPUT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WM_TIMER, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
};

use crate::config::Config;
use crate::hotkey::HotkeyEdge;

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
    unsafe { pump_windows_hotkey_messages() }
}

unsafe fn pump_windows_hotkey_messages() -> Result<(), String> {
    let mut message = MSG::default();
    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
    let sink = create_raw_input_sink();
    if sink.0.is_null() {
        write_hotkey_log("raw input sink was not created; using the keyboard hook alone");
    } else if let Err(error) = register_keyboard_raw_input(sink) {
        write_hotkey_log(&format!("raw input was not registered: {error}"));
    } else {
        write_hotkey_log("windows raw input listening");
        let _ = SetTimer(sink, 1, 30_000, None);
    }
    let mut hook = install_keyboard_hook()?;
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
        if message.message == WM_TIMER {
            let _ = UnhookWindowsHookEx(hook);
            match install_keyboard_hook() {
                Ok(replaced) => hook = replaced,
                Err(error) => write_hotkey_log(&format!("hotkey hook reinstall failed: {error}")),
            }
        }
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    let _ = UnhookWindowsHookEx(hook);
    Ok(())
}

fn install_keyboard_hook() -> Result<HHOOK, String> {
    unsafe {
        let module = GetModuleHandleW(None).map_err(|error| error.to_string())?;
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            HINSTANCE(module.0),
            0,
        )
        .map_err(|error| error.to_string())
    }
}

fn create_raw_input_sink() -> HWND {
    unsafe {
        let module = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("RustleHotkeySink");
        let class = WNDCLASSW {
            lpfnWndProc: Some(hotkey_sink_window_proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!(""),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            HINSTANCE(module.0),
            None,
        )
        .unwrap_or_default()
    }
}

fn register_keyboard_raw_input(sink: HWND) -> Result<(), String> {
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x06,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: sink,
    };
    unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) }
        .map_err(|error| error.to_string())
}

unsafe extern "system" fn hotkey_sink_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_INPUT {
        deliver_raw_keyboard_input(lparam);
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

fn deliver_raw_keyboard_input(lparam: LPARAM) {
    let mut size = size_of::<RAWINPUT>() as u32;
    let mut raw = RAWINPUT::default();
    let written = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as *mut core::ffi::c_void),
            RID_INPUT,
            Some((&mut raw as *mut RAWINPUT).cast()),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if written == u32::MAX || written == 0 {
        return;
    }
    if raw.header.dwType != RIM_TYPEKEYBOARD.0 {
        return;
    }
    let keyboard = unsafe { raw.data.keyboard };
    let up = keyboard.Flags & RI_KEY_BREAK as u16 != 0;
    let extended = keyboard.Flags & RI_KEY_E0 as u16 != 0;
    let _ = deliver_if_hotkey(
        keyboard.VKey as u32,
        keyboard.MakeCode as u32,
        extended,
        up,
        false,
    );
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let mut swallow = false;
    if code >= 0 && lparam.0 as usize != 0 {
        let info = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        let message = wparam.0 as u32;
        let injected = info.flags.contains(LLKHF_INJECTED);
        let up = info.flags.contains(LLKHF_UP) || message == WM_KEYUP || message == WM_SYSKEYUP;
        let extended = info.flags.contains(LLKHF_EXTENDED);
        if matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP) {
            swallow = deliver_if_hotkey(info.vkCode, info.scanCode, extended, up, injected);
        }
    }
    if swallow {
        LRESULT(1)
    } else {
        CallNextHookEx(None, code, wparam, lparam)
    }
}

fn deliver_if_hotkey(vk: u32, scan: u32, extended: bool, up: bool, injected: bool) -> bool {
    if injected {
        return false;
    }
    let Ok(state) = STATE.try_lock() else {
        return false;
    };
    let Some(state) = state.as_ref() else {
        return false;
    };
    let Ok(config) = state.shared_config.try_lock() else {
        return false;
    };
    let choice = config.hotkey.effective();
    drop(config);
    if !choice.matches_win_key(vk, scan, extended) {
        return false;
    }
    let listening = state.listening_enabled.load(Ordering::SeqCst);
    if !up {
        if !state.pressed.swap(true, Ordering::SeqCst) && listening {
            (state.on_edge)(HotkeyEdge::Press);
        }
        listening
    } else {
        let was_pressed = state.pressed.swap(false, Ordering::SeqCst);
        if was_pressed {
            (state.on_edge)(HotkeyEdge::Release);
        }
        was_pressed || listening
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
