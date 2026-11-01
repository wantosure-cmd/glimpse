//! Persisted plain-text subtitle style. Values use mpv's 720p-scaled units.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubtitleStyle {
    pub visible: bool,
    pub size: f32,
    pub position: f32,
    pub color: [u8; 3],
    pub outline: f32,
    pub background: bool,
}
impl Default for SubtitleStyle {
    fn default() -> Self {
        Self::preset(0)
    }
}
impl SubtitleStyle {
    pub fn preset(index: usize) -> Self {
        Self {
            visible: true,
            size: 36.,
            position: 94.,
            color: if index == 1 {
                [255, 222, 128]
            } else {
                [255, 255, 255]
            },
            outline: if index == 2 { 0. } else { 2. },
            background: index == 2,
        }
    }
    pub fn normalize(&mut self) {
        self.size = if self.size.is_finite() {
            self.size.clamp(16., 80.)
        } else {
            36.
        };
        self.position = if self.position.is_finite() {
            self.position.clamp(0., 100.)
        } else {
            94.
        };
        self.outline = if self.outline.is_finite() {
            self.outline.clamp(0., 5.)
        } else {
            2.
        };
    }
    pub fn options(&self) -> Vec<(&'static str, String)> {
        let mut s = self.clone();
        s.normalize();
        vec![
            (
                "sub-visibility",
                if s.visible { "yes" } else { "no" }.into(),
            ),
            ("sub-font", "sans-serif".into()),
            ("sub-font-size", s.size.to_string()),
            ("sub-pos", s.position.to_string()),
            (
                "sub-color",
                format!("#{:02X}{:02X}{:02X}", s.color[0], s.color[1], s.color[2]),
            ),
            ("sub-border-size", s.outline.to_string()),
            (
                "sub-border-style",
                if s.background {
                    "background-box"
                } else {
                    "outline-and-shadow"
                }
                .into(),
            ),
            ("sub-border-color", "#000000".into()),
            (
                "sub-shadow-offset",
                if s.background { "4" } else { "1" }.into(),
            ),
            (
                "sub-back-color",
                if s.background {
                    "#99000000"
                } else {
                    "#80000000"
                }
                .into(),
            ),
            ("sub-ass-override", "force".into()),
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_config_and_invalid_ranges_keep_readable_subtitles() {
        let mut s: SubtitleStyle = serde_json::from_str(r#"{"size":999,"position":-10}"#).unwrap();
        s.normalize();
        assert_eq!((s.size, s.position), (80., 0.));
        assert!(s.visible);
        assert_eq!(
            serde_json::from_str::<SubtitleStyle>("{}").unwrap(),
            SubtitleStyle::default()
        );
        assert_ne!(
            SubtitleStyle::preset(0).options(),
            SubtitleStyle::preset(1).options()
        );
        assert!(
            SubtitleStyle::preset(2)
                .options()
                .contains(&("sub-back-color", "#99000000".into()))
        );
    }
}
