use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const LIVE_TRANSCRIPTION_INTERVAL: Duration = Duration::from_millis(450);
const LIVE_PREVIEW_PASS_WAIT: Duration = Duration::from_secs(3);
const LIVE_TRANSCRIPT_MINIMUM_SECONDS: f32 = 0.35;
const LIVE_PREVIEW_MINIMUM_SECONDS: f32 = 0.4;
const CLIPBOARD_SETTLE: Duration = Duration::from_millis(50);
const CLIPBOARD_RESTORE_AFTER_PASTE: Duration = Duration::from_millis(800);

use crate::audio::{
    clip_has_a_speech_peak, clip_is_quieter_than_speech, downmix_to_mono,
    message_for_a_failed_recording, recording_error_looks_like_a_closed_microphone,
    resample_linear, start_recording, stop_recording, ActiveRecording,
    WHISPER_SAMPLE_RATE,
};
use crate::config::{apply_corrections, resolve_model_path, Config, Correction};
use crate::transcribe::{
    final_pass_only_extends_the_spoken_words, final_pass_threw_away_the_spoken_words,
    transcript_is_a_whisper_blank_phrase, without_a_trailing_whisper_thank_you,
    WhisperTranscriber,
};
use crate::uk_english::apply_locale_english_spelling;

#[derive(Clone, Debug)]
pub enum DictationStatus {
    Idle,
    Listening,
    Transcribing,
    Partial(String),
    Typed(String),
    SettingsPreview(String),
    Failed(String),
    NeedsPermission(String),
}

enum ControllerCommand {
    StartRecording,
    StopRecording,
    LivePreview(String),
}

struct FinishedClip {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    heard: String,
    inserted: String,
    insert_origin: Option<i64>,
    #[cfg(target_os = "macos")]
    insert_target: Option<crate::mac_paste::FrontApp>,
    saved_clipboard: Option<String>,
    delay_clipboard_restore: bool,
    transcriber: Arc<Mutex<WhisperTranscriber>>,
}

pub struct DictationEngine {
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    commands: Sender<ControllerCommand>,
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
        let controller_commands = sender.clone();
        thread::spawn(move || {
            run_dictation_controller(
                controller_config,
                controller_commands,
                receiver,
                controller_status,
            );
        });

        spawn_hotkey_listener(
            shared_config.clone(),
            listening_enabled.clone(),
            report_status.clone(),
            sender.clone(),
        );

        Ok(Self {
            shared_config,
            listening_enabled,
            commands: sender,
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

    pub fn notify_hotkey_edge(&self, pressed: bool) {
        if pressed && !self.listening_enabled.load(Ordering::SeqCst) {
            return;
        }
        let command = if pressed {
            ControllerCommand::StartRecording
        } else {
            ControllerCommand::StopRecording
        };
        let _ = self.commands.send(command);
    }
}

#[cfg(target_os = "macos")]
fn spawn_hotkey_listener(
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
    sender: Sender<ControllerCommand>,
) {
    use crate::hotkey::HotkeyEdge;
    use crate::mac_hotkey::{
        listen_event_access_is_granted, request_listen_event_access, run_hotkey_tap,
    };
    thread::spawn(move || {
        if !listen_event_access_is_granted() {
            let granted = request_listen_event_access();
            write_engine_log(&format!(
                "Input Monitoring was off; request returned {granted}"
            ));
            if !granted && !listen_event_access_is_granted() {
                report_status(DictationStatus::NeedsPermission(
                    "Input Monitoring is off. Enable Rustle there, then quit and reopen.".to_string(),
                ));
            }
        }
        let created = run_hotkey_tap(
            shared_config,
            listening_enabled,
            Box::new(move |edge| {
                let command = match edge {
                    HotkeyEdge::Press => ControllerCommand::StartRecording,
                    HotkeyEdge::Release => ControllerCommand::StopRecording,
                };
                let _ = sender.send(command);
            }) as Box<dyn Fn(crate::hotkey::HotkeyEdge) + Send + Sync>,
        );
        if !created {
            write_engine_log("hotkey tap was not created; Input Monitoring is off");
            report_status(DictationStatus::NeedsPermission(
                "Input Monitoring is off. Enable Rustle there, then quit and reopen.".to_string(),
            ));
        }
    });
}

#[cfg(target_os = "windows")]
fn spawn_hotkey_listener(
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
    sender: Sender<ControllerCommand>,
) {
    use crate::hotkey::HotkeyEdge;
    thread::spawn(move || {
        write_engine_log("windows hotkey listener starting");
        match crate::win_hotkey::run_hotkey_listener(
            shared_config,
            listening_enabled,
            Box::new(move |edge| {
                let command = match edge {
                    HotkeyEdge::Press => ControllerCommand::StartRecording,
                    HotkeyEdge::Release => ControllerCommand::StopRecording,
                };
                let _ = sender.send(command);
            }),
        ) {
            Ok(()) => write_engine_log("windows hotkey listener ended"),
            Err(error) => {
                write_engine_log(&format!("hotkey listener failed: {error}"));
                report_status(DictationStatus::NeedsPermission(
                    "Rustle could not listen for the push-to-talk key.".to_string(),
                ));
            }
        }
    });
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn spawn_hotkey_listener(
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
    sender: Sender<ControllerCommand>,
) {
    let _ = (shared_config, listening_enabled, report_status, sender);
}

#[cfg(target_os = "linux")]
fn spawn_hotkey_listener(
    shared_config: Arc<Mutex<Config>>,
    listening_enabled: Arc<AtomicBool>,
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
    sender: Sender<ControllerCommand>,
) {
    use crate::hotkey::HotkeyEdge;
    thread::spawn(move || {
        let created = crate::rdev_hotkey::run_hotkey_listener(
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
        if let Err(error) = created {
            write_engine_log(&format!("hotkey listener failed: {error}"));
            report_status(DictationStatus::NeedsPermission(
                "Rustle could not listen for the push-to-talk key.".to_string(),
            ));
        }
    });
}

fn run_dictation_controller(
    shared_config: Arc<Mutex<Config>>,
    commands: Sender<ControllerCommand>,
    receiver: Receiver<ControllerCommand>,
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
) {
    let mut loaded_model: Option<(String, Arc<Mutex<WhisperTranscriber>>)> = None;
    let mut recording: Option<ActiveRecording> = None;
    let mut inserted_text = String::new();
    let mut heard_while_holding = String::new();
    let mut insert_origin: Option<i64> = None;
    let mut saved_clipboard: Option<String> = None;
    let mut live_ax_insert_works = true;
    #[cfg(target_os = "macos")]
    let mut insert_target: Option<crate::mac_paste::FrontApp> = None;
    let mut silenced_output: Option<crate::output::SilencedOutput> = None;
    let live_pass_in_flight = Arc::new(AtomicBool::new(false));
    let recording_active = Arc::new(AtomicBool::new(false));
    let finished_clips = spawn_clip_transcribe_worker(
        shared_config.clone(),
        report_status.clone(),
        recording_active.clone(),
    );

    loop {
        let command = if recording.is_some() {
            match receiver.recv_timeout(LIVE_TRANSCRIPTION_INTERVAL) {
                Ok(command) => command,
                Err(RecvTimeoutError::Timeout) => {
                    start_live_preview_pass(
                        recording.as_ref(),
                        &loaded_model,
                        &shared_config,
                        &live_pass_in_flight,
                        &commands,
                    );
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
                    write_engine_log("hotkey press");
                    let config = shared_config.lock().unwrap().clone();
                    if let Err(error) =
                        ensure_model_loaded(&mut loaded_model, &config.model_file_name)
                    {
                        report_status(DictationStatus::Failed(format!("{error}")));
                        continue;
                    }
                    match start_recording(config.input_device_name.as_deref()) {
                        Ok(active) => {
                            silenced_output = if config.silence_other_audio_while_holding {
                                crate::output::silence_system_output()
                            } else {
                                None
                            };
                            write_engine_log(&format!(
                                "system output silenced={silenced_output:?}"
                            ));
                            recording = Some(active);
                            recording_active.store(true, Ordering::SeqCst);
                            inserted_text.clear();
                            heard_while_holding.clear();
                            insert_origin = None;
                            live_ax_insert_works = true;
                            saved_clipboard = read_clipboard_text();
                            #[cfg(target_os = "windows")]
                            crate::win_insert::remember_front_window();
                            #[cfg(not(target_os = "macos"))]
                            write_engine_log(&format!(
                                "insert target={}",
                                crate::insert::front_app_name().unwrap_or_else(|| "-".to_string())
                            ));
                            #[cfg(target_os = "macos")]
                            {
                                insert_target = crate::mac_paste::insert_target_app();
                                match &insert_target {
                                    Some(app) => write_engine_log(&format!(
                                        "AXIsProcessTrusted={} postEvent={} listenEvent={} insert target={} bundle={} pid={} iterm={} paste={} session={} session_name={}",
                                        crate::mac_ax::process_is_trusted(),
                                        crate::mac_hotkey::post_event_access_is_granted(),
                                        crate::mac_hotkey::listen_event_access_is_granted(),
                                        app.name,
                                        app.bundle.as_deref().unwrap_or("-"),
                                        app.pid,
                                        app.is_iterm(),
                                        app.prefers_clipboard_paste(),
                                        app.session_id.as_deref().unwrap_or("-"),
                                        app.session_name.as_deref().unwrap_or("-")
                                    )),
                                    None => write_engine_log(&format!(
                                        "AXIsProcessTrusted={} postEvent={} listenEvent={} insert target unavailable",
                                        crate::mac_ax::process_is_trusted(),
                                        crate::mac_hotkey::post_event_access_is_granted(),
                                        crate::mac_hotkey::listen_event_access_is_granted()
                                    )),
                                }
                            }
                            report_status(DictationStatus::Listening);
                        }
                        Err(error) => {
                            write_engine_log(&format!("recording failed: {error}"));
                            let message = message_for_a_failed_recording(&error);
                            if recording_error_looks_like_a_closed_microphone(&error)
                                || recording_error_looks_like_a_closed_microphone(&message)
                            {
                                report_status(DictationStatus::NeedsPermission(message));
                            } else {
                                report_status(DictationStatus::Failed(message));
                            }
                        }
                    }
                }
            }
            ControllerCommand::StopRecording => {
                write_engine_log("hotkey release");
                let keep_holding = wait_for_release_then_drain_live_previews(
                    &receiver,
                    &live_pass_in_flight,
                    &mut heard_while_holding,
                    &mut inserted_text,
                    &mut insert_origin,
                    &mut live_ax_insert_works,
                    #[cfg(target_os = "macos")]
                    insert_target.as_ref(),
                    report_status.as_ref(),
                );
                if keep_holding {
                    write_engine_log("hotkey still down after a release flicker; keep recording");
                    continue;
                }
                if let Some(active) = recording.take() {
                    recording_active.store(false, Ordering::SeqCst);
                    if let Some(saved) = silenced_output.take() {
                        crate::output::restore_system_output(saved);
                    }
                    let Some((_, transcriber)) = loaded_model.clone() else {
                        report_status(DictationStatus::Failed(
                            "model was not loaded".to_string(),
                        ));
                        continue;
                    };
                    let (samples, sample_rate, channels) = stop_recording(active);
                    let delay_clipboard_restore = {
                        #[cfg(target_os = "macos")]
                        {
                            insert_target
                                .as_ref()
                                .is_some_and(|app| app.is_outlook() || app.is_whatsapp())
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            true
                        }
                    };
                    write_engine_log(&format!(
                        "queued clip samples={} rate={sample_rate} channels={channels}",
                        samples.len()
                    ));
                    let _ = finished_clips.send(FinishedClip {
                        samples,
                        sample_rate,
                        channels,
                        heard: heard_while_holding.clone(),
                        inserted: inserted_text.clone(),
                        insert_origin,
                        #[cfg(target_os = "macos")]
                        insert_target: insert_target.clone(),
                        saved_clipboard: saved_clipboard.take(),
                        delay_clipboard_restore,
                        transcriber,
                    });
                    inserted_text.clear();
                    heard_while_holding.clear();
                    insert_origin = None;
                }
            }
            ControllerCommand::LivePreview(text) => {
                if recording.is_none() {
                    continue;
                }
                apply_incoming_live_preview(
                    &text,
                    &mut heard_while_holding,
                    &mut inserted_text,
                    &mut insert_origin,
                    &mut live_ax_insert_works,
                    #[cfg(target_os = "macos")]
                    insert_target.as_ref(),
                    report_status.as_ref(),
                );
            }
        }
    }
}

fn apply_incoming_live_preview(
    text: &str,
    heard_while_holding: &mut String,
    inserted_text: &mut String,
    insert_origin: &mut Option<i64>,
    live_ax_insert_works: &mut bool,
    #[cfg(target_os = "macos")] insert_target: Option<&crate::mac_paste::FrontApp>,
    report_status: &(dyn Fn(DictationStatus) + Send + Sync),
) {
    let shown = without_a_trailing_whisper_thank_you(text);
    if !live_transcript_should_be_typed(&shown) {
        return;
    }
    if heard_while_holding.is_empty()
        || spoken_word_count(&shown) >= spoken_word_count(heard_while_holding)
    {
        *heard_while_holding = shown.clone();
        apply_live_preview(
            &shown,
            inserted_text,
            insert_origin,
            live_ax_insert_works,
            #[cfg(target_os = "macos")]
            insert_target,
            report_status,
        );
    }
}

fn wait_for_release_then_drain_live_previews(
    receiver: &Receiver<ControllerCommand>,
    live_pass_in_flight: &AtomicBool,
    heard_while_holding: &mut String,
    inserted_text: &mut String,
    insert_origin: &mut Option<i64>,
    live_ax_insert_works: &mut bool,
    #[cfg(target_os = "macos")] insert_target: Option<&crate::mac_paste::FrontApp>,
    report_status: &(dyn Fn(DictationStatus) + Send + Sync),
) -> bool {
    let started = Instant::now();
    loop {
        while let Ok(command) = receiver.try_recv() {
            match command {
                ControllerCommand::LivePreview(text) => apply_incoming_live_preview(
                    &text,
                    heard_while_holding,
                    inserted_text,
                    insert_origin,
                    live_ax_insert_works,
                    #[cfg(target_os = "macos")]
                    insert_target,
                    report_status,
                ),
                ControllerCommand::StopRecording => {}
                ControllerCommand::StartRecording => return true,
            }
        }
        if !live_pass_in_flight.load(Ordering::SeqCst)
            || started.elapsed() > LIVE_PREVIEW_PASS_WAIT
        {
            if live_pass_in_flight.load(Ordering::SeqCst) {
                write_engine_log("live preview still running; continuing to final pass");
            }
            while let Ok(command) = receiver.try_recv() {
                match command {
                    ControllerCommand::LivePreview(text) => apply_incoming_live_preview(
                        &text,
                        heard_while_holding,
                        inserted_text,
                        insert_origin,
                        live_ax_insert_works,
                        #[cfg(target_os = "macos")]
                        insert_target,
                        report_status,
                    ),
                    ControllerCommand::StopRecording => {}
                    ControllerCommand::StartRecording => return true,
                }
            }
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn live_transcript_should_be_typed(text: &str) -> bool {
    !text.trim().is_empty() && !transcript_is_a_whisper_blank_phrase(text)
}

fn spoken_word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| word.chars().any(|character| character.is_ascii_alphanumeric()))
        .count()
}

#[cfg(test)]
fn transcript_looks_unfinished(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() || transcript_looks_cut_short(trimmed) {
        return true;
    }
    !matches!(trimmed.chars().last(), Some('.' | '?' | '!'))
}

fn transcript_to_type_after_release(typed_so_far: &str, heard: &str, spoken: &str) -> String {
    let spoken_usable = !spoken.is_empty()
        && !transcript_is_a_whisper_blank_phrase(spoken)
        && !transcript_looks_cut_short(spoken);
    let heard_usable = live_transcript_should_be_typed(heard);

    if !typed_so_far.is_empty() {
        if !spoken_usable {
            return typed_so_far.to_string();
        }
        if final_pass_only_extends_the_spoken_words(typed_so_far, spoken) {
            return spoken.to_string();
        }
        if spoken_word_count(spoken) > spoken_word_count(typed_so_far)
            && !final_pass_threw_away_the_spoken_words(typed_so_far, spoken)
        {
            return spoken.to_string();
        }
        return typed_so_far.to_string();
    }

    if spoken_usable {
        if heard_usable && final_pass_threw_away_the_spoken_words(heard, spoken) {
            return heard.to_string();
        }
        return spoken.to_string();
    }
    if heard_usable {
        return heard.to_string();
    }
    spoken.to_string()
}

fn should_skip_typing(audio: &[f32], spoken: &str) -> bool {
    if spoken.trim().is_empty() {
        return true;
    }
    if transcript_is_a_whisper_blank_phrase(spoken) {
        return true;
    }
    let too_quiet = clip_is_quieter_than_speech(audio) && !clip_has_a_speech_peak(audio);
    if !too_quiet {
        return false;
    }
    !spoken.chars().any(|character| character.is_ascii_alphanumeric())
}

fn start_live_preview_pass(
    recording: Option<&ActiveRecording>,
    loaded_model: &Option<(String, Arc<Mutex<WhisperTranscriber>>)>,
    shared_config: &Arc<Mutex<Config>>,
    live_pass_in_flight: &Arc<AtomicBool>,
    commands: &Sender<ControllerCommand>,
) {
    let Some(active) = recording else {
        return;
    };
    let Some((_, transcriber)) = loaded_model.clone() else {
        return;
    };
    if live_pass_in_flight.load(Ordering::SeqCst) {
        return;
    }
    let captured = active.samples_handle().lock().unwrap().clone();
    let sample_rate = active.sample_rate();
    let channels = active.channels();
    let captured_seconds =
        captured.len() as f32 / (sample_rate as f32 * channels.max(1) as f32);
    if captured_seconds < LIVE_PREVIEW_MINIMUM_SECONDS {
        return;
    }
    if live_pass_in_flight
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let corrections = shared_config.lock().unwrap().corrections.clone();
    let in_flight = live_pass_in_flight.clone();
    let commands = commands.clone();
    thread::spawn(move || {
        let text = match transcriber.try_lock() {
            Ok(transcriber) => transcribe_captured_samples(
                &captured,
                sample_rate,
                channels,
                &transcriber,
                &corrections,
            ),
            Err(_) => None,
        };
        if let Some(text) = text {
            let _ = commands.send(ControllerCommand::LivePreview(text));
        }
        in_flight.store(false, Ordering::SeqCst);
    });
}

fn apply_live_preview(
    text: &str,
    inserted_text: &mut String,
    insert_origin: &mut Option<i64>,
    live_ax_insert_works: &mut bool,
    #[cfg(target_os = "macos")] insert_target: Option<&crate::mac_paste::FrontApp>,
    report_status: &(dyn Fn(DictationStatus) + Send + Sync),
) {
    report_status(DictationStatus::Partial(text.to_string()));
    let to_type = without_a_trailing_whisper_thank_you(text);
    if !live_transcript_should_be_typed(&to_type) {
        return;
    }
    #[cfg(target_os = "macos")]
    if insert_target.is_some_and(|app| app.is_ours()) {
        report_status(DictationStatus::SettingsPreview(to_type.clone()));
    }
    if *live_ax_insert_works
        && !target_is_iterm(
            #[cfg(target_os = "macos")]
            insert_target,
        )
        && !target_uses_typed_keys(
            #[cfg(target_os = "macos")]
            insert_target,
        )
    {
        match sync_focused_text_with_ax_only(inserted_text, &to_type, insert_origin) {
            Ok(()) => *inserted_text = to_type.clone(),
            Err(error) => {
                write_engine_log(&format!("live AX insert disabled: {error}"));
                *live_ax_insert_works = false;
            }
        }
    }
    if target_is_iterm(
        #[cfg(target_os = "macos")]
        insert_target,
    ) {
        match insert_text_for_target(
            #[cfg(target_os = "macos")]
            insert_target,
            inserted_text,
            &to_type,
            false,
            true,
        ) {
            Ok(InsertKind::Iterm | InsertKind::SystemEvents | InsertKind::Keystroke) => {
                *inserted_text = to_type;
            }
            Ok(_) => {}
            Err(error) => write_engine_log(&format!("live insert failed: {error}")),
        }
    }
}

fn transcribe_captured_samples(
    captured: &[f32],
    sample_rate: u32,
    channels: u16,
    transcriber: &WhisperTranscriber,
    corrections: &[Correction],
) -> Option<String> {
    let minimum_samples =
        ((sample_rate as f32 * LIVE_TRANSCRIPT_MINIMUM_SECONDS) * channels as f32) as usize;
    if captured.len() < minimum_samples {
        return None;
    }
    let mono = downmix_to_mono(captured, channels);
    let audio = resample_linear(&mono, sample_rate, WHISPER_SAMPLE_RATE);
    let transcript = transcriber.transcribe_the_whole_clip(&audio).ok()?;
    let regional = apply_locale_english_spelling(transcript.trim());
    let corrected = apply_corrections(&regional, corrections);
    let trimmed = corrected.trim();
    if trimmed.is_empty() || transcript_is_a_whisper_blank_phrase(trimmed) {
        return None;
    }
    let shown = without_trailing_ellipsis(trimmed);
    if shown.is_empty() {
        return None;
    }
    write_engine_log(&format!("live preview chars={}", shown.chars().count()));
    Some(shown.to_string())
}

fn without_trailing_ellipsis(text: &str) -> &str {
    text.trim_end()
        .trim_end_matches("...")
        .trim_end_matches('…')
        .trim_end()
}

fn transcript_looks_cut_short(text: &str) -> bool {
    let trimmed = text.trim_end();
    trimmed.ends_with("...") || trimmed.ends_with('…')
}

fn spawn_clip_transcribe_worker(
    shared_config: Arc<Mutex<Config>>,
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
    recording_active: Arc<AtomicBool>,
) -> Sender<FinishedClip> {
    let (sender, receiver) = mpsc::channel::<FinishedClip>();
    thread::spawn(move || {
        while let Ok(clip) = receiver.recv() {
            let mut insert_origin = clip.insert_origin;
            let typed = transcribe_and_type(
                clip.samples,
                clip.sample_rate,
                clip.channels,
                &shared_config,
                clip.transcriber,
                &clip.inserted,
                &clip.heard,
                &mut insert_origin,
                #[cfg(target_os = "macos")]
                clip.insert_target.as_ref(),
                report_status.as_ref(),
                recording_active.as_ref(),
            );
            #[cfg(target_os = "windows")]
            if !recording_active.load(Ordering::SeqCst) {
                crate::win_insert::forget_front_window();
            }
            if let Err(error) = typed {
                write_engine_log(&format!("final insert failed: {error}"));
                if !recording_active.load(Ordering::SeqCst) {
                    report_insert_problem(report_status.as_ref(), &error);
                }
            }
            if clip.delay_clipboard_restore {
                thread::spawn(move || {
                    thread::sleep(CLIPBOARD_RESTORE_AFTER_PASTE);
                    restore_clipboard_text(clip.saved_clipboard);
                });
            } else {
                restore_clipboard_text(clip.saved_clipboard);
            }
        }
    });
    sender
}

fn transcribe_and_type(
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    shared_config: &Arc<Mutex<Config>>,
    transcriber: Arc<Mutex<WhisperTranscriber>>,
    already_inserted: &str,
    heard_while_holding: &str,
    insert_origin: &mut Option<i64>,
    #[cfg(target_os = "macos")] insert_target: Option<&crate::mac_paste::FrontApp>,
    report_status: &(dyn Fn(DictationStatus) + Send + Sync),
    recording_active: &AtomicBool,
) -> Result<()> {
    write_engine_log(&format!(
        "captured samples={} rate={sample_rate} channels={channels}",
        samples.len()
    ));
    if samples.is_empty() {
        if !recording_active.load(Ordering::SeqCst) {
            report_status(DictationStatus::Failed(
                "The microphone produced no audio. If you were speaking, enable Rustle in Microphone settings.".to_string(),
            ));
        }
        return Ok(());
    }
    let mono = downmix_to_mono(&samples, channels);
    let audio = resample_linear(&mono, sample_rate, WHISPER_SAMPLE_RATE);

    let heard = without_a_trailing_whisper_thank_you(without_trailing_ellipsis(
        heard_while_holding,
    ));
    let typed_so_far = already_inserted.to_string();
    if !recording_active.load(Ordering::SeqCst) {
        report_status(DictationStatus::Transcribing);
    }

    let config = shared_config.lock().unwrap().clone();
    let transcriber = transcriber.lock().unwrap();
    let raw_transcript = transcriber.transcribe_the_whole_clip(&audio)?;
    drop(transcriber);
    let regional = apply_locale_english_spelling(raw_transcript.trim());
    let corrected = apply_corrections(&regional, &config.corrections);
    let spoken = without_a_trailing_whisper_thank_you(corrected.trim());
    write_engine_log(&format!(
        "final pass chars={} live chars={} typed chars={}",
        spoken.chars().count(),
        heard.chars().count(),
        typed_so_far.chars().count()
    ));

    if typed_so_far.is_empty()
        && heard.is_empty()
        && (should_skip_typing(&audio, &spoken) || transcript_is_a_whisper_blank_phrase(&spoken))
    {
        write_engine_log(&format!(
            "transcript discarded spoken={spoken:?} rms={} samples={}",
            crate::audio::root_mean_square_amplitude(&audio),
            audio.len()
        ));
        if !recording_active.load(Ordering::SeqCst) {
            report_status(DictationStatus::Idle);
        }
        return Ok(());
    }

    let spoken = transcript_to_type_after_release(&typed_so_far, &heard, &spoken);
    if spoken != typed_so_far && spoken != heard {
        write_engine_log(&format!("using final transcript chars={}", spoken.chars().count()));
    } else if spoken == typed_so_far && !typed_so_far.is_empty() {
        write_engine_log("kept already typed transcript");
    } else if spoken == heard && spoken != typed_so_far {
        write_engine_log("kept live transcript");
    }

    if spoken.is_empty() {
        if !recording_active.load(Ordering::SeqCst) {
            report_status(DictationStatus::Idle);
        }
        return Ok(());
    }

    let insert_kind = sync_focused_text_to_transcript(
        &typed_so_far,
        &spoken,
        insert_origin,
        #[cfg(target_os = "macos")]
        insert_target,
    )?;
    if config.press_enter_on_release {
        match insert_kind {
            InsertKind::Iterm => apply_iterm_text_delta(
                "",
                "",
                true,
                #[cfg(target_os = "macos")]
                insert_target.and_then(|app| app.session_id.as_deref()),
                #[cfg(not(target_os = "macos"))]
                None,
            )?,
            InsertKind::Unchanged if target_is_iterm(
                #[cfg(target_os = "macos")]
                insert_target,
            ) =>
            {
                apply_iterm_text_delta(
                    "",
                    "",
                    true,
                    #[cfg(target_os = "macos")]
                    insert_target.and_then(|app| app.session_id.as_deref()),
                    #[cfg(not(target_os = "macos"))]
                    None,
                )?;
            }
            InsertKind::OwnUi => {}
            InsertKind::Keystroke | InsertKind::Unchanged
                if target_skips_return(
                    #[cfg(target_os = "macos")]
                    insert_target,
                ) => {}
            InsertKind::SystemEvents => {
                if let Err(error) = apply_system_events_text_delta("", "", true) {
                    write_engine_log(&format!("System Events return failed: {error}"));
                    thread::sleep(CLIPBOARD_SETTLE);
                    post_return_keystroke()?;
                }
            }
            _ => {
                thread::sleep(CLIPBOARD_SETTLE);
                post_return_keystroke()?;
            }
        }
    }
    report_status(DictationStatus::Typed(spoken.to_string()));
    let typed_into_settings = {
        #[cfg(target_os = "macos")]
        {
            insert_target.is_some_and(|app| app.is_ours())
        }
        #[cfg(not(target_os = "macos"))]
        {
            insert_kind == InsertKind::OwnUi
        }
    };
    if typed_into_settings {
        report_status(DictationStatus::SettingsPreview(spoken.to_string()));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertKind {
    Unchanged,
    Accessibility,
    Iterm,
    SystemEvents,
    Keystroke,
    OwnUi,
}

fn target_is_iterm(#[cfg(target_os = "macos")] target: Option<&crate::mac_paste::FrontApp>) -> bool {
    #[cfg(target_os = "macos")]
    {
        target.is_some_and(|app| app.is_iterm())
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn target_skips_return(
    #[cfg(target_os = "macos")] target: Option<&crate::mac_paste::FrontApp>,
) -> bool {
    #[cfg(target_os = "macos")]
    {
        target.is_some_and(|app| app.is_outlook() || app.is_whatsapp())
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn target_uses_typed_keys(
    #[cfg(target_os = "macos")] target: Option<&crate::mac_paste::FrontApp>,
) -> bool {
    #[cfg(target_os = "macos")]
    {
        target.is_some_and(|app| app.is_outlook() || app.is_whatsapp())
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn insert_text_for_target(
    #[cfg(target_os = "macos")] target: Option<&crate::mac_paste::FrontApp>,
    previous: &str,
    current: &str,
    press_return: bool,
    is_live: bool,
) -> Result<InsertKind> {
    if current == previous && !press_return {
        return Ok(InsertKind::Unchanged);
    }
    #[cfg(target_os = "macos")]
    {
        if target.is_some_and(|app| app.is_ours()) {
            write_engine_log("insert skipped; no other app to type into");
            return Ok(InsertKind::OwnUi);
        }
        if target.is_some_and(|app| app.is_outlook() || app.is_whatsapp()) {
            if is_live {
                return Ok(InsertKind::Unchanged);
            }
            if let Some(app) = target {
                crate::mac_paste::paste_string_into_pid(app.pid, current)?;
                write_engine_log(&format!(
                    "paste insert used pid={} name={} chars={} text={:?}",
                    app.pid,
                    app.name,
                    current.chars().count(),
                    current
                ));
                write_insert_receipt(None, current, false);
            }
            return Ok(InsertKind::Keystroke);
        }
        let prefix = shared_prefix_char_count(previous, current);
        let addition: String = current.chars().skip(prefix).collect();
        if target.is_some_and(|app| app.is_iterm()) {
            if is_live && !current.starts_with(previous) {
                write_engine_log("iterm live insert skipped; transcript was revised");
                return Ok(InsertKind::Unchanged);
            }
            apply_iterm_text_delta(
                previous,
                current,
                press_return,
                target.and_then(|app| app.session_id.as_deref()),
            )?;
            return Ok(InsertKind::Iterm);
        }
        if let Some(app) = target {
            if prefix < previous.chars().count() {
                if let Err(error) = crate::mac_paste::activate_pid(app.pid) {
                    write_engine_log(&format!("activate failed: {error}"));
                }
                crate::mac_paste::post_delete_keystrokes(
                    previous.chars().count() - prefix,
                )?;
            }
            if !addition.is_empty() {
                if let Err(error) = crate::mac_paste::activate_pid(app.pid) {
                    write_engine_log(&format!("activate failed: {error}"));
                }
                crate::mac_paste::post_unicode_to_pid(app.pid, &addition)?;
            } else if let Err(error) = crate::mac_paste::activate_pid(app.pid) {
                write_engine_log(&format!("activate failed: {error}"));
            }
            if press_return {
                crate::mac_paste::post_return_keystroke()?;
            }
            write_engine_log(&format!(
                "unicode insert used pid={} name={} chars={} return={press_return} text={:?}",
                app.pid,
                app.name,
                addition.chars().count(),
                addition
            ));
            write_insert_receipt(app.session_id.as_deref(), &addition, press_return);
            return Ok(InsertKind::Keystroke);
        }
        apply_system_events_text_delta(previous, current, press_return)?;
        return Ok(InsertKind::SystemEvents);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (previous, press_return);
        if is_live {
            return Ok(InsertKind::Unchanged);
        }
        if crate::insert::front_app_is_ours() {
            write_engine_log("insert skipped; no other app to type into");
            return Ok(InsertKind::OwnUi);
        }
        crate::insert::paste_transcript(current)?;
        write_engine_log(&format!(
            "paste insert used name={} chars={} text={:?}",
            crate::insert::front_app_name().unwrap_or_else(|| "-".to_string()),
            current.chars().count(),
            current
        ));
        write_insert_receipt(None, current, false);
        Ok(InsertKind::Keystroke)
    }
}

fn apply_system_events_text_delta(
    previous: &str,
    current: &str,
    press_return: bool,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let prefix = shared_prefix_char_count(previous, current);
        let delete_count = previous.chars().count().saturating_sub(prefix);
        let addition: String = current.chars().skip(prefix).collect();
        crate::mac_paste::apply_system_events_delta(delete_count, &addition, press_return)?;
        write_engine_log(&format!(
            "system events insert used deletes={delete_count} chars={} return={press_return}",
            addition.chars().count()
        ));
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (previous, current, press_return);
        Err(anyhow!("System Events insert needs macOS"))
    }
}

fn apply_iterm_text_delta(
    previous: &str,
    current: &str,
    press_return: bool,
    session_id: Option<&str>,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let prefix = shared_prefix_char_count(previous, current);
        let delete_count = previous.chars().count().saturating_sub(prefix);
        let addition: String = current.chars().skip(prefix).collect();
        crate::mac_paste::apply_iterm_session_delta(
            session_id,
            delete_count,
            &addition,
            press_return,
        )?;
        write_engine_log(&format!(
            "iterm insert used session={} deletes={delete_count} chars={} return={press_return} text={:?}",
            session_id.unwrap_or("-"),
            addition.chars().count(),
            addition
        ));
        write_insert_receipt(session_id, &addition, press_return);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (previous, current, press_return, session_id);
        Err(anyhow!("iTerm insert needs macOS"))
    }
}

fn report_insert_problem(report_status: &(dyn Fn(DictationStatus) + Send + Sync), error: &anyhow::Error) {
    write_engine_log(&format!("insert problem: {error}"));
    report_status(DictationStatus::Failed(error.to_string()));
}

fn shared_prefix_char_count(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(first, second)| first == second)
        .count()
}

fn sync_focused_text_with_ax_only(
    previous: &str,
    current: &str,
    insert_origin: &mut Option<i64>,
) -> Result<()> {
    if current == previous {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let origin =
            crate::mac_ax::replace_in_focused_field(*insert_origin, previous, current)?;
        *insert_origin = Some(origin);
        write_engine_log(&format!("ax insert ok origin={origin}"));
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (previous, current, insert_origin);
        Err(anyhow!("live insert needs macOS"))
    }
}

fn sync_focused_text_to_transcript(
    previous: &str,
    current: &str,
    insert_origin: &mut Option<i64>,
    #[cfg(target_os = "macos")] insert_target: Option<&crate::mac_paste::FrontApp>,
) -> Result<InsertKind> {
    if current == previous {
        return Ok(InsertKind::Unchanged);
    }
    #[cfg(target_os = "macos")]
    {
        if !target_is_iterm(insert_target) && !target_uses_typed_keys(insert_target) {
            match crate::mac_ax::replace_in_focused_field(*insert_origin, previous, current) {
                Ok(origin) => {
                    *insert_origin = Some(origin);
                    write_engine_log(&format!("ax insert ok origin={origin}"));
                    return Ok(InsertKind::Accessibility);
                }
                Err(error) => {
                    write_engine_log(&format!("ax insert failed: {error}"));
                }
            }
        }
        match insert_text_for_target(insert_target, previous, current, false, false) {
            Ok(kind) => Ok(kind),
            Err(error) => {
                write_engine_log(&format!("target insert failed: {error}"));
                Err(error)
            }
        }
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = insert_origin;
        return Ok(InsertKind::OwnUi);
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
    {
        let _ = insert_origin;
        insert_text_for_target(previous, current, false, false)
    }
}

fn write_insert_receipt(session_id: Option<&str>, text: &str, press_return: bool) {
    let Ok(directory) = crate::config::rustle_directory() else {
        return;
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let body = format!(
        "stamp={stamp}\nsession={}\nreturn={press_return}\ntext={text}\n",
        session_id.unwrap_or("-")
    );
    let _ = std::fs::write(directory.join("last-insert.txt"), body);
}

fn write_engine_log(message: &str) {
    let Ok(directory) = crate::config::rustle_directory() else {
        return;
    };
    let path = directory.join("engine.log");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
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

fn read_clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut clipboard| clipboard.get_text().ok())
}

fn restore_clipboard_text(text: Option<String>) {
    let Some(text) = text else {
        return;
    };
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
    }
}

#[cfg(target_os = "macos")]
fn post_return_keystroke() -> Result<()> {
    crate::mac_paste::post_return_keystroke()
}

#[cfg(not(target_os = "macos"))]
fn post_return_keystroke() -> Result<()> {
    crate::insert::post_return_key()
}

fn ensure_model_loaded(
    loaded_model: &mut Option<(String, Arc<Mutex<WhisperTranscriber>>)>,
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
    *loaded_model = Some((
        model_file_name.to_string(),
        Arc::new(Mutex::new(transcriber)),
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        live_transcript_should_be_typed, should_skip_typing, transcript_looks_cut_short,
        transcript_looks_unfinished, transcript_to_type_after_release, without_trailing_ellipsis,
    };

    #[test]
    fn a_quiet_thank_you_is_not_typed() {
        let audio = vec![0.0; 8000];
        assert!(should_skip_typing(&audio, "Thank you."));
    }

    #[test]
    fn a_loud_thank_you_is_not_typed() {
        let audio = vec![0.2; 8000];
        assert!(should_skip_typing(&audio, "Thank you."));
        assert!(should_skip_typing(&audio, "Thanks"));
        assert!(should_skip_typing(&audio, "Thank you so much!"));
        assert!(!should_skip_typing(&audio, "Please send the invoice, thank you"));
        assert!(!live_transcript_should_be_typed("Thank you."));
        assert!(live_transcript_should_be_typed(
            "Please send the invoice, thank you"
        ));
    }

    #[test]
    fn a_quiet_real_sentence_is_typed() {
        let audio = vec![0.0; 8000];
        assert!(!should_skip_typing(&audio, "That's all."));
    }

    #[test]
    fn loud_punctuation_is_typed() {
        let audio = vec![0.2; 8000];
        assert!(!should_skip_typing(&audio, "(, )."));
    }

    #[test]
    fn a_quiet_period_is_not_typed() {
        let audio = vec![0.0; 8000];
        assert!(should_skip_typing(&audio, "."));
    }

    #[test]
    fn empty_speech_is_not_typed() {
        let audio = vec![0.2; 8000];
        assert!(should_skip_typing(&audio, "  "));
    }

    #[test]
    fn trailing_ellipsis_is_treated_as_cut_short() {
        assert!(transcript_looks_cut_short("the things that..."));
        assert_eq!(
            without_trailing_ellipsis("the things that..."),
            "the things that"
        );
        assert!(!transcript_looks_cut_short("the things that"));
    }

    #[test]
    fn a_half_sentence_is_unfinished() {
        assert!(transcript_looks_unfinished("I've never paid any"));
        assert!(transcript_looks_unfinished("Great. Can you amend the"));
        assert!(!transcript_looks_unfinished("Sometimes it stops halfway."));
        assert!(!transcript_looks_unfinished("Does it still work?"));
    }

    #[test]
    fn release_prefers_the_longer_whole_clip_over_a_cut_live_pass() {
        assert_eq!(
            transcript_to_type_after_release(
                "",
                "I've never paid any",
                "I've never paid anything like that much money."
            ),
            "I've never paid anything like that much money."
        );
        assert_eq!(
            transcript_to_type_after_release(
                "",
                "It doesn't seem quite as reliable when I hold the button down. Sometimes it stops halfway.",
                "It doesn't seem quite as reliable when I hold a button down. Sometimes it stops halfway. Try again now."
            ),
            "It doesn't seem quite as reliable when I hold a button down. Sometimes it stops halfway. Try again now."
        );
        assert_eq!(
            transcript_to_type_after_release(
                "It's not about NGX. I've never paid any",
                "It's not about NGX. I've never paid any",
                "This isn't about NGX. I've never paid anything like that much money."
            ),
            "This isn't about NGX. I've never paid anything like that much money."
        );
        assert_eq!(
            transcript_to_type_after_release(
                "",
                "Please send the invoice, thank you",
                "Please send the invoice"
            ),
            "Please send the invoice, thank you"
        );
    }
}
