use anyhow::{anyhow, Result};
use std::ffi::c_void;
use std::io::Write;
use std::process::{Command, Stdio};

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

pub struct FrontApp {
    pub name: String,
    pub pid: i32,
}

pub fn name_looks_like_iterm(name: &str) -> bool {
    name.to_ascii_lowercase().contains("iterm")
}

pub fn frontmost_app() -> Option<FrontApp> {
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
            if name.is_empty() {
                continue;
            }
            found = Some(FrontApp { name, pid });
            break;
        }
        CFRelease(layer_key as CFTypeRef);
        CFRelease(name_key as CFTypeRef);
        CFRelease(pid_key as CFTypeRef);
        CFRelease(windows);
        found
    }
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
    let script = r#"on run argv
  set theDeletes to item 1 of argv
  set theText to item 2 of argv
  set shouldReturn to item 3 of argv
  tell application "iTerm"
    tell current session of current window
      if (count of theDeletes) > 0 then
        write text theDeletes newline no
      end if
      if (count of theText) > 0 then
        write text theText newline no
      end if
      if shouldReturn is "yes" then
        write text "" newline yes
      end if
    end tell
  end tell
end run"#;
    let return_flag = if press_return { "yes" } else { "no" };
    let output = run_osascript(script, &[&deletes, text, return_flag])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "iTerm write failed: {}",
            stderr.trim().replace('\n', " ")
        ));
    }
    Ok(())
}

pub fn post_command_v_keystroke() -> Result<()> {
    unsafe {
        let source = create_event_source()?;
        post_key(source, KEYCODE_COMMAND, true, EVENT_FLAG_COMMAND);
        post_key(source, KEYCODE_ANSI_V, true, EVENT_FLAG_COMMAND);
        post_key(source, KEYCODE_ANSI_V, false, EVENT_FLAG_COMMAND);
        post_key(source, KEYCODE_COMMAND, false, 0);
        CFRelease(source as *const c_void);
    }
    Ok(())
}

pub fn post_delete_keystrokes(count: usize) -> Result<()> {
    if count == 0 {
        return Ok(());
    }
    unsafe {
        let source = create_event_source()?;
        for _ in 0..count {
            post_key(source, KEYCODE_DELETE, true, 0);
            post_key(source, KEYCODE_DELETE, false, 0);
        }
        CFRelease(source as *const c_void);
    }
    Ok(())
}

pub fn post_return_keystroke() -> Result<()> {
    unsafe {
        let source = create_event_source()?;
        post_key(source, KEYCODE_RETURN, true, 0);
        post_key(source, KEYCODE_RETURN, false, 0);
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

unsafe fn post_key(source: CGEventSourceRef, keycode: u16, key_down: bool, flags: u64) {
    let event = CGEventCreateKeyboardEvent(source, keycode, key_down);
    if event.is_null() {
        return;
    }
    CGEventSetFlags(event, flags);
    CGEventPost(HID_EVENT_TAP, event);
    CFRelease(event as *const c_void);
}

fn run_osascript(script: &str, args: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new("osascript");
    command.arg("-").args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| anyhow!("could not start osascript: {error}"))?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("osascript stdin was unavailable"))?;
        stdin
            .write_all(script.as_bytes())
            .map_err(|error| anyhow!("could not send iTerm script: {error}"))?;
    }
    child
        .wait_with_output()
        .map_err(|error| anyhow!("osascript did not finish: {error}"))
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
    use super::name_looks_like_iterm;

    #[test]
    fn recognises_iterm_process_names() {
        assert!(name_looks_like_iterm("iTerm"));
        assert!(name_looks_like_iterm("iTerm2"));
        assert!(name_looks_like_iterm("iTerm.app"));
        assert!(!name_looks_like_iterm("Terminal"));
        assert!(!name_looks_like_iterm("TextEdit"));
    }
}
