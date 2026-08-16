use serde::{Deserialize, Serialize};

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
            HotkeyChoice::RightOption => "Right Option",
            HotkeyChoice::RightControl => "Right Control",
            HotkeyChoice::F8 => "F8",
            HotkeyChoice::F9 => "F9",
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
}

#[cfg(test)]
mod tests {
    use super::HotkeyChoice;

    #[test]
    fn right_option_accepts_either_option_key() {
        assert!(HotkeyChoice::RightOption.matches_macos_keycode(61));
        assert!(HotkeyChoice::RightOption.matches_macos_keycode(58));
        assert!(!HotkeyChoice::RightOption.matches_macos_keycode(62));
    }
}
