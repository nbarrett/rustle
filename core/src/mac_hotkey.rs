use std::cell::Cell;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::config::Config;

#[derive(Clone, Copy, Debug)]
pub enum HotkeyEdge {
    Press,
    Release,
}

type CFTypeRef = *mut c_void;
type EventTapCallback =
    extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void;

const EVENT_KEY_DOWN: u32 = 10;
const EVENT_KEY_UP: u32 = 11;
const EVENT_FLAGS_CHANGED: u32 = 12;
const EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

const HID_EVENT_TAP: u32 = 0;
const HEAD_INSERT_EVENT_TAP: u32 = 0;
const EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

const FIELD_KEYCODE: u32 = 9;
const FIELD_AUTOREPEAT: u32 = 8;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: EventTapCallback,
        user_info: *mut c_void,
    ) -> CFTypeRef;
    fn CGEventTapEnable(tap: CFTypeRef, enable: bool);
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGEventGetFlags(event: *mut c_void) -> u64;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGPreflightPostEventAccess() -> bool;
    fn CGRequestPostEventAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: CFTypeRef;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: CFTypeRef,
        order: isize,
    ) -> CFTypeRef;
    fn CFRunLoopGetCurrent() -> CFTypeRef;
    fn CFRunLoopAddSource(run_loop: CFTypeRef, source: CFTypeRef, mode: CFTypeRef);
    fn CFRunLoopRun();
}

struct TapContext {
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    on_edge: Box<dyn Fn(HotkeyEdge) + Send>,
    tap_port: Cell<CFTypeRef>,
}

fn deliver_edge(context: &TapContext, pressed: bool) {
    if pressed {
        if context.listening_enabled.load(Ordering::SeqCst) {
            write_hotkey_log("hotkey press");
            (context.on_edge)(HotkeyEdge::Press);
        } else {
            write_hotkey_log("hotkey press ignored; listening is off");
        }
    } else {
        write_hotkey_log("hotkey release");
        (context.on_edge)(HotkeyEdge::Release);
    }
}

extern "C" fn tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    user_info: *mut c_void,
) -> *mut c_void {
    let context = unsafe { &*(user_info as *const TapContext) };

    if event_type == EVENT_TAP_DISABLED_BY_TIMEOUT
        || event_type == EVENT_TAP_DISABLED_BY_USER_INPUT
    {
        unsafe { CGEventTapEnable(context.tap_port.get(), true) };
        return event;
    }

    let choice = context.shared_config.lock().unwrap().hotkey;
    let keycode = unsafe { CGEventGetIntegerValueField(event, FIELD_KEYCODE) };
    if !choice.matches_macos_keycode(keycode) {
        return event;
    }

    if choice.is_modifier() {
        if event_type == EVENT_FLAGS_CHANGED {
            let flags = unsafe { CGEventGetFlags(event) };
            deliver_edge(context, (flags & choice.macos_modifier_flag()) != 0);
        }
    } else if event_type == EVENT_KEY_DOWN {
        let is_repeat = unsafe { CGEventGetIntegerValueField(event, FIELD_AUTOREPEAT) };
        if is_repeat == 0 {
            deliver_edge(context, true);
        }
    } else if event_type == EVENT_KEY_UP {
        deliver_edge(context, false);
    }

    event
}

pub fn listen_event_access_is_granted() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

pub fn request_listen_event_access() -> bool {
    unsafe { CGRequestListenEventAccess() }
}

pub fn post_event_access_is_granted() -> bool {
    unsafe { CGPreflightPostEventAccess() }
}

pub fn request_post_event_access() -> bool {
    unsafe { CGRequestPostEventAccess() }
}

pub fn run_hotkey_tap(
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    on_edge: Box<dyn Fn(HotkeyEdge) + Send>,
) -> bool {
    let context = Box::new(TapContext {
        shared_config,
        listening_enabled,
        on_edge,
        tap_port: Cell::new(ptr::null_mut()),
    });
    let context_pointer = Box::into_raw(context);

    let events_of_interest =
        (1u64 << EVENT_KEY_DOWN) | (1u64 << EVENT_KEY_UP) | (1u64 << EVENT_FLAGS_CHANGED);

    unsafe {
        write_hotkey_log(&format!(
            "Input Monitoring preflight={}",
            CGPreflightListenEventAccess()
        ));

        let tap_port = CGEventTapCreate(
            HID_EVENT_TAP,
            HEAD_INSERT_EVENT_TAP,
            EVENT_TAP_OPTION_LISTEN_ONLY,
            events_of_interest,
            tap_callback,
            context_pointer as *mut c_void,
        );

        if tap_port.is_null() {
            drop(Box::from_raw(context_pointer));
            write_hotkey_log("hotkey tap was not created");
            return false;
        }

        (*context_pointer).tap_port.set(tap_port);
        write_hotkey_log("hotkey tap listening");

        let source = CFMachPortCreateRunLoopSource(ptr::null(), tap_port, 0);
        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap_port, true);
        CFRunLoopRun();
    }
    true
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
