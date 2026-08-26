use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;
const SPEECH_RMS_MINIMUM: f32 = 0.012;
const SPEECH_PEAK_MINIMUM: f32 = 0.04;
const QUIET_EDGE_PAD_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize / 5;

pub struct ActiveRecording {
    _keep_capturing: CaptureSession,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

enum CaptureSession {
    Cpal(#[allow(dead_code)] cpal::Stream),
    #[cfg(windows)]
    Wasapi(crate::win_capture::WasapiCapture),
}

pub fn list_input_device_names() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            names.push(name);
        }
    }
    Ok(names)
}

fn select_input_device(preferred_device_name: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if let Some(wanted) = preferred_device_name {
        for device in host.input_devices()? {
            if device.name().map(|name| name == wanted).unwrap_or(false) {
                return Ok(device);
            }
        }
    }
    host.default_input_device()
        .ok_or_else(|| anyhow!("no default input device found"))
}

pub fn start_recording(preferred_device_name: Option<&str>) -> Result<ActiveRecording> {
    #[cfg(windows)]
    {
        match crate::win_capture::start_wasapi_capture(preferred_device_name) {
            Ok((keep_capturing, samples, sample_rate, channels)) => {
                crate::win_capture::write_capture_log(&format!(
                    "wasapi recording rate={sample_rate} channels={channels}"
                ));
                return Ok(ActiveRecording {
                    _keep_capturing: CaptureSession::Wasapi(keep_capturing),
                    samples,
                    sample_rate,
                    channels,
                });
            }
            Err(error) => crate::win_capture::write_capture_log(&format!(
                "wasapi capture failed; using cpal: {error}"
            )),
        }
    }
    start_cpal_recording(preferred_device_name)
}

fn start_cpal_recording(preferred_device_name: Option<&str>) -> Result<ActiveRecording> {
    let device = select_input_device(preferred_device_name)?;
    let supported = device.default_input_config()?;

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sink = samples.clone();
    let report_stream_error = |error| {
        let message = format!("audio stream error: {error}");
        eprintln!("{message}");
        write_audio_log(&message);
    };

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| sink.lock().unwrap().extend_from_slice(data),
            report_stream_error,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                sink.lock()
                    .unwrap()
                    .extend(data.iter().map(|sample| *sample as f32 / i16::MAX as f32));
            },
            report_stream_error,
            None,
        )?,
        cpal::SampleFormat::I32 => device.build_input_stream(
            &config,
            move |data: &[i32], _| {
                sink.lock()
                    .unwrap()
                    .extend(data.iter().map(|sample| *sample as f32 / i32::MAX as f32));
            },
            report_stream_error,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _| {
                sink.lock().unwrap().extend(
                    data.iter()
                        .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0),
                );
            },
            report_stream_error,
            None,
        )?,
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    stream.play()?;
    write_audio_log(&format!(
        "cpal recording rate={sample_rate} channels={channels} format={sample_format:?}"
    ));
    Ok(ActiveRecording {
        _keep_capturing: CaptureSession::Cpal(stream),
        samples,
        sample_rate,
        channels,
    })
}

fn write_audio_log(message: &str) {
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

impl ActiveRecording {
    pub fn samples_handle(&self) -> Arc<Mutex<Vec<f32>>> {
        self.samples.clone()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
}

pub fn stop_recording(active: ActiveRecording) -> (Vec<f32>, u32, u16) {
    let ActiveRecording {
        _keep_capturing,
        samples,
        sample_rate,
        channels,
    } = active;
    drop(_keep_capturing);
    let captured = samples.lock().unwrap().clone();
    (captured, sample_rate, channels)
}

pub fn root_mean_square_amplitude(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_of_squares: f32 = samples.iter().map(|sample| sample * sample).sum();
    (sum_of_squares / samples.len() as f32).sqrt()
}

pub fn clip_is_quieter_than_speech(samples: &[f32]) -> bool {
    root_mean_square_amplitude(samples) < SPEECH_RMS_MINIMUM
}

pub fn clip_has_a_speech_peak(samples: &[f32]) -> bool {
    samples
        .iter()
        .any(|sample| sample.abs() >= SPEECH_PEAK_MINIMUM)
}

pub fn trim_quiet_edges(samples: &[f32]) -> &[f32] {
    let Some(first) = samples
        .iter()
        .position(|sample| sample.abs() >= SPEECH_PEAK_MINIMUM)
    else {
        return samples;
    };
    let Some(last) = samples
        .iter()
        .rposition(|sample| sample.abs() >= SPEECH_PEAK_MINIMUM)
    else {
        return samples;
    };
    let start = first.saturating_sub(QUIET_EDGE_PAD_SAMPLES);
    let end = (last + 1).saturating_add(QUIET_EDGE_PAD_SAMPLES).min(samples.len());
    &samples[start..end]
}

pub fn downmix_to_mono(interleaved_samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved_samples.to_vec();
    }
    let channel_count = channels as usize;
    interleaved_samples
        .chunks(channel_count)
        .map(|frame| frame.iter().sum::<f32>() / channel_count as f32)
        .collect()
}

pub fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to_rate as f64 / from_rate as f64;
    let output_length = ((input.len() as f64) * ratio) as usize;
    let last_index = input.len() - 1;
    (0..output_length)
        .map(|output_index| {
            let source_position = output_index as f64 / ratio;
            let lower_index = source_position.floor() as usize;
            let fraction = (source_position - lower_index as f64) as f32;
            let lower_sample = input[lower_index.min(last_index)];
            let upper_sample = input[(lower_index + 1).min(last_index)];
            lower_sample + (upper_sample - lower_sample) * fraction
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        clip_has_a_speech_peak, clip_is_quieter_than_speech, root_mean_square_amplitude,
        trim_quiet_edges,
    };

    #[test]
    fn silence_is_quieter_than_speech() {
        let silence = [0.0f32; 1600];
        assert!(clip_is_quieter_than_speech(&silence));
        assert!(!clip_has_a_speech_peak(&silence));
        assert_eq!(root_mean_square_amplitude(&silence), 0.0);
    }

    #[test]
    fn a_short_word_still_has_a_speech_peak() {
        let mut clip = vec![0.0f32; 16000];
        for sample in &mut clip[8000..8600] {
            *sample = 0.2;
        }
        assert!(clip_has_a_speech_peak(&clip));
    }

    #[test]
    fn trim_quiet_edges_keeps_the_last_spoken_peak() {
        let mut clip = vec![0.0f32; 16000];
        for sample in &mut clip[2000..2600] {
            *sample = 0.2;
        }
        let trimmed = trim_quiet_edges(&clip);
        assert!(trimmed.len() < clip.len());
        assert!(trimmed.iter().any(|sample| *sample == 0.2));
        assert_eq!(trimmed[trimmed.len() - 1], 0.0);
    }
}
