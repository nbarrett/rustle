use crate::output::SilencedOutput;
use std::process::{Command, Stdio};

pub fn silence_system_output() -> Option<SilencedOutput> {
    if stdout_contains("pactl", &["get-sink-mute", "@DEFAULT_SINK@"], "yes") {
        return Some(SilencedOutput::AlreadySilent);
    }
    if run_ok("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "1"]) {
        return Some(SilencedOutput::Muted);
    }
    if run_ok("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "1"]) {
        Some(SilencedOutput::Muted)
    } else {
        None
    }
}

pub fn restore_system_output(saved: SilencedOutput) {
    if !matches!(saved, SilencedOutput::Muted) {
        return;
    }
    if run_ok("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "0"]) {
        return;
    }
    let _ = run_ok("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "0"]);
}

fn run_ok(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn stdout_contains(program: &str, args: &[&str], needle: &str) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains(needle)
        })
}
