use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::hotkey::HotkeyChoice;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Correction {
    pub spoken: String,
    pub written: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub hotkey: HotkeyChoice,
    pub model_file_name: String,
    pub input_device_name: Option<String>,
    pub launch_at_login: bool,
    #[serde(default)]
    pub press_enter_on_release: bool,
    #[serde(default = "default_corrections")]
    pub corrections: Vec<Correction>,
}

fn default_corrections() -> Vec<Correction> {
    vec![
        Correction {
            spoken: "whisper flow".to_string(),
            written: "Wispr Flow".to_string(),
        },
        Correction {
            spoken: "russell".to_string(),
            written: "Rustle".to_string(),
        },
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: HotkeyChoice::preferred(),
            model_file_name: "ggml-base.en.bin".to_string(),
            input_device_name: None,
            launch_at_login: false,
            press_enter_on_release: false,
            corrections: default_corrections(),
        }
    }
}

pub fn apply_corrections(text: &str, corrections: &[Correction]) -> String {
    let mut result = text.to_string();
    for correction in corrections {
        if correction.spoken.is_empty() {
            continue;
        }
        result = replace_ascii_case_insensitive(&result, &correction.spoken, &correction.written);
    }
    result
}

fn replace_ascii_case_insensitive(haystack: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return haystack.to_string();
    }
    let lower_haystack = haystack.to_ascii_lowercase();
    let lower_from = from.to_ascii_lowercase();
    let bytes = lower_haystack.as_bytes();
    let mut result = String::with_capacity(haystack.len());
    let mut index = 0;
    while let Some(found) = lower_haystack[index..].find(&lower_from) {
        let start = index + found;
        let end = start + from.len();
        let preceded_by_word = start > 0 && bytes[start - 1].is_ascii_alphanumeric();
        let followed_by_word = end < bytes.len() && bytes[end].is_ascii_alphanumeric();
        if preceded_by_word || followed_by_word {
            result.push_str(&haystack[index..end]);
        } else {
            result.push_str(&haystack[index..start]);
            result.push_str(to);
        }
        index = end;
    }
    result.push_str(&haystack[index..]);
    result
}

pub fn rustle_directory() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow!("could not locate a config directory"))?;
    Ok(base.join("rustle"))
}

pub fn config_file_path() -> Result<PathBuf> {
    Ok(rustle_directory()?.join("config.json"))
}

pub fn models_directory() -> Result<PathBuf> {
    Ok(rustle_directory()?.join("models"))
}

pub fn load_config() -> Result<Config> {
    let path = config_file_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn save_config(config: &Config) -> Result<()> {
    let directory = rustle_directory()?;
    std::fs::create_dir_all(&directory)?;
    let text = serde_json::to_string_pretty(config)?;
    std::fs::write(config_file_path()?, text)?;
    Ok(())
}

pub fn resolve_model_path(model_file_name: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(model_file_name);
    if candidate.is_absolute() {
        return Ok(candidate);
    }
    let in_data_directory = models_directory()?.join(model_file_name);
    if in_data_directory.exists() {
        return Ok(in_data_directory);
    }
    let in_working_directory = PathBuf::from("models").join(model_file_name);
    if in_working_directory.exists() {
        return Ok(in_working_directory);
    }
    Ok(in_data_directory)
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelChoice {
    pub label: String,
    pub file_name: String,
    pub approximate_download: String,
    pub download_url: String,
    pub installed: bool,
}

pub fn model_catalog() -> Vec<ModelChoice> {
    ["base.en", "small.en", "medium.en", "large-v3"]
        .into_iter()
        .map(|name| {
            let file_name = format!("ggml-{name}.bin");
            let installed = resolve_model_path(&file_name)
                .map(|path| path.exists())
                .unwrap_or(false);
            ModelChoice {
                label: name.to_string(),
                approximate_download: approximate_download_size(name).to_string(),
                download_url: format!(
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{file_name}"
                ),
                file_name,
                installed,
            }
        })
        .collect()
}

fn approximate_download_size(model_name: &str) -> &'static str {
    match model_name {
        "base.en" => "~150 MB",
        "small.en" => "~500 MB",
        "medium.en" => "~1.5 GB",
        "large-v3" => "~3 GB",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_corrections, Correction};

    fn rule(spoken: &str, written: &str) -> Correction {
        Correction {
            spoken: spoken.to_string(),
            written: written.to_string(),
        }
    }

    #[test]
    fn replaces_whole_word_get_with_git() {
        let rules = [rule("Get", "Git")];
        assert_eq!(
            apply_corrections("I'm just trying to get it to work.", &rules),
            "I'm just trying to Git it to work."
        );
    }

    #[test]
    fn does_not_replace_get_inside_getting() {
        let rules = [rule("Get", "Git")];
        assert_eq!(apply_corrections("getting started", &rules), "getting started");
    }

    #[test]
    fn replaces_multi_word_spoken_form() {
        let rules = [rule("whisper flow", "Wispr Flow")];
        assert_eq!(
            apply_corrections("I no longer need whisper flow", &rules),
            "I no longer need Wispr Flow"
        );
    }

    #[test]
    fn skips_empty_spoken_form() {
        let rules = [rule("", "Nope")];
        assert_eq!(apply_corrections("leave this", &rules), "leave this");
    }
}
