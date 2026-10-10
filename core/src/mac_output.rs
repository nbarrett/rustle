use std::os::raw::c_void;

const AUDIO_OBJECT_SYSTEM: u32 = 1;
const AUDIO_HARDWARE_PROPERTY_DEFAULT_INPUT_DEVICE: u32 = u32::from_be_bytes(*b"dIn ");
const AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: u32 = u32::from_be_bytes(*b"dOut");
const AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
const AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;
const AUDIO_OBJECT_PROPERTY_OWNED_OBJECTS: u32 = u32::from_be_bytes(*b"ownd");
const AUDIO_OBJECT_PROPERTY_CLASS: u32 = u32::from_be_bytes(*b"clas");
const AUDIO_DEVICE_PROPERTY_MUTE: u32 = u32::from_be_bytes(*b"mute");
const AUDIO_DEVICE_PROPERTY_VOLUME_SCALAR: u32 = u32::from_be_bytes(*b"volm");
const AUDIO_DEVICE_PROPERTY_HOG_MODE: u32 = u32::from_be_bytes(*b"hogm");
const AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
const AUDIO_HARDWARE_SERVICE_DEVICE_VIRTUAL_MAIN_VOLUME: u32 = u32::from_be_bytes(*b"vmvc");
const AUDIO_MUTE_CONTROL_CLASS: u32 = u32::from_be_bytes(*b"mute");
const AUDIO_VOLUME_CONTROL_CLASS: u32 = u32::from_be_bytes(*b"vlme");
const AUDIO_BOOLEAN_CONTROL_VALUE: u32 = u32::from_be_bytes(*b"bcvl");
const AUDIO_LEVEL_CONTROL_SCALAR_VALUE: u32 = u32::from_be_bytes(*b"lcsv");
const AUDIO_CONTROL_SCOPE: u32 = u32::from_be_bytes(*b"cscp");
const UNOWNED_HOG: i32 = -1;
const OUTPUT_ELEMENTS: [u32; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 8];

#[repr(C)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectHasProperty(object_id: u32, address: *const AudioObjectPropertyAddress) -> u8;
    fn AudioObjectIsPropertySettable(
        object_id: u32,
        address: *const AudioObjectPropertyAddress,
        settable: *mut u8,
    ) -> i32;
    fn AudioObjectGetPropertyDataSize(
        object_id: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: *mut u32,
    ) -> i32;
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

pub use crate::output::SilencedOutput;

fn property_address(selector: u32, scope: u32, element: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector,
        scope,
        element,
    }
}

fn global_main(selector: u32) -> AudioObjectPropertyAddress {
    property_address(
        selector,
        AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    )
}

fn output_main(selector: u32) -> AudioObjectPropertyAddress {
    property_address(
        selector,
        AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
        AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    )
}

fn device_has_property(object: u32, address: &AudioObjectPropertyAddress) -> bool {
    unsafe { AudioObjectHasProperty(object, address) != 0 }
}

fn property_is_settable(object: u32, address: &AudioObjectPropertyAddress) -> bool {
    if !device_has_property(object, address) {
        return false;
    }
    let mut settable: u8 = 0;
    unsafe { AudioObjectIsPropertySettable(object, address, &mut settable) == 0 && settable != 0 }
}

fn read_u32(object: u32, address: &AudioObjectPropertyAddress) -> Option<u32> {
    if !device_has_property(object, address) {
        return None;
    }
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            &mut size,
            (&raw mut value).cast(),
        )
    };
    if status == 0 {
        Some(value)
    } else {
        None
    }
}

fn write_u32(object: u32, address: &AudioObjectPropertyAddress, value: u32) -> bool {
    if !property_is_settable(object, address) {
        return false;
    }
    unsafe {
        AudioObjectSetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            std::mem::size_of::<u32>() as u32,
            (&raw const value).cast(),
        ) == 0
    }
}

fn read_i32(object: u32, address: &AudioObjectPropertyAddress) -> Option<i32> {
    if !device_has_property(object, address) {
        return None;
    }
    let mut value: i32 = 0;
    let mut size = std::mem::size_of::<i32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            &mut size,
            (&raw mut value).cast(),
        )
    };
    if status == 0 {
        Some(value)
    } else {
        None
    }
}

fn write_i32(object: u32, address: &AudioObjectPropertyAddress, value: i32) -> bool {
    if !property_is_settable(object, address) {
        return false;
    }
    unsafe {
        AudioObjectSetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            std::mem::size_of::<i32>() as u32,
            (&raw const value).cast(),
        ) == 0
    }
}

fn read_f32(object: u32, address: &AudioObjectPropertyAddress) -> Option<f32> {
    if !device_has_property(object, address) {
        return None;
    }
    let mut value: f32 = 0.0;
    let mut size = std::mem::size_of::<f32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            &mut size,
            (&raw mut value).cast(),
        )
    };
    if status == 0 {
        Some(value)
    } else {
        None
    }
}

fn write_f32(object: u32, address: &AudioObjectPropertyAddress, value: f32) -> bool {
    if !property_is_settable(object, address) {
        return false;
    }
    let clamped = value.clamp(0.0, 1.0);
    unsafe {
        AudioObjectSetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            std::mem::size_of::<f32>() as u32,
            (&raw const clamped).cast(),
        ) == 0
    }
}

fn default_audio_device(selector: u32) -> Option<u32> {
    let address = global_main(selector);
    let device = read_u32(AUDIO_OBJECT_SYSTEM, &address)?;
    if device == 0 {
        None
    } else {
        Some(device)
    }
}

fn default_output_device() -> Option<u32> {
    default_audio_device(AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE)
}

fn default_input_device() -> Option<u32> {
    default_audio_device(AUDIO_HARDWARE_PROPERTY_DEFAULT_INPUT_DEVICE)
}

fn default_input_and_output_are_the_same_device() -> bool {
    match (default_input_device(), default_output_device()) {
        (Some(input), Some(output)) => input == output,
        _ => false,
    }
}

fn object_class(object: u32) -> Option<u32> {
    read_u32(object, &global_main(AUDIO_OBJECT_PROPERTY_CLASS))
}

fn control_scope(object: u32) -> Option<u32> {
    read_u32(object, &global_main(AUDIO_CONTROL_SCOPE))
}

fn control_is_output(object: u32) -> bool {
    match control_scope(object) {
        Some(scope) => {
            scope == AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT || scope == AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL
        }
        None => true,
    }
}

fn owned_objects(object: u32) -> Vec<u32> {
    let address = global_main(AUDIO_OBJECT_PROPERTY_OWNED_OBJECTS);
    if !device_has_property(object, &address) {
        return Vec::new();
    }
    let mut size: u32 = 0;
    let size_status = unsafe {
        AudioObjectGetPropertyDataSize(object, &address, 0, std::ptr::null(), &mut size)
    };
    if size_status != 0 || size == 0 {
        return Vec::new();
    }
    let count = size as usize / std::mem::size_of::<u32>();
    let mut ids = vec![0u32; count];
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            ids.as_mut_ptr().cast(),
        )
    };
    if status == 0 {
        ids.into_iter().filter(|id| *id != 0).collect()
    } else {
        Vec::new()
    }
}

fn owned_objects_of_class(device: u32, class_id: u32) -> Vec<u32> {
    owned_objects(device)
        .into_iter()
        .filter(|object| object_class(*object) == Some(class_id))
        .collect()
}

fn mute_device_output(device: u32) -> bool {
    let mut wrote = false;
    for element in OUTPUT_ELEMENTS {
        let address = property_address(
            AUDIO_DEVICE_PROPERTY_MUTE,
            AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            element,
        );
        if write_u32(device, &address, 1) {
            wrote = true;
        }
    }
    for control in owned_objects_of_class(device, AUDIO_MUTE_CONTROL_CLASS) {
        if !control_is_output(control) {
            continue;
        }
        if write_u32(control, &global_main(AUDIO_BOOLEAN_CONTROL_VALUE), 1) {
            wrote = true;
        }
    }
    wrote
}

fn unmute_device_output(device: u32) {
    for element in OUTPUT_ELEMENTS {
        let address = property_address(
            AUDIO_DEVICE_PROPERTY_MUTE,
            AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            element,
        );
        let _ = write_u32(device, &address, 0);
    }
    for control in owned_objects_of_class(device, AUDIO_MUTE_CONTROL_CLASS) {
        if !control_is_output(control) {
            continue;
        }
        let _ = write_u32(control, &global_main(AUDIO_BOOLEAN_CONTROL_VALUE), 0);
    }
}

fn output_already_muted(device: u32) -> bool {
    let address = output_main(AUDIO_DEVICE_PROPERTY_MUTE);
    matches!(read_u32(device, &address), Some(value) if value != 0)
}

fn volume_addresses(object: u32) -> Vec<AudioObjectPropertyAddress> {
    let mut addresses = Vec::new();
    for selector in [
        AUDIO_HARDWARE_SERVICE_DEVICE_VIRTUAL_MAIN_VOLUME,
        AUDIO_DEVICE_PROPERTY_VOLUME_SCALAR,
        AUDIO_LEVEL_CONTROL_SCALAR_VALUE,
    ] {
        for scope in [
            AUDIO_DEVICE_PROPERTY_SCOPE_OUTPUT,
            AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        ] {
            for element in OUTPUT_ELEMENTS {
                let address = property_address(selector, scope, element);
                if property_is_settable(object, &address) {
                    addresses.push(address);
                }
            }
        }
    }
    addresses
}

fn first_readable_volume(object: u32) -> Option<(AudioObjectPropertyAddress, f32)> {
    volume_addresses(object).into_iter().find_map(|address| {
        let volume = read_f32(object, &address)?;
        Some((address, volume))
    })
}

fn lower_output_volume(device: u32) -> Option<(u32, f32)> {
    let mut objects = vec![device];
    objects.extend(
        owned_objects_of_class(device, AUDIO_VOLUME_CONTROL_CLASS)
            .into_iter()
            .filter(|control| control_is_output(*control)),
    );
    for object in objects {
        let Some((address, previous)) = first_readable_volume(object) else {
            continue;
        };
        if previous == 0.0 {
            continue;
        }
        if write_f32(object, &address, 0.0) {
            return Some((object, previous));
        }
    }
    None
}

fn restore_output_volume(object: u32, previous: f32) {
    for address in volume_addresses(object) {
        if write_f32(object, &address, previous) {
            return;
        }
    }
}

fn hog_address() -> AudioObjectPropertyAddress {
    global_main(AUDIO_DEVICE_PROPERTY_HOG_MODE)
}

fn hog_output_device(device: u32) -> Option<u32> {
    if default_input_and_output_are_the_same_device() {
        return None;
    }
    let address = hog_address();
    let current = read_i32(device, &address)?;
    let this_process = std::process::id() as i32;
    if current == this_process {
        return Some(device);
    }
    if current != UNOWNED_HOG {
        return None;
    }
    if write_i32(device, &address, this_process) {
        Some(device)
    } else {
        None
    }
}

fn release_hog(device: u32) {
    let address = hog_address();
    let this_process = std::process::id() as i32;
    if read_i32(device, &address) == Some(this_process) {
        let _ = write_i32(device, &address, UNOWNED_HOG);
    }
}

pub fn silence_system_output() -> Option<SilencedOutput> {
    let device = default_output_device()?;
    if output_already_muted(device) {
        return Some(SilencedOutput::AlreadySilent);
    }
    if mute_device_output(device) {
        return Some(SilencedOutput::Muted);
    }
    let lowered = lower_output_volume(device);
    let hogged = hog_output_device(device);
    match (lowered, hogged) {
        (Some((object, previous)), hogged_device) => Some(SilencedOutput::VolumeLowered {
            previous,
            object,
            hogged_device,
        }),
        (None, Some(hogged_device)) => Some(SilencedOutput::Hogged {
            device: hogged_device,
        }),
        (None, None) => None,
    }
}

pub fn restore_system_output(saved: SilencedOutput) {
    match saved {
        SilencedOutput::AlreadySilent => {}
        SilencedOutput::Muted => {
            if let Some(device) = default_output_device() {
                unmute_device_output(device);
            }
        }
        SilencedOutput::VolumeLowered {
            previous,
            object,
            hogged_device,
        } => {
            restore_output_volume(object, previous);
            if let Some(device) = hogged_device {
                release_hog(device);
            }
        }
        SilencedOutput::Hogged { device } => {
            release_hog(device);
        }
    }
}
