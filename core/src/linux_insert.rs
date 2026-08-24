use anyhow::{anyhow, Result};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn paste_transcript(text: &str) -> Result<()> {
    arboard::Clipboard::new()?
        .set_text(text.to_string())
        .map_err(|error| anyhow!("{error}"))?;
    thread::sleep(Duration::from_millis(40));
    if run_ok("xdotool", &["key", "--clearmodifiers", "ctrl+v"]) {
        return Ok(());
    }
    if run_ok("wtype", &["-M", "ctrl", "v", "-m", "ctrl"]) {
        return Ok(());
    }
    if run_ok("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"]) {
        Ok(())
    } else {
        Err(anyhow!(
            "could not paste; install xdotool, wtype, or ydotool"
        ))
    }
}

pub fn post_return_key() -> Result<()> {
    if run_ok("xdotool", &["key", "Return"]) {
        return Ok(());
    }
    if run_ok("wtype", &["-k", "Return"]) {
        return Ok(());
    }
    if run_ok("ydotool", &["key", "28:1", "28:0"]) {
        Ok(())
    } else {
        Err(anyhow!(
            "could not press Return; install xdotool, wtype, or ydotool"
        ))
    }
}

pub fn front_app_is_ours() -> bool {
    front_pid() == Some(std::process::id())
}

pub fn front_app_name() -> Option<String> {
    stdout_trim("xdotool", &["getactivewindow", "getwindowname"])
}

fn front_pid() -> Option<u32> {
    stdout_trim("xdotool", &["getactivewindow", "getwindowpid"])?
        .parse()
        .ok()
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

fn stdout_trim(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
