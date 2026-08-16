use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const LIVE_TRANSCRIPTION_INTERVAL: Duration = Duration::from_millis(450);
const LIVE_TRANSCRIPT_MINIMUM_SECONDS: f32 = 0.35;
const CLIPBOARD_SETTLE: Duration = Duration::from_millis(50);
const DELETE_SETTLE: Duration = Duration::from_millis(8);

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
    NeedsPermission(String),
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

        spawn_hotkey_listener(
            shared_config.clone(),
            listening_enabled.clone(),
            report_status.clone(),
            sender,
        );

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
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
    sender: Sender<ControllerCommand>,
) {
    use crate::mac_hotkey::{run_hotkey_tap, HotkeyEdge};
    thread::spawn(move || {
        let created = run_hotkey_tap(
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
        if !created {
            write_engine_log("hotkey tap was not created; Input Monitoring is off");
            report_status(DictationStatus::NeedsPermission(
                "Input Monitoring is off. Enable Rustle there, then quit and reopen.".to_string(),
            ));
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn spawn_hotkey_listener(
    _shared_config: Arc<Mutex<Config>>,
    _listening_enabled: Arc<AtomicBool>,
    _report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
    _sender: Sender<ControllerCommand>,
) {
}

fn run_dictation_controller(
    shared_config: Arc<Mutex<Config>>,
    receiver: Receiver<ControllerCommand>,
    report_status: Arc<dyn Fn(DictationStatus) + Send + Sync>,
) {
    let mut loaded_model: Option<(String, WhisperTranscriber)> = None;
    let mut recording: Option<ActiveRecording> = None;
    let mut inserted_text = String::new();
    let mut insert_origin: Option<i64> = None;
    let mut saved_clipboard: Option<String> = None;
    let mut live_ax_insert_works = true;
    #[cfg(target_os = "macos")]
    let mut insert_target: Option<crate::mac_paste::FrontApp> = None;

    loop {
        let command = if recording.is_some() {
            match receiver.recv_timeout(LIVE_TRANSCRIPTION_INTERVAL) {
                Ok(command) => command,
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(active) = recording.as_ref() {
                        let corrections = shared_config.lock().unwrap().corrections.clone();
                        if let Some(text) =
                            transcribe_current_buffer(active, &loaded_model, &corrections)
                        {
                            if live_ax_insert_works {
                                match sync_focused_text_with_ax_only(
                                    &inserted_text,
                                    &text,
                                    &mut insert_origin,
                                ) {
                                    Ok(()) => inserted_text = text.clone(),
                                    Err(error) => {
                                        write_engine_log(&format!(
                                            "live AX insert disabled: {error}"
                                        ));
                                        live_ax_insert_works = false;
                                    }
                                }
                            }
                            if !live_ax_insert_works {
                                match insert_text_for_target(
                                    #[cfg(target_os = "macos")]
                                    insert_target.as_ref(),
                                    &inserted_text,
                                    &text,
                                    false,
                                    true,
                                ) {
                                    Ok(InsertKind::Iterm | InsertKind::SystemEvents) => {
                                        inserted_text = text.clone();
                                    }
                                    Ok(_) => {}
                                    Err(error) => write_engine_log(&format!(
                                        "live insert failed: {error}"
                                    )),
                                }
                            }
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
                            inserted_text.clear();
                            insert_origin = None;
                            live_ax_insert_works = true;
                            saved_clipboard = read_clipboard_text();
                            #[cfg(target_os = "macos")]
                            {
                                insert_target = crate::mac_paste::insert_target_app();
                                match &insert_target {
                                    Some(app) => write_engine_log(&format!(
                                        "AXIsProcessTrusted={} insert target={} bundle={} pid={} iterm={} session={} session_name={}",
                                        crate::mac_ax::process_is_trusted(),
                                        app.name,
                                        app.bundle.as_deref().unwrap_or("-"),
                                        app.pid,
                                        app.is_iterm(),
                                        app.session_id.as_deref().unwrap_or("-"),
                                        app.session_name.as_deref().unwrap_or("-")
                                    )),
                                    None => write_engine_log(&format!(
                                        "AXIsProcessTrusted={} insert target unavailable",
                                        crate::mac_ax::process_is_trusted()
                                    )),
                                }
                            }
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
                        &inserted_text,
                        &mut insert_origin,
                        #[cfg(target_os = "macos")]
                        insert_target.as_ref(),
                        report_status.as_ref(),
                    ) {
                        write_engine_log(&format!("final insert failed: {error}"));
                        report_insert_problem(report_status.as_ref(), &error);
                    }
                    restore_clipboard_text(saved_clipboard.take());
                    inserted_text.clear();
                    insert_origin = None;
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
    let minimum_samples = ((active.sample_rate() as f32 * LIVE_TRANSCRIPT_MINIMUM_SECONDS)
        * active.channels() as f32) as usize;
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
    already_inserted: &str,
    insert_origin: &mut Option<i64>,
    #[cfg(target_os = "macos")] insert_target: Option<&crate::mac_paste::FrontApp>,
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

    let insert_kind = sync_focused_text_to_transcript(
        already_inserted,
        spoken,
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
            InsertKind::SystemEvents | InsertKind::Unchanged => {
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
        let prefix = shared_prefix_char_count(previous, current);
        let addition: String = current.chars().skip(prefix).collect();
        if crate::mac_ax::process_is_trusted() {
            if let Some(app) = target {
                if let Err(error) = crate::mac_paste::activate_pid(app.pid) {
                    write_engine_log(&format!("activate failed: {error}"));
                }
                if prefix < previous.chars().count() {
                    crate::mac_paste::post_delete_keystrokes(
                        previous.chars().count() - prefix,
                    )?;
                }
                crate::mac_paste::post_unicode_to_pid(app.pid, &addition)?;
                if press_return {
                    crate::mac_paste::post_return_keystroke()?;
                }
                write_engine_log(&format!(
                    "unicode insert used pid={} chars={} return={press_return} text={:?}",
                    app.pid,
                    addition.chars().count(),
                    addition
                ));
                write_insert_receipt(app.session_id.as_deref(), &addition, press_return);
                return Ok(InsertKind::Keystroke);
            }
        }
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
        apply_system_events_text_delta(previous, current, press_return)?;
        return Ok(InsertKind::SystemEvents);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (previous, current, press_return, is_live);
        Err(anyhow!("insert needs macOS"))
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
    let message = error.to_string();
    if message.contains("AX -25211") {
        report_status(DictationStatus::NeedsPermission(
            "Accessibility is off. Use Privacy & Security → Accessibility, then quit and reopen.".to_string(),
        ));
    } else {
        report_status(DictationStatus::Failed(message));
    }
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
        match insert_text_for_target(insert_target, previous, current, false, false) {
            Ok(kind) => return Ok(kind),
            Err(error) => {
                write_engine_log(&format!("target insert failed: {error}"));
                if target_is_iterm(insert_target) {
                    return Err(error);
                }
            }
        }
    }
    let prefix = shared_prefix_char_count(previous, current);
    let delete_count = previous.chars().count().saturating_sub(prefix);
    let addition: String = current.chars().skip(prefix).collect();
    if delete_count > 0 {
        post_delete_keystrokes(delete_count)?;
        thread::sleep(DELETE_SETTLE);
    }
    if !addition.is_empty() {
        paste_text(&addition)?;
    }
    write_engine_log("keystroke insert used");
    Ok(InsertKind::Keystroke)
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

fn paste_text(text: &str) -> Result<()> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| anyhow!("clipboard unavailable: {error}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|error| anyhow!("failed to set clipboard: {error}"))?;
    thread::sleep(CLIPBOARD_SETTLE);
    #[cfg(target_os = "macos")]
    {
        match crate::mac_paste::post_system_events_command_v() {
            Ok(()) => {
                write_engine_log("system events paste used");
                return Ok(());
            }
            Err(error) => write_engine_log(&format!("system events paste failed: {error}")),
        }
    }
    post_paste_keystroke()
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
fn post_paste_keystroke() -> Result<()> {
    crate::mac_paste::post_command_v_keystroke()
}

#[cfg(target_os = "macos")]
fn post_delete_keystrokes(count: usize) -> Result<()> {
    crate::mac_paste::post_delete_keystrokes(count)
}

#[cfg(target_os = "macos")]
fn post_return_keystroke() -> Result<()> {
    crate::mac_paste::post_return_keystroke()
}

#[cfg(not(target_os = "macos"))]
fn post_paste_keystroke() -> Result<()> {
    Err(anyhow!("paste is only implemented on macOS"))
}

#[cfg(not(target_os = "macos"))]
fn post_delete_keystrokes(_count: usize) -> Result<()> {
    Err(anyhow!("delete is only implemented on macOS"))
}

#[cfg(not(target_os = "macos"))]
fn post_return_keystroke() -> Result<()> {
    Err(anyhow!("return is only implemented on macOS"))
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
