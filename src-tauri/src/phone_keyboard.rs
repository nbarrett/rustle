use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, Ordering};

static KEYBOARD_SESSION: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn rustle_publish_keyboard_transcript(text: *const c_char);
    fn rustle_set_keyboard_phase(phase: *const c_char);
    fn rustle_return_to_host_app();
    fn rustle_prepare_phone_audio_session();
    fn rustle_listen_for_keyboard_stop(handler: extern "C" fn());
    fn rustle_begin_transcribe_background_task();
    fn rustle_end_transcribe_background_task();
}

pub fn url_asks_to_dictate(url: &str) -> bool {
    url.starts_with("rustle://dictate")
}

pub fn mark_keyboard_session() {
    KEYBOARD_SESSION.store(true, Ordering::SeqCst);
}

pub fn keyboard_session() -> bool {
    KEYBOARD_SESSION.load(Ordering::SeqCst)
}

pub fn end_keyboard_session() {
    KEYBOARD_SESSION.store(false, Ordering::SeqCst);
    set_phase("idle");
    end_transcribe_background_task();
}

pub fn prepare_audio_session() {
    unsafe {
        rustle_prepare_phone_audio_session();
    }
}

pub fn listen_for_stop(handler: extern "C" fn()) {
    unsafe {
        rustle_listen_for_keyboard_stop(handler);
    }
}

pub fn return_to_host_app() {
    unsafe {
        rustle_return_to_host_app();
    }
}

pub fn begin_transcribe_background_task() {
    unsafe {
        rustle_begin_transcribe_background_task();
    }
}

pub fn end_transcribe_background_task() {
    unsafe {
        rustle_end_transcribe_background_task();
    }
}

pub fn set_phase(phase: &str) {
    let Ok(c_phase) = CString::new(phase) else {
        return;
    };
    unsafe {
        rustle_set_keyboard_phase(c_phase.as_ptr());
    }
}

pub fn publish_transcript(text: &str) {
    let Ok(c_text) = CString::new(text) else {
        return;
    };
    unsafe {
        rustle_publish_keyboard_transcript(c_text.as_ptr());
    }
}
