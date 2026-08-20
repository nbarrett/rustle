pub mod config;
pub mod download;
pub mod hotkey;

#[cfg(feature = "runtime")]
pub mod audio;
#[cfg(feature = "runtime")]
pub mod engine;
#[cfg(feature = "runtime")]
pub mod transcribe;
pub mod uk_english;

#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod mac_ax;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod mac_hotkey;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod mac_output;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod mac_paste;
