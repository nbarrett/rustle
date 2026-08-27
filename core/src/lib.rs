pub mod config;
pub mod download;
pub mod hotkey;
pub mod install_location;

#[cfg(feature = "runtime")]
pub mod audio;
#[cfg(feature = "runtime")]
pub mod engine;
#[cfg(feature = "runtime")]
pub mod insert;
#[cfg(feature = "runtime")]
pub mod output;
#[cfg(feature = "runtime")]
pub mod transcribe;
pub mod uk_english;

#[cfg(target_os = "macos")]
pub mod mac_mic;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod mac_ax;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod mac_hotkey;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod mac_output;
#[cfg(all(feature = "runtime", target_os = "macos"))]
pub mod mac_paste;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod rdev_hotkey;
#[cfg(all(feature = "runtime", target_os = "windows"))]
pub mod win_hotkey;
#[cfg(all(feature = "runtime", target_os = "windows"))]
pub mod win_insert;
#[cfg(all(feature = "runtime", target_os = "windows"))]
pub mod win_output;
#[cfg(all(feature = "runtime", target_os = "windows"))]
pub mod win_capture;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod linux_insert;
#[cfg(all(feature = "runtime", target_os = "linux"))]
pub mod linux_output;
