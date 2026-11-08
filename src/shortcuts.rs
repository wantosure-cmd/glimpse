//! Application-local detail shortcuts; no system-wide keyboard hook.
use eframe::egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VideoShortcut {
    Seek(f64), Volume(f32), Speed(f64), ResetSpeed, ToggleMute, Restart,
}
pub fn video_shortcut(key: Key, modifiers: Modifiers) -> Option<VideoShortcut> {
    if modifiers.alt || modifiers.shift || modifiers.mac_cmd { return None; }
    if modifiers.ctrl || modifiers.command {
        return match key {
            Key::ArrowUp => Some(VideoShortcut::Speed(0.25)),
            Key::ArrowDown => Some(VideoShortcut::Speed(-0.25)),
            _ => None,
        };
    }
    match key {
        Key::ArrowLeft => Some(VideoShortcut::Seek(-5.)),
        Key::ArrowRight => Some(VideoShortcut::Seek(5.)),
        Key::ArrowUp => Some(VideoShortcut::Volume(5.)),
        Key::ArrowDown => Some(VideoShortcut::Volume(-5.)),
        Key::M => Some(VideoShortcut::ToggleMute),
        Key::R => Some(VideoShortcut::ResetSpeed),
        Key::Home => Some(VideoShortcut::Restart),
        _ => None,
    }
}
pub fn normalized_speed(speed: f64) -> f64 {
    if speed.is_finite() { speed.clamp(0.25, 4.) } else { 1. }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailKey {
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    Left,
    Right,
    PageUp,
    PageDown,
}
impl DetailKey {
    pub const ALL: [Self; 14] = [
        Self::Numpad0,
        Self::Numpad1,
        Self::Numpad2,
        Self::Numpad3,
        Self::Numpad4,
        Self::Numpad5,
        Self::Numpad6,
        Self::Numpad7,
        Self::Numpad8,
        Self::Numpad9,
        Self::Left,
        Self::Right,
        Self::PageUp,
        Self::PageDown,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Numpad0 => crate::ui_text!(msg_f955177ad8b9),
            Self::Numpad1 => crate::ui_text!(msg_098508d17199),
            Self::Numpad2 => crate::ui_text!(msg_90fda0dd2002),
            Self::Numpad3 => crate::ui_text!(msg_194577418054),
            Self::Numpad4 => crate::ui_text!(msg_3ae46a79e5d7),
            Self::Numpad5 => crate::ui_text!(msg_2d1a354bd5a2),
            Self::Numpad6 => crate::ui_text!(msg_c3ddde21047a),
            Self::Numpad7 => crate::ui_text!(msg_264a21bf98c9),
            Self::Numpad8 => crate::ui_text!(msg_f1cb48b8c875),
            Self::Numpad9 => crate::ui_text!(msg_d762207e9c78),
            Self::Left => "←",
            Self::Right => "→",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
        }
    }
    pub fn matches(self, key: Key, numpad_down: impl Fn(u8) -> bool) -> bool {
        let (logical, digit) = match self {
            Self::Numpad0 => (Key::Num0, Some(0)),
            Self::Numpad1 => (Key::Num1, Some(1)),
            Self::Numpad2 => (Key::Num2, Some(2)),
            Self::Numpad3 => (Key::Num3, Some(3)),
            Self::Numpad4 => (Key::Num4, Some(4)),
            Self::Numpad5 => (Key::Num5, Some(5)),
            Self::Numpad6 => (Key::Num6, Some(6)),
            Self::Numpad7 => (Key::Num7, Some(7)),
            Self::Numpad8 => (Key::Num8, Some(8)),
            Self::Numpad9 => (Key::Num9, Some(9)),
            Self::Left => (Key::ArrowLeft, None),
            Self::Right => (Key::ArrowRight, None),
            Self::PageUp => (Key::PageUp, None),
            Self::PageDown => (Key::PageDown, None),
        };
        key == logical && digit.is_none_or(numpad_down)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DetailShortcuts {
    pub previous: DetailKey,
    pub next: DetailKey,
}
impl Default for DetailShortcuts {
    fn default() -> Self {
        Self {
            previous: DetailKey::Numpad1,
            next: DetailKey::Numpad0,
        }
    }
}
#[cfg(windows)]
pub fn numpad_down(digit: u8) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetKeyState(key: i32) -> i16;
    }
    digit < 10
        && unsafe { GetKeyState(0x60 + digit as i32) } < 0
        && unsafe { GetKeyState(0x30 + digit as i32) } >= 0
}
#[cfg(not(windows))]
pub fn numpad_down(_digit: u8) -> bool {
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_keys_distinguish_speed_seek_volume_and_modifiers() {
        assert_eq!(video_shortcut(Key::ArrowUp, Modifiers::CTRL), Some(VideoShortcut::Speed(0.25)));
        assert_eq!(video_shortcut(Key::ArrowDown, Modifiers::CTRL), Some(VideoShortcut::Speed(-0.25)));
        assert_eq!(video_shortcut(Key::ArrowUp, Modifiers::NONE), Some(VideoShortcut::Volume(5.)));
        assert_eq!(video_shortcut(Key::ArrowDown, Modifiers::NONE), Some(VideoShortcut::Volume(-5.)));
        assert_eq!(video_shortcut(Key::ArrowLeft, Modifiers::NONE), Some(VideoShortcut::Seek(-5.)));
        assert_eq!(video_shortcut(Key::ArrowRight, Modifiers::NONE), Some(VideoShortcut::Seek(5.)));
        assert_eq!(video_shortcut(Key::M, Modifiers::NONE), Some(VideoShortcut::ToggleMute));
        assert_eq!(video_shortcut(Key::R, Modifiers::NONE), Some(VideoShortcut::ResetSpeed));
        assert_eq!(video_shortcut(Key::Home, Modifiers::NONE), Some(VideoShortcut::Restart));
        for key in [Key::ArrowLeft, Key::ArrowRight, Key::M, Key::R, Key::Home] {
            assert_eq!(video_shortcut(key, Modifiers::CTRL), None);
        }
        assert_eq!(video_shortcut(Key::ArrowUp, Modifiers::ALT), None);
        assert_eq!(video_shortcut(Key::ArrowRight, Modifiers::SHIFT), None);
        assert_eq!(normalized_speed(-1.), 0.25);
        assert_eq!(normalized_speed(9.), 4.);
        assert_eq!(normalized_speed(f64::NAN), 1.);
    }
    #[test]
    fn distinguishes_top_row_and_roundtrips_settings() {
        let keys = DetailShortcuts::default();
        assert!(keys.next.matches(Key::Num0, |d| d == 0));
        assert!(!keys.next.matches(Key::Num0, |_| false));
        assert!(!keys.previous.matches(Key::Num0, |_| true));
        assert_eq!(
            keys,
            serde_json::from_str(&serde_json::to_string(&keys).unwrap()).unwrap()
        );
    }
}
