use anyhow::{anyhow, Result};
use std::ffi::c_void;

type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;

const SESSION_EVENT_TAP: u32 = 1;
const EVENT_SOURCE_STATE_COMBINED_SESSION: i32 = 0;
const KEYCODE_COMMAND: u16 = 0x37;
const KEYCODE_ANSI_V: u16 = 0x09;
const KEYCODE_DELETE: u16 = 0x33;
const EVENT_FLAG_COMMAND: u64 = 0x0010_0000;

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
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

pub fn post_command_v_keystroke() -> Result<()> {
    unsafe {
        let source = CGEventSourceCreate(EVENT_SOURCE_STATE_COMBINED_SESSION);
        if source.is_null() {
            return Err(anyhow!("failed to create event source for paste"));
        }
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
        let source = CGEventSourceCreate(EVENT_SOURCE_STATE_COMBINED_SESSION);
        if source.is_null() {
            return Err(anyhow!("failed to create event source for delete"));
        }
        for _ in 0..count {
            post_key(source, KEYCODE_DELETE, true, 0);
            post_key(source, KEYCODE_DELETE, false, 0);
        }
        CFRelease(source as *const c_void);
    }
    Ok(())
}

unsafe fn post_key(source: CGEventSourceRef, keycode: u16, key_down: bool, flags: u64) {
    let event = CGEventCreateKeyboardEvent(source, keycode, key_down);
    if event.is_null() {
        return;
    }
    CGEventSetFlags(event, flags);
    CGEventPost(SESSION_EVENT_TAP, event);
    CFRelease(event as *const c_void);
}
