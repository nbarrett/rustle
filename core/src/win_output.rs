use crate::output::SilencedOutput;
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

pub fn silence_system_output() -> Option<SilencedOutput> {
    let volume = endpoint_volume()?;
    let already_muted = unsafe { volume.GetMute() }.ok()?.as_bool();
    if already_muted {
        return Some(SilencedOutput::AlreadySilent);
    }
    unsafe { volume.SetMute(true, std::ptr::null()) }
        .ok()
        .map(|_| SilencedOutput::Muted)
}

pub fn restore_system_output(saved: SilencedOutput) {
    if !matches!(saved, SilencedOutput::Muted) {
        return;
    }
    if let Some(volume) = endpoint_volume() {
        let _ = unsafe { volume.SetMute(false, std::ptr::null()) };
    }
}

fn endpoint_volume() -> Option<IAudioEndpointVolume> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok()?;
        device
            .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .ok()
    }
}
