use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::{Mutex, OnceLock};

use crate::hud_placement::ScreenRectangle;

#[repr(C)]
struct AccessibilityPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
struct AccessibilityRectangle {
    origin: AccessibilityPoint,
    size: AccessibilitySize,
}

#[repr(C)]
struct AccessibilitySize {
    width: f64,
    height: f64,
}

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type AXUIElementRef = *mut c_void;
type AXValueRef = *mut c_void;

const AX_ERROR_SUCCESS: i32 = 0;
const AX_ERROR_API_DISABLED: i32 = -25211;
const AX_VALUE_CF_RANGE: u32 = 4;
const AX_VALUE_CG_RECT: u32 = 3;
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

#[repr(C)]
#[derive(Clone, Copy)]
struct CFRange {
    location: isize,
    length: isize,
}

#[repr(C)]
struct CfDictionaryKeyCallBacks {
    version: isize,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
    hash: *const c_void,
}

#[repr(C)]
struct CfDictionaryValueCallBacks {
    version: isize,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
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
    static kCFBooleanTrue: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: CfDictionaryKeyCallBacks;
    static kCFTypeDictionaryValueCallBacks: CfDictionaryValueCallBacks;
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        num_bytes: isize,
        encoding: u32,
        is_external_representation: u8,
    ) -> CFStringRef;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        num_values: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFStringGetLength(the_string: CFStringRef) -> isize;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut i8,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
}

pub fn process_is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn request_accessibility_prompt() -> bool {
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt as CFTypeRef];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const CfDictionaryKeyCallBacks as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const CfDictionaryValueCallBacks as *const c_void,
        );
        if options.is_null() {
            return AXIsProcessTrusted();
        }
        let trusted = AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
        trusted
    }
}

pub fn replace_in_target_field(
    target_pid: Option<i32>,
    origin_utf16: Option<i64>,
    previous: &str,
    current: &str,
) -> Result<i64> {
    unsafe {
        let focused = copy_focused_ui_element(target_pid)?;
        let result = (|| {
            let previous_len = utf16_len(previous);
            let origin = match origin_utf16 {
                Some(origin) => origin,
                None => selection_location(focused as AXUIElementRef)?,
            };
            if previous_len > 0 {
                set_selected_range(focused as AXUIElementRef, origin, previous_len)?;
            }
            set_selected_text(focused as AXUIElementRef, current)?;
            Ok(origin)
        })();
        CFRelease(focused);
        result
    }
}

pub fn text_before_the_caret_in_the_target_field(target_pid: Option<i32>) -> Result<String> {
    Ok(insert_context_for_the_target_field(target_pid)?.text_before_caret)
}

pub struct FocusedFieldInsertContext {
    pub text_before_caret: String,
    pub text_after_caret: String,
    pub is_a_single_line_field: bool,
}

pub fn insert_context_for_the_target_field(
    target_pid: Option<i32>,
) -> Result<FocusedFieldInsertContext> {
    unsafe {
        let focused = copy_focused_ui_element(target_pid)?;
        let caret = copy_selected_range(focused as AXUIElementRef);
        let field_text = copy_string_attribute(focused as AXUIElementRef, "AXValue");
        let role = copy_string_attribute(focused as AXUIElementRef, "AXRole").unwrap_or_default();
        CFRelease(focused);
        let range = caret?;
        let (prefix, suffix) =
            split_field_around_utf16_selection(&field_text?, range.location, range.length)?;
        let is_a_single_line_field = matches!(
            role.as_str(),
            "AXTextField" | "AXComboBox" | "AXSearchField"
        );
        Ok(FocusedFieldInsertContext {
            text_before_caret: prefix,
            text_after_caret: suffix,
            is_a_single_line_field,
        })
    }
}

pub fn focused_caret_bounds_in_screen_coordinates() -> Result<ScreenRectangle> {
    unsafe {
        let focused = copy_focused_ui_element(None)?;
        let result = (|| {
            let selection = copy_selected_range(focused as AXUIElementRef)?;
            let caret = CFRange {
                location: selection.location,
                length: 0,
            };
            let parameter =
                AXValueCreate(AX_VALUE_CF_RANGE, &caret as *const CFRange as *const c_void);
            if parameter.is_null() {
                return Err(anyhow!("could not create caret range"));
            }
            let attribute = cf_string("AXBoundsForRange");
            let mut value: CFTypeRef = ptr::null();
            let status = AXUIElementCopyParameterizedAttributeValue(
                focused as AXUIElementRef,
                attribute,
                parameter as CFTypeRef,
                &mut value,
            );
            CFRelease(attribute);
            CFRelease(parameter as CFTypeRef);
            if status != AX_ERROR_SUCCESS || value.is_null() {
                return Err(anyhow!("could not read caret bounds (AX {status})"));
            }
            let mut rect = AccessibilityRectangle {
                origin: AccessibilityPoint { x: 0.0, y: 0.0 },
                size: AccessibilitySize {
                    width: 0.0,
                    height: 0.0,
                },
            };
            let read = AXValueGetValue(
                value as AXValueRef,
                AX_VALUE_CG_RECT,
                &mut rect as *mut AccessibilityRectangle as *mut c_void,
            );
            CFRelease(value);
            if !read
                || !rect.origin.x.is_finite()
                || !rect.origin.y.is_finite()
                || !rect.size.width.is_finite()
                || !rect.size.height.is_finite()
                || rect.size.height <= 0.0
                || rect.size.width < 0.0
            {
                return Err(anyhow!("caret bounds were invalid"));
            }
            Ok(ScreenRectangle {
                x: rect.origin.x,
                y: rect.origin.y,
                width: rect.size.width.max(1.0),
                height: rect.size.height,
            })
        })();
        CFRelease(focused);
        result
    }
}

pub fn focused_field_bounds_in_screen_coordinates() -> Result<ScreenRectangle> {
    unsafe {
        let focused = copy_focused_ui_element(None)?;
        let result = (|| {
            let mut position = AccessibilityPoint { x: 0.0, y: 0.0 };
            let mut size = AccessibilitySize {
                width: 0.0,
                height: 0.0,
            };
            copy_accessibility_geometry(
                focused as AXUIElementRef,
                "AXPosition",
                1,
                &mut position as *mut AccessibilityPoint as *mut c_void,
            )?;
            copy_accessibility_geometry(
                focused as AXUIElementRef,
                "AXSize",
                2,
                &mut size as *mut AccessibilitySize as *mut c_void,
            )?;
            if !position.x.is_finite()
                || !position.y.is_finite()
                || !size.width.is_finite()
                || !size.height.is_finite()
                || size.width <= 0.0
                || size.height <= 0.0
            {
                return Err(anyhow!("focused field bounds were invalid"));
            }
            Ok(ScreenRectangle {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            })
        })();
        CFRelease(focused);
        result
    }
}

unsafe fn copy_accessibility_geometry(
    element: AXUIElementRef,
    attribute_name: &str,
    value_type: u32,
    output: *mut c_void,
) -> Result<()> {
    let attribute = cf_string(attribute_name);
    let mut value: CFTypeRef = ptr::null();
    let status = AXUIElementCopyAttributeValue(element, attribute, &mut value);
    CFRelease(attribute);
    if status != AX_ERROR_SUCCESS || value.is_null() {
        return Err(anyhow!("could not read {attribute_name} (AX {status})"));
    }
    let read = AXValueGetValue(value as AXValueRef, value_type, output);
    CFRelease(value);
    if !read {
        return Err(anyhow!("{attribute_name} did not contain geometry"));
    }
    Ok(())
}

fn split_field_around_utf16_selection(
    text: &str,
    location: isize,
    length: isize,
) -> Result<(String, String)> {
    let location = usize::try_from(location).map_err(|_| anyhow!("negative caret location"))?;
    let length = usize::try_from(length).map_err(|_| anyhow!("negative selection length"))?;
    let end = location
        .checked_add(length)
        .ok_or_else(|| anyhow!("selection range overflowed"))?;
    let encoded: Vec<u16> = text.encode_utf16().collect();
    if end > encoded.len() {
        return Err(anyhow!("caret range exceeded the focused field"));
    }
    let prefix = String::from_utf16(&encoded[..location])
        .map_err(|_| anyhow!("caret split a Unicode character"))?;
    let suffix = String::from_utf16(&encoded[end..])
        .map_err(|_| anyhow!("selection split a Unicode character"))?;
    Ok((prefix, suffix))
}

fn apps_asked_to_build_an_accessibility_tree() -> &'static Mutex<HashMap<i32, bool>> {
    static APPS: OnceLock<Mutex<HashMap<i32, bool>>> = OnceLock::new();
    APPS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn app_only_reveals_its_text_when_asked(pid: i32) -> bool {
    apps_asked_to_build_an_accessibility_tree()
        .lock()
        .map(|asked| asked.get(&pid).copied().unwrap_or(false))
        .unwrap_or(false)
}

unsafe fn ask_chromium_to_build_its_accessibility_tree(application: AXUIElementRef, pid: i32) {
    let Ok(mut asked) = apps_asked_to_build_an_accessibility_tree().lock() else {
        return;
    };
    if asked.contains_key(&pid) {
        return;
    }
    let attribute = cf_string("AXManualAccessibility");
    let status = AXUIElementSetAttributeValue(application, attribute, kCFBooleanTrue);
    CFRelease(attribute);
    asked.insert(pid, status == AX_ERROR_SUCCESS);
}

unsafe fn copy_focused_ui_element(target_pid: Option<i32>) -> Result<CFTypeRef> {
    let application = match target_pid {
        Some(pid) => AXUIElementCreateApplication(pid),
        None => AXUIElementCreateSystemWide(),
    };
    if application.is_null() {
        return Err(anyhow!("accessibility system element was unavailable"));
    }
    if let Some(pid) = target_pid {
        ask_chromium_to_build_its_accessibility_tree(application, pid);
    }
    let focused_attribute = cf_string("AXFocusedUIElement");
    let mut focused: CFTypeRef = ptr::null();
    let focused_status =
        AXUIElementCopyAttributeValue(application, focused_attribute, &mut focused);
    CFRelease(focused_attribute);
    CFRelease(application as CFTypeRef);
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
    Ok(focused)
}

unsafe fn copy_string_attribute(element: AXUIElementRef, attribute_name: &str) -> Result<String> {
    let attribute = cf_string(attribute_name);
    let mut value: CFTypeRef = ptr::null();
    let status = AXUIElementCopyAttributeValue(element, attribute, &mut value);
    CFRelease(attribute);
    if status != AX_ERROR_SUCCESS || value.is_null() {
        return Err(anyhow!("could not read {attribute_name} (AX {status})"));
    }
    let text = cf_string_to_rust(value as CFStringRef);
    CFRelease(value);
    text.ok_or_else(|| anyhow!("{attribute_name} was not text"))
}

unsafe fn cf_string_to_rust(value: CFStringRef) -> Option<String> {
    let length = CFStringGetLength(value);
    let buffer_size = length * 4 + 1;
    let mut buffer = vec![0u8; buffer_size as usize];
    if CFStringGetCString(
        value,
        buffer.as_mut_ptr().cast(),
        buffer_size,
        CF_STRING_ENCODING_UTF8,
    ) == 0
    {
        return None;
    }
    let terminator = buffer.iter().position(|byte| *byte == 0)?;
    buffer.truncate(terminator);
    String::from_utf8(buffer).ok()
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
        let ok = AXValueGetValue(
            value as AXValueRef,
            AX_VALUE_CF_RANGE,
            &mut range as *mut CFRange as *mut c_void,
        );
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

#[cfg(test)]
mod insertion_context_tests {
    use super::split_field_around_utf16_selection;

    #[test]
    fn selected_text_is_excluded_from_insertion_context() {
        assert_eq!(
            split_field_around_utf16_selection("Please remove this today", 7, 11).unwrap(),
            ("Please ".to_string(), " today".to_string())
        );
    }

    #[test]
    fn caret_ranges_use_utf16_without_splitting_unicode_characters() {
        assert_eq!(
            split_field_around_utf16_selection("Hi 🦀 there", 5, 0).unwrap(),
            ("Hi 🦀".to_string(), " there".to_string())
        );
        assert!(split_field_around_utf16_selection("Hi 🦀 there", 4, 0).is_err());
    }

    #[test]
    fn invalid_caret_ranges_do_not_look_like_the_end_of_the_field() {
        for (location, length) in [(-1, 0), (0, -1), (5, 0), (3, 2), (isize::MAX, isize::MAX)] {
            assert!(split_field_around_utf16_selection("text", location, length).is_err());
        }
    }
}
