use anyhow::{anyhow, Result};
use dispatch2::DispatchQueue;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{
    NSAppleScript, NSAppleScriptErrorBriefMessage, NSAppleScriptErrorMessage, NSDictionary,
    NSString,
};
use std::ffi::c_void;
use std::sync::mpsc;

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;
type CFArrayRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFStringRef = *const c_void;
type CFNumberRef = *const c_void;
type CFTypeRef = *const c_void;

const HID_EVENT_TAP: u32 = 0;
const EVENT_SOURCE_STATE_HID_SYSTEM: i32 = 1;
const EVENT_SOURCE_STATE_COMBINED_SESSION: i32 = 0;
const KEYCODE_COMMAND: u16 = 0x37;
const KEYCODE_ANSI_V: u16 = 0x09;
const KEYCODE_DELETE: u16 = 0x33;
const KEYCODE_RETURN: u16 = 0x24;
const EVENT_FLAG_COMMAND: u64 = 0x0010_0000;
const WINDOW_LIST_ON_SCREEN_AND_EXCLUDE_DESKTOP: u32 = 1 | 16;
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const CF_NUMBER_SINT32_TYPE: i32 = 3;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceCreate(state_id: i32) -> CGEventSourceRef;
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CGEventPostToPid(pid: i32, event: CGEventRef);
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFArrayGetCount(array: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, index: isize) -> *const c_void;
    fn CFDictionaryGetValue(dictionary: CFDictionaryRef, key: *const c_void) -> *const c_void;
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        num_bytes: isize,
        encoding: u32,
        is_external_representation: u8,
    ) -> CFStringRef;
    fn CFStringGetLength(the_string: CFStringRef) -> isize;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut i8,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
    fn CFNumberGetValue(number: CFNumberRef, the_type: i32, value_ptr: *mut c_void) -> u8;
}

#[derive(Clone, Debug)]
pub struct FrontApp {
    pub name: String,
    pub bundle: Option<String>,
    pub pid: i32,
}

pub fn name_looks_like_iterm(name: &str) -> bool {
    name.to_ascii_lowercase().contains("iterm")
}

pub fn bundle_looks_like_iterm(bundle: &str) -> bool {
    bundle.eq_ignore_ascii_case("com.googlecode.iterm2")
}

impl FrontApp {
    pub fn is_iterm(&self) -> bool {
        name_looks_like_iterm(&self.name)
            || self
                .bundle
                .as_deref()
                .is_some_and(bundle_looks_like_iterm)
    }

    pub fn is_ours(&self) -> bool {
        self.pid == std::process::id() as i32
            || self.name.eq_ignore_ascii_case("rustle")
            || self
                .bundle
                .as_deref()
                .is_some_and(|bundle| bundle.eq_ignore_ascii_case("com.annix.rustle"))
    }
}

pub fn frontmost_app() -> Option<FrontApp> {
    if let Some(app) = workspace_front_app() {
        if !name_is_chrome_ui(&app.name) {
            return Some(app);
        }
    }
    window_list_front_app()
}

pub fn insert_target_app() -> Option<FrontApp> {
    if let Some(app) = workspace_front_app() {
        if !app.is_ours() && !name_is_chrome_ui(&app.name) {
            return Some(app);
        }
    }
    window_list_front_app()
}

fn workspace_front_app() -> Option<FrontApp> {
    let workspace = NSWorkspace::sharedWorkspace();
    let running = workspace.frontmostApplication()?;
    let name = running
        .localizedName()
        .map(|name| name.to_string())
        .unwrap_or_default();
    let bundle = running.bundleIdentifier().map(|bundle| bundle.to_string());
    let pid = running.processIdentifier();
    if name.is_empty() && bundle.is_none() {
        return None;
    }
    Some(FrontApp { name, bundle, pid })
}

fn name_is_chrome_ui(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "window server"
            | "dock"
            | "control center"
            | "notification center"
            | "notification centre"
            | "spotlight"
            | "systemuiserver"
            | "wallpaper"
            | "loginwindow"
    )
}

fn window_list_front_app() -> Option<FrontApp> {
    unsafe {
        let windows = CGWindowListCopyWindowInfo(WINDOW_LIST_ON_SCREEN_AND_EXCLUDE_DESKTOP, 0);
        if windows.is_null() {
            return None;
        }
        let self_pid = std::process::id() as i32;
        let count = CFArrayGetCount(windows);
        let layer_key = cf_string("kCGWindowLayer");
        let name_key = cf_string("kCGWindowOwnerName");
        let pid_key = cf_string("kCGWindowOwnerPID");
        let mut found = None;
        for index in 0..count {
            let dictionary = CFArrayGetValueAtIndex(windows, index) as CFDictionaryRef;
            if dictionary.is_null() {
                continue;
            }
            let Some(layer) = dictionary_i32(dictionary, layer_key) else {
                continue;
            };
            if layer != 0 {
                continue;
            }
            let Some(pid) = dictionary_i32(dictionary, pid_key) else {
                continue;
            };
            if pid == self_pid || pid <= 0 {
                continue;
            }
            let Some(name) = dictionary_string(dictionary, name_key) else {
                continue;
            };
            if name.is_empty() || name_is_chrome_ui(&name) {
                continue;
            }
            found = Some(FrontApp {
                name,
                bundle: None,
                pid,
            });
            break;
        }
        CFRelease(layer_key as CFTypeRef);
        CFRelease(name_key as CFTypeRef);
        CFRelease(pid_key as CFTypeRef);
        CFRelease(windows);
        found
    }
}

pub fn apply_system_events_delta(
    backspace_count: usize,
    text: &str,
    press_return: bool,
) -> Result<()> {
    if backspace_count == 0 && text.is_empty() && !press_return {
        return Ok(());
    }
    let text_literal = applescript_literal(text);
    let script = format!(
        r#"tell application "System Events"
  repeat {backspace_count} times
    key code 51
  end repeat
  if {text_literal} is not "" then
    keystroke {text_literal}
  end if
  if "{return_flag}" is "yes" then
    key code 36
  end if
end tell"#,
        return_flag = if press_return { "yes" } else { "no" },
    );
    run_applescript(&script)
}

pub fn post_system_events_command_v() -> Result<()> {
    run_applescript(r#"tell application "System Events" to keystroke "v" using command down"#)
}

pub fn apply_iterm_session_delta(
    backspace_count: usize,
    text: &str,
    press_return: bool,
) -> Result<()> {
    if backspace_count == 0 && text.is_empty() && !press_return {
        return Ok(());
    }
    let deletes = "\u{7f}".repeat(backspace_count);
    let deletes_literal = applescript_literal(&deletes);
    let text_literal = applescript_literal(text);
    let return_flag = if press_return { "yes" } else { "no" };
    let script = format!(
        r#"tell application "iTerm"
  tell current session of current window
    if (count of {deletes_literal}) > 0 then
      write text {deletes_literal} newline no
    end if
    if (count of {text_literal}) > 0 then
      write text {text_literal} newline no
    end if
    if "{return_flag}" is "yes" then
      write text "" newline yes
    end if
  end tell
end tell"#
    );
    match run_applescript(&script) {
        Ok(()) => Ok(()),
        Err(first) => {
            let fallback = script.replace(
                r#"tell application "iTerm""#,
                r#"tell application "iTerm2""#,
            );
            run_applescript(&fallback).map_err(|second| {
                anyhow!("{first}; iTerm2 name also failed: {second}")
            })
        }
    }
}

pub fn post_command_v_keystroke() -> Result<()> {
    let target_pid = frontmost_app().map(|app| app.pid);
    unsafe {
        let source = create_event_source()?;
        post_key(source, KEYCODE_COMMAND, true, EVENT_FLAG_COMMAND, target_pid);
        post_key(source, KEYCODE_ANSI_V, true, EVENT_FLAG_COMMAND, target_pid);
        post_key(source, KEYCODE_ANSI_V, false, EVENT_FLAG_COMMAND, target_pid);
        post_key(source, KEYCODE_COMMAND, false, 0, target_pid);
        CFRelease(source as *const c_void);
    }
    Ok(())
}

pub fn post_delete_keystrokes(count: usize) -> Result<()> {
    if count == 0 {
        return Ok(());
    }
    let target_pid = frontmost_app().map(|app| app.pid);
    unsafe {
        let source = create_event_source()?;
        for _ in 0..count {
            post_key(source, KEYCODE_DELETE, true, 0, target_pid);
            post_key(source, KEYCODE_DELETE, false, 0, target_pid);
        }
        CFRelease(source as *const c_void);
    }
    Ok(())
}

pub fn post_return_keystroke() -> Result<()> {
    let target_pid = frontmost_app().map(|app| app.pid);
    unsafe {
        let source = create_event_source()?;
        post_key(source, KEYCODE_RETURN, true, 0, target_pid);
        post_key(source, KEYCODE_RETURN, false, 0, target_pid);
        CFRelease(source as *const c_void);
    }
    Ok(())
}

fn create_event_source() -> Result<CGEventSourceRef> {
    unsafe {
        let source = CGEventSourceCreate(EVENT_SOURCE_STATE_HID_SYSTEM);
        if !source.is_null() {
            return Ok(source);
        }
        let source = CGEventSourceCreate(EVENT_SOURCE_STATE_COMBINED_SESSION);
        if source.is_null() {
            return Err(anyhow!("failed to create event source"));
        }
        Ok(source)
    }
}

unsafe fn post_key(
    source: CGEventSourceRef,
    keycode: u16,
    key_down: bool,
    flags: u64,
    target_pid: Option<i32>,
) {
    let event = CGEventCreateKeyboardEvent(source, keycode, key_down);
    if event.is_null() {
        return;
    }
    CGEventSetFlags(event, flags);
    if let Some(pid) = target_pid {
        CGEventPostToPid(pid, event);
    } else {
        CGEventPost(HID_EVENT_TAP, event);
    }
    CFRelease(event as *const c_void);
    std::thread::sleep(std::time::Duration::from_millis(4));
}

fn run_applescript(source: &str) -> Result<()> {
    let source = source.to_string();
    run_on_main(move || {
        let ns_source = NSString::from_str(&source);
        let Some(script) = NSAppleScript::initWithSource(NSAppleScript::alloc(), &ns_source) else {
            return Err(anyhow!("could not build AppleScript"));
        };
        let mut error: Option<objc2::rc::Retained<NSDictionary<NSString, AnyObject>>> = None;
        unsafe {
            let _ = script.executeAndReturnError(Some(&mut error));
        }
        if let Some(error) = error {
            return Err(anyhow!("{}", applescript_error_message(&error)));
        }
        Ok(())
    })
}

fn applescript_error_message(error: &NSDictionary<NSString, AnyObject>) -> String {
    unsafe {
        for key in [NSAppleScriptErrorMessage, NSAppleScriptErrorBriefMessage] {
            if let Some(value) = error.objectForKey(key) {
                if let Some(text) = value.downcast_ref::<NSString>() {
                    let message = text.to_string();
                    if !message.is_empty() {
                        return message;
                    }
                }
            }
        }
    }
    "AppleScript failed".to_string()
}

fn applescript_literal(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn run_on_main<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    if MainThreadMarker::new().is_some() {
        return work();
    }
    let (sender, receiver) = mpsc::channel();
    DispatchQueue::main().exec_async(move || {
        let _ = sender.send(work());
    });
    receiver
        .recv()
        .expect("main queue dropped AppleScript work")
}

fn cf_string(text: &str) -> CFStringRef {
    unsafe {
        CFStringCreateWithBytes(
            std::ptr::null(),
            text.as_ptr(),
            text.len() as isize,
            CF_STRING_ENCODING_UTF8,
            0,
        )
    }
}

fn dictionary_i32(dictionary: CFDictionaryRef, key: CFStringRef) -> Option<i32> {
    unsafe {
        let value = CFDictionaryGetValue(dictionary, key as *const c_void);
        if value.is_null() {
            return None;
        }
        let mut number: i32 = 0;
        let ok = CFNumberGetValue(
            value as CFNumberRef,
            CF_NUMBER_SINT32_TYPE,
            &mut number as *mut i32 as *mut c_void,
        );
        if ok == 0 {
            None
        } else {
            Some(number)
        }
    }
}

fn dictionary_string(dictionary: CFDictionaryRef, key: CFStringRef) -> Option<String> {
    unsafe {
        let value = CFDictionaryGetValue(dictionary, key as *const c_void);
        if value.is_null() {
            return None;
        }
        cf_string_to_rust(value as CFStringRef)
    }
}

fn cf_string_to_rust(value: CFStringRef) -> Option<String> {
    unsafe {
        let length = CFStringGetLength(value);
        if length < 0 {
            return None;
        }
        let mut buffer = vec![0i8; (length as usize) * 4 + 1];
        let ok = CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            CF_STRING_ENCODING_UTF8,
        );
        if ok == 0 {
            return None;
        }
        let bytes = buffer.iter().map(|b| *b as u8).take_while(|b| *b != 0).collect::<Vec<_>>();
        String::from_utf8(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{applescript_literal, bundle_looks_like_iterm, name_looks_like_iterm};

    #[test]
    fn recognises_iterm_process_names() {
        assert!(name_looks_like_iterm("iTerm"));
        assert!(name_looks_like_iterm("iTerm2"));
        assert!(name_looks_like_iterm("iTerm.app"));
        assert!(bundle_looks_like_iterm("com.googlecode.iterm2"));
        assert!(!name_looks_like_iterm("Terminal"));
        assert!(!name_looks_like_iterm("TextEdit"));
    }

    #[test]
    fn quotes_applescript_text() {
        assert_eq!(applescript_literal(r#"say "hi""#), r#""say \"hi\"""#);
    }
}
