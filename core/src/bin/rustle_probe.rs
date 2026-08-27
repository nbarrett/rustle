#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
use anyhow::{anyhow, Result};
#[cfg(target_os = "macos")]
use rustle_core::mac_ax;
#[cfg(target_os = "macos")]
use rustle_core::mac_paste;
#[cfg(target_os = "macos")]
use std::env;
#[cfg(target_os = "macos")]
use std::process::ExitCode;
#[cfg(target_os = "macos")]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    match run_probe() {
        Ok(()) => {
            println!("PROBE PASS");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("PROBE FAIL: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "macos")]
fn run_probe() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let marker = env::var("RUSTLE_PROBE_TEXT")
        .unwrap_or_else(|_| format!("RUSTLEPROBE{stamp}"));
    let pinned = env::var("RUSTLE_PROBE_SESSION").ok();
    let keep = env::var("RUSTLE_PROBE_KEEP").is_ok();
    println!("trusted={}", mac_ax::process_is_trusted());
    println!("marker={marker}");

    let using_pinned = pinned.is_some();
    let (session_id, session_name) = if let Some(session_id) = pinned {
        (session_id, "pinned".to_string())
    } else {
        mac_paste::open_probe_iterm_session()?
    };
    println!("opened session={session_id} name={session_name}");

    let write_result = mac_paste::apply_iterm_session_delta(Some(&session_id), 0, &marker, false);
    let contents = mac_paste::iterm_session_text(&session_id);
    if !using_pinned && !keep {
        let _ = mac_paste::close_iterm_session(&session_id);
    }

    write_result?;
    let contents = contents?;
    println!("session text after insert:\n{contents}");
    if !contents.contains(&marker) {
        return Err(anyhow!(
            "marker {marker} was not in the probe session after insert"
        ));
    }
    Ok(())
}
