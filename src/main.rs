//! Rustle - Milestone 0 (the spike).
//!
//! Record from the default microphone until you press Enter, transcribe the
//! audio on-device with Whisper, and print the text. No hotkey, no typing into
//! other apps, no tray yet - this exists only to prove the ears and the brain
//! work end to end. Everything else builds on top of this.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Whisper expects 16 kHz mono f32 audio.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

fn main() -> Result<()> {
    let model_path =
        std::env::var("RUSTLE_MODEL").unwrap_or_else(|_| "models/ggml-base.en.bin".to_string());

    let (raw, sample_rate, channels) = record_until_enter()?;
    println!(
        "captured {} samples at {} Hz ({} channel(s))",
        raw.len(),
        sample_rate,
        channels
    );

    let mono = to_mono(&raw, channels);
    let audio = resample_linear(&mono, sample_rate, WHISPER_SAMPLE_RATE);

    println!("transcribing with {model_path} ...");
    let text = transcribe(&model_path, &audio)?;

    println!("\n--- transcript ---\n{}", text.trim());
    Ok(())
}

/// Capture microphone audio into a buffer until the user presses Enter.
fn record_until_enter() -> Result<(Vec<f32>, u32, u16)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device found"))?;
    let supported = device.default_input_config()?;

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sink = buffer.clone();
    let err_fn = |err| eprintln!("audio stream error: {err}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| sink.lock().unwrap().extend_from_slice(data),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                let mut b = sink.lock().unwrap();
                b.extend(data.iter().map(|s| *s as f32 / i16::MAX as f32));
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _| {
                let mut b = sink.lock().unwrap();
                b.extend(data.iter().map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0));
            },
            err_fn,
            None,
        )?,
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    stream.play()?;
    print!("recording... press Enter to stop. ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    drop(stream);

    let captured = buffer.lock().unwrap().clone();
    Ok((captured, sample_rate, channels))
}

/// Average interleaved channels down to a single mono track.
fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let ch = channels as usize;
    samples
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Naive linear resampler - good enough for the spike, swap for `rubato` later.
fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = ((input.len() as f64) * ratio) as usize;
    let last = input.len() - 1;
    (0..out_len)
        .map(|i| {
            let src = i as f64 / ratio;
            let idx = src.floor() as usize;
            let frac = (src - idx as f64) as f32;
            let a = input[idx.min(last)];
            let b = input[(idx + 1).min(last)];
            a + (b - a) * frac
        })
        .collect()
}

/// Load the Whisper model and transcribe the audio on-device.
fn transcribe(model_path: &str, audio: &[f32]) -> Result<String> {
    let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
        .map_err(|e| anyhow!("failed to load model at {model_path}: {e}"))?;
    let mut state = ctx.create_state()?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);

    state.full(params, audio)?;

    let segments = state.full_n_segments()?;
    let mut text = String::new();
    for i in 0..segments {
        text.push_str(&state.full_get_segment_text(i)?);
    }
    Ok(text)
}
