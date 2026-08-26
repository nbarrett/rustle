use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::core::{BSTR, GUID};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, IAudioCaptureClient, IAudioClient,
    IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_NOPERSIST, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE,
    WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};
const SHARED_BUFFER_HNS: i64 = 1_000_000;
const POLL_IDLE: Duration = Duration::from_millis(5);
const WASAPI_PACKET_IS_SILENT: u32 = 2;

pub struct WasapiCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WasapiCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn start_wasapi_capture(
    preferred_device_name: Option<&str>,
) -> Result<(WasapiCapture, Arc<Mutex<Vec<f32>>>, u32, u16)> {
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel();
    let preferred = preferred_device_name.map(str::to_string);
    let samples_for_thread = samples.clone();
    let stop_for_thread = stop.clone();

    let thread = thread::Builder::new()
        .name("rustle-wasapi-in".into())
        .spawn(move || {
            capture_from_wasapi_until_stopped(
                preferred.as_deref(),
                samples_for_thread,
                &stop_for_thread,
                ready_tx,
            );
        })?;

    let (sample_rate, channels) = ready_rx
        .recv()
        .map_err(|_| anyhow!("wasapi capture thread ended before it started"))??;

    Ok((
        WasapiCapture {
            stop,
            thread: Some(thread),
        },
        samples,
        sample_rate,
        channels,
    ))
}

pub fn write_capture_log(message: &str) {
    let Ok(directory) = crate::config::rustle_directory() else {
        return;
    };
    let path = directory.join("engine.log");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            use std::io::Write;
            writeln!(file, "{stamp} {message}")
        });
}

fn capture_from_wasapi_until_stopped(
    preferred_device_name: Option<&str>,
    samples: Arc<Mutex<Vec<f32>>>,
    stop: &AtomicBool,
    ready: mpsc::Sender<Result<(u32, u16)>>,
) {
    let (audio_client, capture_client, sample_rate, channels, bits_per_sample, is_float) =
        match start_wasapi_capture_session(preferred_device_name) {
            Ok(session) => session,
            Err(error) => {
                write_capture_log(&format!("wasapi capture failed: {error}"));
                let _ = ready.send(Err(error));
                return;
            }
        };
    if ready.send(Ok((sample_rate, channels))).is_err() {
        let _ = unsafe { audio_client.Stop() };
        return;
    }
    while !stop.load(Ordering::SeqCst) {
        match copy_available_wasapi_packets(
            &capture_client,
            &samples,
            channels,
            bits_per_sample,
            is_float,
        ) {
            Ok(false) => thread::sleep(POLL_IDLE),
            Ok(true) => {}
            Err(error) => {
                write_capture_log(&format!("wasapi packet read failed: {error}"));
                break;
            }
        }
    }
    let _ = copy_available_wasapi_packets(
        &capture_client,
        &samples,
        channels,
        bits_per_sample,
        is_float,
    );
    let _ = unsafe { audio_client.Stop() };
}

fn start_wasapi_capture_session(
    preferred_device_name: Option<&str>,
) -> Result<(
    IAudioClient,
    IAudioCaptureClient,
    u32,
    u16,
    u16,
    bool,
)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = select_wasapi_capture_device(&enumerator, preferred_device_name)?;
        let device_name =
            friendly_name_of_wasapi_device(&device).unwrap_or_else(|| "-".to_string());
        let audio_client = open_shared_wasapi_capture_client(&device)?;
        let format_ptr = audio_client.GetMixFormat()?;
        if format_ptr.is_null() {
            return Err(anyhow!("wasapi mix format was missing"));
        }
        let format = *format_ptr;
        let is_float = mix_format_is_ieee_float(format_ptr);
        CoTaskMemFree(Some(format_ptr as *const _));
        if !mix_format_can_decode(format.wBitsPerSample, is_float) {
            return Err(anyhow!(
                "unsupported wasapi mix format bits={} float={is_float}",
                format.wBitsPerSample
            ));
        }
        let capture_client: IAudioCaptureClient = audio_client.GetService()?;
        audio_client.Start()?;
        write_capture_log(&format!(
            "wasapi capture started device={device_name:?} rate={} channels={} bits={} float={is_float}",
            format.nSamplesPerSec,
            format.nChannels,
            format.wBitsPerSample
        ));
        Ok((
            audio_client,
            capture_client,
            format.nSamplesPerSec,
            format.nChannels,
            format.wBitsPerSample,
            is_float,
        ))
    }
}

fn open_shared_wasapi_capture_client(device: &IMMDevice) -> Result<IAudioClient> {
    unsafe {
        let format_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format_ptr = format_client.GetMixFormat()?;
        if format_ptr.is_null() {
            return Err(anyhow!("wasapi mix format was missing"));
        }
        let opened = initialize_shared_wasapi_capture_client(device, format_ptr);
        CoTaskMemFree(Some(format_ptr as *const _));
        opened
    }
}

fn initialize_shared_wasapi_capture_client(
    device: &IMMDevice,
    format_ptr: *const WAVEFORMATEX,
) -> Result<IAudioClient> {
    unsafe {
        let flag_sets = [
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
                | AUDCLNT_STREAMFLAGS_NOPERSIST,
            AUDCLNT_STREAMFLAGS_NOPERSIST,
            0,
        ];
        let mut last_error = None;
        for flags in flag_sets {
            let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            match audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                SHARED_BUFFER_HNS,
                0,
                format_ptr,
                None,
            ) {
                Ok(()) => return Ok(audio_client),
                Err(error) => last_error = Some(error),
            }
        }
        Err(anyhow!(
            "wasapi initialize failed: {}",
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ))
    }
}

fn select_wasapi_capture_device(
    enumerator: &IMMDeviceEnumerator,
    preferred_device_name: Option<&str>,
) -> Result<IMMDevice> {
    unsafe {
        if let Some(wanted) = preferred_device_name {
            let collection = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;
            for index in 0..count {
                let device = collection.Item(index)?;
                if friendly_name_of_wasapi_device(&device).as_deref() == Some(wanted) {
                    return Ok(device);
                }
            }
        }
        enumerator
            .GetDefaultAudioEndpoint(eCapture, eMultimedia)
            .or_else(|_| enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications))
            .or_else(|_| enumerator.GetDefaultAudioEndpoint(eCapture, eConsole))
            .map_err(|error| anyhow!("no wasapi capture device: {error}"))
    }
}

fn friendly_name_of_wasapi_device(device: &IMMDevice) -> Option<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let value = store.GetValue(&DEVICE_FRIENDLY_NAME).ok()?;
        BSTR::try_from(&value).ok().map(|name| name.to_string())
    }
}

fn mix_format_is_ieee_float(format_ptr: *const WAVEFORMATEX) -> bool {
    unsafe {
        let format = *format_ptr;
        match format.wFormatTag {
            WAVE_FORMAT_IEEE_FLOAT => true,
            tag if tag == WAVE_FORMAT_PCM as u16 => false,
            WAVE_FORMAT_EXTENSIBLE => {
                let extensible = std::ptr::read_unaligned(format_ptr as *const WAVEFORMATEXTENSIBLE);
                let subtype = std::ptr::addr_of!(extensible.SubFormat).read_unaligned();
                subtype == SUBTYPE_IEEE_FLOAT
            }
            _ => false,
        }
    }
}

fn mix_format_can_decode(bits_per_sample: u16, is_float: bool) -> bool {
    matches!(
        (is_float, bits_per_sample),
        (true, 32) | (false, 16) | (false, 32)
    )
}

fn copy_available_wasapi_packets(
    capture_client: &IAudioCaptureClient,
    samples: &Mutex<Vec<f32>>,
    channels: u16,
    bits_per_sample: u16,
    is_float: bool,
) -> Result<bool> {
    unsafe {
        let mut copied_any = false;
        loop {
            let frames_in_next_packet = capture_client.GetNextPacketSize()?;
            if frames_in_next_packet == 0 {
                return Ok(copied_any);
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            capture_client.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            let silent = flags & WASAPI_PACKET_IS_SILENT != 0;
            append_wasapi_frames_as_f32(
                samples,
                data,
                frames,
                channels,
                bits_per_sample,
                is_float,
                silent,
            );
            capture_client.ReleaseBuffer(frames)?;
            copied_any = true;
        }
    }
}

fn append_wasapi_frames_as_f32(
    samples: &Mutex<Vec<f32>>,
    data: *const u8,
    frames: u32,
    channels: u16,
    bits_per_sample: u16,
    is_float: bool,
    silent: bool,
) {
    let sample_count = frames as usize * channels as usize;
    if sample_count == 0 {
        return;
    }
    let mut decoded = vec![0.0f32; sample_count];
    if !silent && !data.is_null() {
        if is_float && bits_per_sample == 32 {
            let slice = unsafe { std::slice::from_raw_parts(data as *const f32, sample_count) };
            decoded.copy_from_slice(slice);
        } else if !is_float && bits_per_sample == 16 {
            let slice = unsafe { std::slice::from_raw_parts(data as *const i16, sample_count) };
            for (destination, source) in decoded.iter_mut().zip(slice) {
                *destination = *source as f32 / i16::MAX as f32;
            }
        } else if !is_float && bits_per_sample == 32 {
            let slice = unsafe { std::slice::from_raw_parts(data as *const i32, sample_count) };
            for (destination, source) in decoded.iter_mut().zip(slice) {
                *destination = *source as f32 / i32::MAX as f32;
            }
        }
    }
    samples.lock().unwrap().extend_from_slice(&decoded);
}
