use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

static RECORDING_REQUEST: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    fn rustle_request_microphone_permission(
        context: *mut c_void,
        completion: extern "C" fn(*mut c_void, bool),
    );
}

pub fn replace_recording_request() -> u64 {
    RECORDING_REQUEST.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn recording_request_is_current(request: u64) -> bool {
    RECORDING_REQUEST.load(Ordering::SeqCst) == request
}

extern "C" fn send_microphone_permission_result(context: *mut c_void, granted: bool) {
    let sender = unsafe { Box::from_raw(context.cast::<mpsc::Sender<bool>>()) };
    let _ = sender.send(granted);
}

pub async fn request_permission_and_prepare_microphone() -> bool {
    tauri::async_runtime::spawn_blocking(|| {
        let (sender, receiver) = mpsc::channel();
        let context = Box::into_raw(Box::new(sender)).cast::<c_void>();
        unsafe {
            rustle_request_microphone_permission(context, send_microphone_permission_result);
        }
        receiver.recv().unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}
