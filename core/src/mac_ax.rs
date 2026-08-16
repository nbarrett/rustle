use anyhow::{anyhow, Result};
use std::ffi::c_void;
use std::ptr;

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type AXUIElementRef = *mut c_void;
type AXValueRef = *mut c_void;

const AX_ERROR_SUCCESS: i32 = 0;
const AX_ERROR_API_DISABLED: i32 = -25211;
const AX_VALUE_CF_RANGE: u32 = 4;
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

#[repr(C)]
#[derive(Clone, Copy)]
struct CFRange {
    location: isize,
    length: isize,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXValueCreate(value_type: u32, value_ptr: *const c_void) -> AXValueRef;
    fn AXValueGetValue(value: AXValueRef, value_type: u32, value_ptr: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> *const c_void;
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        num_bytes: isize,
        encoding: u32,
        is_external_representation: u8,
    ) -> CFStringRef;
    fn CFRelease(cf: CFTypeRef);
}

extern "C" {
    static kCFBooleanTrue: *const c_void;
}

pub fn process_is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn request_trust_prompt() -> bool {
    unsafe {
        let key = cf_string("AXTrustedCheckOptionPrompt");
        if key.is_null() {
            return AXIsProcessTrusted();
        }
        let keys = [key as *const c_void];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            ptr::null(),
            ptr::null(),
        );
        let trusted = if options.is_null() {
            AXIsProcessTrusted()
        } else {
            let trusted = AXIsProcessTrustedWithOptions(options);
            CFRelease(options);
            trusted
        };
        CFRelease(key);
        trusted
    }
}

pub fn replace_in_focused_field(origin_utf16: Option<i64>, previous: &str, current: &str) -> Result<i64> {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return Err(anyhow!("accessibility system element was unavailable"));
        }
        let focused_attribute = cf_string("AXFocusedUIElement");
        let mut focused: CFTypeRef = ptr::null();
        let focused_status = AXUIElementCopyAttributeValue(
            system,
            focused_attribute,
            &mut focused,
        );
        CFRelease(focused_attribute);
        CFRelease(system as CFTypeRef);
        if focused_status == AX_ERROR_API_DISABLED {
            return Err(anyhow!(
                "could not read the focused field (AX {AX_ERROR_API_DISABLED})"
            ));
        }
        if focused_status != AX_ERROR_SUCCESS || focused.is_null() {
            return Err(anyhow!(
                "could not read the focused field (AX {focused_status})"
            ));
        }

        let previous_len = utf16_len(previous);
        let origin = match origin_utf16 {
            Some(origin) => origin,
            None => selection_location(focused as AXUIElementRef)?,
        };

        if previous_len > 0 {
            set_selected_range(focused as AXUIElementRef, origin, previous_len)?;
        }
        set_selected_text(focused as AXUIElementRef, current)?;
        CFRelease(focused);
        Ok(origin)
    }
}

fn selection_location(element: AXUIElementRef) -> Result<i64> {
    let range = copy_selected_range(element)?;
    Ok(range.location as i64)
}

fn copy_selected_range(element: AXUIElementRef) -> Result<CFRange> {
    unsafe {
        let attribute = cf_string("AXSelectedTextRange");
        let mut value: CFTypeRef = ptr::null();
        let status = AXUIElementCopyAttributeValue(element, attribute, &mut value);
        CFRelease(attribute);
        if status != AX_ERROR_SUCCESS || value.is_null() {
            return Err(anyhow!("could not read caret position (AX {status})"));
        }
        let mut range = CFRange {
            location: 0,
            length: 0,
        };
        let ok = AXValueGetValue(value as AXValueRef, AX_VALUE_CF_RANGE, &mut range as *mut CFRange as *mut c_void);
        CFRelease(value);
        if !ok {
            return Err(anyhow!("caret position was not a text range"));
        }
        Ok(range)
    }
}

fn set_selected_range(element: AXUIElementRef, location: i64, length: i64) -> Result<()> {
    unsafe {
        let range = CFRange {
            location: location as isize,
            length: length as isize,
        };
        let value = AXValueCreate(AX_VALUE_CF_RANGE, &range as *const CFRange as *const c_void);
        if value.is_null() {
            return Err(anyhow!("could not build a text range"));
        }
        let attribute = cf_string("AXSelectedTextRange");
        let status = AXUIElementSetAttributeValue(element, attribute, value as CFTypeRef);
        CFRelease(attribute);
        CFRelease(value as CFTypeRef);
        if status != AX_ERROR_SUCCESS {
            return Err(anyhow!("could not select inserted text (AX {status})"));
        }
        Ok(())
    }
}

fn set_selected_text(element: AXUIElementRef, text: &str) -> Result<()> {
    unsafe {
        let cf_text = CFStringCreateWithBytes(
            ptr::null(),
            text.as_ptr(),
            text.len() as isize,
            CF_STRING_ENCODING_UTF8,
            0,
        );
        if cf_text.is_null() {
            return Err(anyhow!("could not build insert text"));
        }
        let attribute = cf_string("AXSelectedText");
        let status = AXUIElementSetAttributeValue(element, attribute, cf_text);
        CFRelease(attribute);
        CFRelease(cf_text);
        if status != AX_ERROR_SUCCESS {
            return Err(anyhow!("could not insert text (AX {status})"));
        }
        Ok(())
    }
}

fn utf16_len(text: &str) -> i64 {
    text.encode_utf16().count() as i64
}

fn cf_string(text: &str) -> CFStringRef {
    unsafe {
        CFStringCreateWithBytes(
            ptr::null(),
            text.as_ptr(),
            text.len() as isize,
            CF_STRING_ENCODING_UTF8,
            0,
        )
    }
}
