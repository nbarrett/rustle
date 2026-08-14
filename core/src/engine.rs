use anyhow::{anyhow, Result};
use enigo::{Enigo, Keyboard, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const PARTIAL_TRANSCRIPTION_INTERVAL: Duration = Duration::from_millis(600);

use crate::audio::{
    downmix_to_mono, resample_linear, start_recording, stop_recording, ActiveRecording,
    WHISPER_SAMPLE_RATE,
};
use crate::config::{apply_corrections, resolve_model_path, Config, Correction};
use crate::transcribe::WhisperTranscriber;

#[derive(Clone, Debug)]
pub enum DictationStatus {
    Idle,
    Listening,
    Transcribing,
    Partial(String),
    Typed(String),
    Failed(String),
}

enum ControllerCommand {
    StartRecording,
    StopRecording,
}

pub struct DictationEngine {
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
}

impl DictationEngine {
    pub fn start(
        config: Config,
        on_status: impl Fn(DictationStatus) + Send + Sync + 'static,
    ) -> Result<Self> {
        let shared_config = Arc::new(Mutex::new(config));
        let listening_enabled = Arc::new(AtomicBool::new(true));
        let (sender, receiver) = mpsc::channel::<ControllerCommand>();
        let report_status: Arc<dyn Fn(DictationStatus) + Send + Sync> = Arc::new(on_status);

        let controller_config = shared_config.clone();
        let controller_status = report_status.clone();
        thread::spawn(move || {
            run_dictation_controller(controller_config, receiver, controller_status);
        });

        spawn_hotkey_listener(shared_config.clone(), listening_enabled.clone(), sender);

        Ok(Self {
            shared_config,
            listening_enabled,
        })
    }

    pub fn apply_config(&self, config: Config) {
        *self.shared_config.lock().unwrap() = config;
    }

    pub fn current_config(&self) -> Config {
        self.shared_config.lock().unwrap().clone()
    }

    pub fn set_listening_enabled(&self, enabled: bool) {
        self.listening_enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn is_listening_enabled(&self) -> bool {
        self.listening_enabled.load(Ordering::SeqCst)
    }
}

#[cfg(target_os = "macos")]
fn spawn_hotkey_listener(
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    sender: Sender<ControllerCommand>,
) {
    use crate::mac_hotkey::{run_hotkey_tap, HotkeyEdge};
    thread::spawn(move || {
        run_hotkey_tap(
            shared_config,
            listening_enabled,
            Box::new(move |edge| {
                let command = match edge {
                    HotkeyEdge::Press => ControllerCommand::StartRecording,
                    HotkeyEdge::Release => ControllerCommand::StopRecording,
                };
                let _ = sender.send(command);
            }),
        );
    });
}

#[cfg(not(target_os = "macos"))]
fn spawn_hotkey_listener(
    _shared_config: Arc<Mutex<Config>>,
    _listening_enabled: Arc<AtomicBool>,
    _sender: Sender<ControllerCommand>,
) {
}

fn run_dictation_controller(
    shared_config: Arc<Mutex<Config>>,
    receiver: Receiver<ControllerCommand>,
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
) {
    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(enigo) => enigo,
        Err(error) => {
            report_status(DictationStatus::Failed(format!(
                "keyboard output unavailable: {error}"
            )));
            return;
        }
    };

    let mut loaded_model: Option<(String, WhisperTranscriber)> = None;
    let mut recording: Option<ActiveRecording> = None;

    loop {
        let command = if recording.is_some() {
            match receiver.recv_timeout(PARTIAL_TRANSCRIPTION_INTERVAL) {
                Ok(command) => command,
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(active) = recording.as_ref() {
                        let corrections = shared_config.lock().unwrap().corrections.clone();
                        if let Some(text) =
                            transcribe_current_buffer(active, &loaded_model, &corrections)
                        {
                            report_status(DictationStatus::Partial(text));
                        }
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match receiver.recv() {
                Ok(command) => command,
                Err(_) => break,
            }
        };

        match command {
            ControllerCommand::StartRecording => {
                if recording.is_none() {
                    let config = shared_config.lock().unwrap().clone();
                    if let Err(error) =
                        ensure_model_loaded(&mut loaded_model, &config.model_file_name)
                    {
                        report_status(DictationStatus::Failed(format!("{error}")));
                        continue;
                    }
                    match start_recording(config.input_device_name.as_deref()) {
                        Ok(active) => {
                            recording = Some(active);
                            report_status(DictationStatus::Listening);
                        }
                        Err(error) => report_status(DictationStatus::Failed(format!(
                            "recording failed: {error}"
                        ))),
                    }
                }
            }
            ControllerCommand::StopRecording => {
                if let Some(active) = recording.take() {
                    if let Err(error) = transcribe_and_type(
                        active,
                        &shared_config,
                        &mut loaded_model,
                        &mut enigo,
                        report_status.as_ref(),
                    ) {
                        report_status(DictationStatus::Failed(format!("{error}")));
                    }
                }
            }
        }
    }
}

fn transcribe_current_buffer(
    active: &ActiveRecording,
    loaded_model: &Option<(String, WhisperTranscriber)>,
    corrections: &[Correction],
) -> Option<String> {
    let transcriber = &loaded_model.as_ref()?.1;
    let captured = active.samples_handle().lock().unwrap().clone();
    let minimum_samples = (active.sample_rate() as usize * active.channels() as usize) / 2;
    if captured.len() < minimum_samples {
        return None;
    }
    let mono = downmix_to_mono(&captured, active.channels());
    let audio = resample_linear(&mono, active.sample_rate(), WHISPER_SAMPLE_RATE);
    let transcript = transcriber.transcribe(&audio).ok()?;
    let corrected = apply_corrections(transcript.trim(), corrections);
    let trimmed = corrected.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn transcribe_and_type(
    active: ActiveRecording,
    shared_config: &Arc<Mutex<Config>>,
    loaded_model: &mut Option<(String, WhisperTranscriber)>,
    enigo: &mut Enigo,
    report_status: &(dyn Fn(DictationStatus) + Send + Sync),
) -> Result<()> {
    let (samples, sample_rate, channels) = stop_recording(active);
    let mono = downmix_to_mono(&samples, channels);
    let audio = resample_linear(&mono, sample_rate, WHISPER_SAMPLE_RATE);

    report_status(DictationStatus::Transcribing);

    let config = shared_config.lock().unwrap().clone();
    ensure_model_loaded(loaded_model, &config.model_file_name)?;
    let transcriber = &loaded_model
        .as_ref()
        .ok_or_else(|| anyhow!("model was not loaded"))?
        .1;
    let raw_transcript = transcriber.transcribe(&audio)?;
    let corrected = apply_corrections(raw_transcript.trim(), &config.corrections);
    let spoken = corrected.trim();

    if spoken.is_empty() {
        report_status(DictationStatus::Idle);
        return Ok(());
    }

    enigo
        .text(spoken)
        .map_err(|error| anyhow!("failed to type transcript: {error}"))?;
    report_status(DictationStatus::Typed(spoken.to_string()));
    Ok(())
}

fn ensure_model_loaded(
    loaded_model: &mut Option<(String, WhisperTranscriber)>,
    model_file_name: &str,
) -> Result<()> {
    let already_loaded = loaded_model
        .as_ref()
        .map(|(name, _)| name == model_file_name)
        .unwrap_or(false);
    if already_loaded {
        return Ok(());
    }
    let path = resolve_model_path(model_file_name)?;
    let path_text = path.to_string_lossy().to_string();
    let transcriber = WhisperTranscriber::load_from_path(&path_text)?;
    *loaded_model = Some((model_file_name.to_string(), transcriber));
    Ok(())
}
