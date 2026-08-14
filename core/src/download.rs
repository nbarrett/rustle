use anyhow::{anyhow, Result};
use std::io::{Read, Write};
use std::path::Path;

use crate::config::models_directory;

pub fn download_model_file(
    file_name: &str,
    download_url: &str,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<()> {
    let destination = models_directory()?.join(file_name);
    download_to_path(download_url, &destination, &mut on_progress)
}

fn download_to_path(
    download_url: &str,
    destination: &Path,
    on_progress: &mut impl FnMut(u64, Option<u64>),
) -> Result<()> {
    let response = ureq::get(download_url)
        .call()
        .map_err(|error| anyhow!("download request failed: {error}"))?;

    let total_bytes = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let partial_path = destination.with_extension("partial");
    let mut file = std::fs::File::create(&partial_path)?;
    let mut reader = response.into_reader();
    let mut buffer = [0u8; 65_536];
    let mut received_bytes: u64 = 0;

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        received_bytes += read as u64;
        on_progress(received_bytes, total_bytes);
    }

    file.sync_all()?;
    std::fs::rename(&partial_path, destination)?;
    Ok(())
}
