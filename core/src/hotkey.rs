use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug)]
pub enum HotkeyEdge {
    Press,
    Release,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HotkeyOption {
    pub value: HotkeyChoice,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotkeyChoice {
    Function,
    RightOption,
    RightControl,
    F8,
    F9,
}

impl HotkeyChoice {
    pub fn label(self) -> &'static str {
        match self {
            HotkeyChoice::Function => "fn (Globe)",
            HotkeyChoice::RightOption => {
                if cfg!(target_os = "macos") {
                    "Right Option"
                } else {
                    "Right Alt"
                }
            }
            HotkeyChoice::RightControl => "Right Control",
            HotkeyChoice::F8 => "F8",
            HotkeyChoice::F9 => "F9",
        }
    }

    pub fn preferred() -> Self {
        if cfg!(target_os = "macos") {
            HotkeyChoice::Function
        } else {
            HotkeyChoice::F8
        }
    }

    pub fn available() -> Vec<Self> {
        if cfg!(target_os = "macos") {
            Self::every_choice().to_vec()
        } else {
            vec![
                HotkeyChoice::RightOption,
                HotkeyChoice::RightControl,
                HotkeyChoice::F8,
                HotkeyChoice::F9,
            ]
        }
    }

    pub fn effective(self) -> Self {
        if cfg!(target_os = "macos") || self != HotkeyChoice::Function {
            self
        } else {
            Self::preferred()
        }
    }

    pub fn option(self) -> HotkeyOption {
        HotkeyOption {
            value: self,
            label: self.label().to_string(),
        }
    }

    pub fn every_choice() -> [HotkeyChoice; 5] {
        [
            HotkeyChoice::Function,
            HotkeyChoice::RightOption,
            HotkeyChoice::RightControl,
            HotkeyChoice::F8,
            HotkeyChoice::F9,
        ]
    }

    pub fn macos_keycode(self) -> i64 {
        match self {
            HotkeyChoice::Function => 63,
            HotkeyChoice::RightOption => 61,
            HotkeyChoice::RightControl => 62,
            HotkeyChoice::F8 => 100,
            HotkeyChoice::F9 => 101,
        }
    }

    pub fn matches_macos_keycode(self, keycode: i64) -> bool {
        match self {
            HotkeyChoice::RightOption => keycode == 61 || keycode == 58,
            HotkeyChoice::RightControl => keycode == 62 || keycode == 59,
            other => keycode == other.macos_keycode(),
        }
    }

    pub fn is_modifier(self) -> bool {
        matches!(
            self,
            HotkeyChoice::Function | HotkeyChoice::RightOption | HotkeyChoice::RightControl
        )
    }

    pub fn macos_modifier_flag(self) -> u64 {
        match self {
            HotkeyChoice::Function => 0x0080_0000,
            HotkeyChoice::RightOption => 0x0008_0000,
            HotkeyChoice::RightControl => 0x0004_0000,
            _ => 0,
        }
    }

    pub fn matches_win_vk(self, vk: u32, extended: bool) -> bool {
        match self.effective() {
            HotkeyChoice::RightControl => {
                vk == WIN_VK_RCONTROL || (vk == WIN_VK_CONTROL && extended)
            }
            HotkeyChoice::RightOption => {
                vk == WIN_VK_RMENU || (vk == WIN_VK_MENU && extended)
            }
            HotkeyChoice::F8 => vk == WIN_VK_F8 || vk == WIN_VK_MEDIA_PLAY_PAUSE,
            HotkeyChoice::F9 => vk == WIN_VK_F9 || vk == WIN_VK_MEDIA_NEXT_TRACK,
            HotkeyChoice::Function => false,
        }
    }
}

pub(crate) const WIN_VK_CONTROL: u32 = 0x11;
pub(crate) const WIN_VK_MENU: u32 = 0x12;
pub(crate) const WIN_VK_LCONTROL: u32 = 0xA2;
pub(crate) const WIN_VK_RCONTROL: u32 = 0xA3;
pub(crate) const WIN_VK_LMENU: u32 = 0xA4;
pub(crate) const WIN_VK_RMENU: u32 = 0xA5;
pub(crate) const WIN_VK_F8: u32 = 0x77;
pub(crate) const WIN_VK_F9: u32 = 0x78;
pub(crate) const WIN_VK_MEDIA_NEXT_TRACK: u32 = 0xB0;
pub(crate) const WIN_VK_MEDIA_PLAY_PAUSE: u32 = 0xB3;

#[cfg(test)]
mod tests {
    use super::{
        HotkeyChoice, WIN_VK_CONTROL, WIN_VK_F8, WIN_VK_F9, WIN_VK_LCONTROL, WIN_VK_LMENU,
        WIN_VK_MEDIA_NEXT_TRACK, WIN_VK_MEDIA_PLAY_PAUSE, WIN_VK_MENU, WIN_VK_RCONTROL,
        WIN_VK_RMENU,
    };

    #[test]
    fn right_option_accepts_either_option_key() {
        assert!(HotkeyChoice::RightOption.matches_macos_keycode(61));
        assert!(HotkeyChoice::RightOption.matches_macos_keycode(58));
        assert!(!HotkeyChoice::RightOption.matches_macos_keycode(62));
    }

    #[test]
    fn globe_is_only_offered_on_macos() {
        let available = HotkeyChoice::available();
        assert_eq!(
            available.contains(&HotkeyChoice::Function),
            cfg!(target_os = "macos")
        );
        assert!(available.contains(&HotkeyChoice::RightControl));
    }

    #[test]
    fn function_maps_to_a_usable_key_off_macos() {
        if cfg!(target_os = "macos") {
            assert_eq!(HotkeyChoice::Function.effective(), HotkeyChoice::Function);
        } else {
            assert_eq!(HotkeyChoice::Function.effective(), HotkeyChoice::F8);
        }
    }

    #[test]
    fn windows_f8_accepts_the_mac_keyboard_media_key() {
        assert!(HotkeyChoice::F8.matches_win_vk(WIN_VK_F8, false));
        assert!(HotkeyChoice::F8.matches_win_vk(WIN_VK_MEDIA_PLAY_PAUSE, false));
        assert!(!HotkeyChoice::F8.matches_win_vk(WIN_VK_F9, false));
    }

    #[test]
    fn windows_f9_accepts_media_next_track() {
        assert!(HotkeyChoice::F9.matches_win_vk(WIN_VK_F9, false));
        assert!(HotkeyChoice::F9.matches_win_vk(WIN_VK_MEDIA_NEXT_TRACK, false));
        assert!(!HotkeyChoice::F9.matches_win_vk(WIN_VK_F8, false));
    }

    #[test]
    fn windows_right_control_ignores_left_control() {
        assert!(HotkeyChoice::RightControl.matches_win_vk(WIN_VK_RCONTROL, false));
        assert!(HotkeyChoice::RightControl.matches_win_vk(WIN_VK_CONTROL, true));
        assert!(!HotkeyChoice::RightControl.matches_win_vk(WIN_VK_CONTROL, false));
        assert!(!HotkeyChoice::RightControl.matches_win_vk(WIN_VK_LCONTROL, false));
    }

    #[test]
    fn windows_right_alt_ignores_left_alt() {
        assert!(HotkeyChoice::RightOption.matches_win_vk(WIN_VK_RMENU, false));
        assert!(HotkeyChoice::RightOption.matches_win_vk(WIN_VK_MENU, true));
        assert!(!HotkeyChoice::RightOption.matches_win_vk(WIN_VK_MENU, false));
        assert!(!HotkeyChoice::RightOption.matches_win_vk(WIN_VK_LMENU, false));
    }
}
