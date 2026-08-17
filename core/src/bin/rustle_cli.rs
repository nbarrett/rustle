use anyhow::Result;
use rustle_core::config::load_config;
use rustle_core::engine::{DictationEngine, DictationStatus};
use std::thread;

fn main() -> Result<()> {
    let config = load_config()?;
    println!(
        "Rustle is running. Hold the {} key to talk, release to type into the focused app. Ctrl-C to quit.",
        config.hotkey.label()
    );

    let _engine = DictationEngine::start(config, |status| match status {
        DictationStatus::Listening => println!("listening..."),
        DictationStatus::Transcribing => println!("transcribing..."),
        DictationStatus::Typed(text) => println!("typed: {text}"),
        DictationStatus::Failed(message) => eprintln!("error: {message}"),
        DictationStatus::NeedsPermission(message) => eprintln!("{message}"),
        DictationStatus::Partial(text) => println!("partial: {text}"),
        DictationStatus::SettingsPreview(text) => println!("settings: {text}"),
        DictationStatus::Idle => {}
    })?;

    loop {
        thread::park();
    }
}
