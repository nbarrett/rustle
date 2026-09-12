use rustle_core::config::{default_corrections, Correction};
use rustle_core::transcript::transcript_ready_for_the_clipboard;
use wasm_bindgen::prelude::wasm_bindgen;

fn corrections_from_json(corrections_json: &str) -> Vec<Correction> {
    serde_json::from_str(corrections_json).unwrap_or_default()
}

#[wasm_bindgen]
pub fn polish_transcript_for_the_clipboard(
    raw_transcript: &str,
    corrections_json: &str,
    prefers_british_spelling: bool,
) -> Option<String> {
    transcript_ready_for_the_clipboard(
        raw_transcript,
        &corrections_from_json(corrections_json),
        prefers_british_spelling,
    )
}

#[wasm_bindgen]
pub fn starting_corrections_as_json() -> String {
    serde_json::to_string(&default_corrections()).unwrap_or_else(|_| "[]".to_string())
}
