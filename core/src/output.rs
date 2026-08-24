#[derive(Clone, Copy, Debug)]
pub enum SilencedOutput {
    AlreadySilent,
    Muted,
    VolumeLowered { previous: f32 },
}

pub fn silence_system_output() -> Option<SilencedOutput> {
    #[cfg(target_os = "macos")]
    {
        return crate::mac_output::silence_system_output();
    }
    #[cfg(target_os = "windows")]
    {
        crate::win_output::silence_system_output()
    }
    #[cfg(target_os = "linux")]
    {
        crate::linux_output::silence_system_output()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

pub fn restore_system_output(saved: SilencedOutput) {
    #[cfg(target_os = "macos")]
    {
        crate::mac_output::restore_system_output(saved);
    }
    #[cfg(target_os = "windows")]
    {
        crate::win_output::restore_system_output(saved);
    }
    #[cfg(target_os = "linux")]
    {
        crate::linux_output::restore_system_output(saved);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = saved;
    }
}
