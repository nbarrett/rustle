use std::os::raw::c_void;

const AUDIO_OBJECT_SYSTEM: u32 = 1;
const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = u32::from_be_bytes(*b"dOut");
const AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
const AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;
const AUDIO_DEVICE_PROPERTY_MUTE: u32 = u32::from_be_bytes(*b"mute");
const AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
const AUDIO_HARDWARE_SERVICE_DEVICE_VIRTUAL_MAIN_VOLUME: u32 = u32::from_be_bytes(*b"vmvc");

#[repr(C)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectHasProperty(object_id: u32, address: *const AudioObjectPropertyAddress) -> u8;
    fn AudioObjectGetPropertyData(
        object_id: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        object_id: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> i32;
}

#[derive(Clone, Copy, Debug)]
pub enum SilencedOutput {
    AlreadySilent,
    Muted,
    VolumeLowered { previous: f32 },
}

fn default_output_device() -> Option<u32> {
    let address = AudioObjectPropertyAddress {
        selector: AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
        scope: AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut device: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            AUDIO_OBJECT_SYSTEM,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            (&raw mut device).cast(),
        )
    };
    if status == 0 && device != 0 {
        Some(device)
    } else {
        None
    }
}

fn mute_address() -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector: AUDIO_DEVICE_PROPERTY_MUTE,
        scope: AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    }
}

fn volume_address() -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector: AUDIO_HARDWARE_SERVICE_DEVICE_VIRTUAL_MAIN_VOLUME,
        scope: AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
        element: AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    }
}

fn device_has_property(device: u32, address: &AudioObjectPropertyAddress) -> bool {
    unsafe { AudioObjectHasProperty(device, address) != 0 }
}

fn read_mute(device: u32) -> Option<bool> {
    let address = mute_address();
    if !device_has_property(device, &address) {
        return None;
    }
    let mut muted: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            (&raw mut muted).cast(),
        )
    };
    if status == 0 {
        Some(muted != 0)
    } else {
        None
    }
}

fn write_mute(device: u32, muted: bool) -> bool {
    let address = mute_address();
    if !device_has_property(device, &address) {
        return false;
    }
    let value: u32 = u32::from(muted);
    unsafe {
        AudioObjectSetPropertyData(
            device,
            &address,
            0,
            std::ptr::null(),
            std::mem::size_of::<u32>() as u32,
            (&raw const value).cast(),
        ) == 0
    }
}

fn read_volume(device: u32) -> Option<f32> {
    let address = volume_address();
    if !device_has_property(device, &address) {
        return None;
    }
    let mut volume: f32 = 0.0;
    let mut size = std::mem::size_of::<f32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            (&raw mut volume).cast(),
        )
    };
    if status == 0 {
        Some(volume)
    } else {
        None
    }
}

fn write_volume(device: u32, volume: f32) -> bool {
    let address = volume_address();
    if !device_has_property(device, &address) {
        return false;
    }
    let value = volume.clamp(0.0, 1.0);
    unsafe {
        AudioObjectSetPropertyData(
            device,
            &address,
            0,
            std::ptr::null(),
            std::mem::size_of::<f32>() as u32,
            (&raw const value).cast(),
        ) == 0
    }
}

pub fn silence_system_output() -> Option<SilencedOutput> {
    let device = default_output_device()?;
    if let Some(true) = read_mute(device) {
        return Some(SilencedOutput::AlreadySilent);
    }
    if write_mute(device, true) {
        return Some(SilencedOutput::Muted);
    }
    let previous = read_volume(device)?;
    if previous == 0.0 {
        return Some(SilencedOutput::AlreadySilent);
    }
    if write_volume(device, 0.0) {
        Some(SilencedOutput::VolumeLowered { previous })
    } else {
        None
    }
}

pub fn restore_system_output(saved: SilencedOutput) {
    let Some(device) = default_output_device() else {
        return;
    };
    match saved {
        SilencedOutput::AlreadySilent => {}
        SilencedOutput::Muted => {
            let _ = write_mute(device, false);
        }
        SilencedOutput::VolumeLowered { previous } => {
            let _ = write_volume(device, previous);
        }
    }
}
