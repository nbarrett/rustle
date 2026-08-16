use anyhow::{anyhow, Result};
use rustle_core::mac_ax;
use rustle_core::mac_paste;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn run_probe() -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let marker = format!("RUSTLEPROBE{stamp}");
    println!("trusted={}", mac_ax::process_is_trusted());
    println!("marker={marker}");

    let (session_id, session_name) = mac_paste::open_probe_iterm_session()?;
    println!("opened session={session_id} name={session_name}");

    let write_result = mac_paste::apply_iterm_session_delta(Some(&session_id), 0, &marker, false);
    let contents = mac_paste::iterm_session_text(&session_id);
    let _ = mac_paste::close_iterm_session(&session_id);

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
