use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const WHISPER_SAMPLE_RATE: u32 = 16_000;

fn main() -> Result<()> {
    let model_path =
        std::env::var("RUSTLE_MODEL").unwrap_or_else(|_| "models/ggml-base.en.bin".to_string());

    let (captured_samples, sample_rate, channels) = record_microphone_until_enter()?;
    println!(
        "captured {} samples at {} Hz ({} channel(s))",
        captured_samples.len(),
        sample_rate,
        channels
    );

    let mono = downmix_to_mono(&captured_samples, channels);
    let audio = resample_linear(&mono, sample_rate, WHISPER_SAMPLE_RATE);

    println!("transcribing with {model_path} ...");
    let text = transcribe_with_whisper(&model_path, &audio)?;

    println!("\n--- transcript ---\n{}", text.trim());
    Ok(())
}

fn record_microphone_until_enter() -> Result<(Vec<f32>, u32, u16)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device found"))?;
    let supported = device.default_input_config()?;

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let recorded_samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sink = recorded_samples.clone();
    let report_stream_error = |err| eprintln!("audio stream error: {err}");

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
    print!("recording... press Enter to stop. ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    drop(stream);

    let captured = recorded_samples.lock().unwrap().clone();
    Ok((captured, sample_rate, channels))
}

fn downmix_to_mono(interleaved_samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved_samples.to_vec();
    }
    let channel_count = channels as usize;
    interleaved_samples
        .chunks(channel_count)
        .map(|frame| frame.iter().sum::<f32>() / channel_count as f32)
        .collect()
}

fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
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

fn transcribe_with_whisper(model_path: &str, audio: &[f32]) -> Result<String> {
    let context =
        WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|error| anyhow!("failed to load model at {model_path}: {error}"))?;
    let mut state = context.create_state()?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);

    state.full(params, audio)?;

    let segment_count = state.full_n_segments()?;
    let mut transcript = String::new();
    for segment_index in 0..segment_count {
        transcript.push_str(&state.full_get_segment_text(segment_index)?);
    }
    Ok(transcript)
}
