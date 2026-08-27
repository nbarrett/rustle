use objc2::runtime::AnyClass;
use objc2::msg_send;
use objc2_foundation::NSString;

const AV_MEDIA_TYPE_AUDIO: &str = "soun";
const AV_AUTHORIZATION_NOT_DETERMINED: i64 = 0;
const AV_AUTHORIZATION_RESTRICTED: i64 = 1;
const AV_AUTHORIZATION_DENIED: i64 = 2;
const AV_AUTHORIZATION_AUTHORIZED: i64 = 3;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

fn av_capture_device_class() -> Option<&'static AnyClass> {
    AnyClass::get(c"AVCaptureDevice")
}

fn microphone_authorization_status() -> Option<i64> {
    let class = av_capture_device_class()?;
    let media = NSString::from_str(AV_MEDIA_TYPE_AUDIO);
    Some(unsafe { msg_send![class, authorizationStatusForMediaType: &*media] })
}

pub fn microphone_access_is_granted() -> bool {
    microphone_authorization_status() == Some(AV_AUTHORIZATION_AUTHORIZED)
}

pub fn microphone_access_was_refused() -> bool {
    matches!(
        microphone_authorization_status(),
        Some(AV_AUTHORIZATION_RESTRICTED) | Some(AV_AUTHORIZATION_DENIED)
    )
}

pub fn prompt_for_microphone_access() {
    if microphone_access_is_granted() {
        return;
    }
    let Some(class) = av_capture_device_class() else {
        return;
    };
    let media = NSString::from_str(AV_MEDIA_TYPE_AUDIO);
    let block = block2::RcBlock::new(|_granted: objc2::runtime::Bool| {});
    unsafe {
        let _: () = msg_send![
            class,
            requestAccessForMediaType: &*media,
            completionHandler: &*block
        ];
    }
    std::mem::forget(block);
}

pub fn microphone_access_still_needs_a_prompt() -> bool {
    microphone_authorization_status() == Some(AV_AUTHORIZATION_NOT_DETERMINED)
}
