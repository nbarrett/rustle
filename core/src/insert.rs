use anyhow::{anyhow, Result};

pub fn paste_transcript(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        return crate::win_insert::paste_transcript(text);
    }
    #[cfg(target_os = "linux")]
    {
        return crate::linux_insert::paste_transcript(text);
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = text;
        Err(anyhow!("paste is not implemented on this OS"))
    }
}

pub fn post_return_key() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        return crate::win_insert::post_return_key();
    }
    #[cfg(target_os = "linux")]
    {
        return crate::linux_insert::post_return_key();
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Err(anyhow!("return is not implemented on this OS"))
    }
}

pub fn front_app_is_ours() -> bool {
    #[cfg(target_os = "windows")]
    {
        return crate::win_insert::front_app_is_ours();
    }
    #[cfg(target_os = "linux")]
    {
        return crate::linux_insert::front_app_is_ours();
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

pub fn front_app_name() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        return crate::win_insert::front_app_name();
    }
    #[cfg(target_os = "linux")]
    {
        return crate::linux_insert::front_app_name();
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}
