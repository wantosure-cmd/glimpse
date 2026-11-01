//! Ordered slideshow pages and aspect-aware, bounded 2–5-image collage geometry.
use crate::{
    domain::{MediaId, MediaKind},
    layout::Rect,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub seconds: f32,
    pub group_size: usize,
    pub repeat: bool,
    pub fill: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            seconds: 3.,
            group_size: 3,
            repeat: true,
            fill: true,
        }
    }
}
impl Preferences {
    pub fn validate(&mut self) {
        if !self.seconds.is_finite() {
            self.seconds = 3.;
        }
        self.seconds = self.seconds.clamp(1., 60.);
        self.group_size = self.group_size.clamp(2, 5);
    }
}
pub struct Item {
    pub id: MediaId,
    pub kind: MediaKind,
    pub parent: PathBuf,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub ids: Vec<MediaId>,
    pub video: bool,
}

pub fn pages(items: &[Item], size: usize) -> Vec<Page> {
    let size = size.clamp(1, 5);
    let mut result = Vec::new();
    let mut at = 0;
    while at < items.len() {
        let start = at;
        at += 1;
        let video = items[start].kind == MediaKind::Video;
        if !video {
            while at < items.len()
                && at - start < size
                && items[at].kind == MediaKind::Image
                && items[at].parent == items[start].parent
            {
                at += 1;
            }
        }
        result.push(Page {
            ids: items[start..at].iter().map(|i| i.id).collect(),
            video,
        });
    }
    result
}

/// Consider every ordered row partition and its transpose (at most 32 choices).
/// Choose the geometry requiring least cover crop; all rectangles tile the canvas.
pub fn collage(aspects: &[f32], width: f32, height: f32, gap: f32) -> Vec<Rect> {
    if aspects.is_empty() || aspects.len() > 5 || width <= 0. || height <= 0. {
        return vec![];
    }
    let aspects: Vec<_> = aspects
        .iter()
        .map(|a| if a.is_finite() && *a > 0. { *a } else { 1. })
        .collect();
    let mut best = (f32::INFINITY, Vec::new());
    for transpose in [false, true] {
        let ratios: Vec<_> = aspects
            .iter()
            .map(|a| if transpose { 1. / a } else { *a })
            .collect();
        let (w, h) = if transpose {
            (height, width)
        } else {
            (width, height)
        };
        for mask in 0..(1usize << (aspects.len() - 1)) {
            let mut ends: Vec<usize> = (1..aspects.len())
                .filter(|i| mask & (1 << (i - 1)) != 0)
                .collect();
            ends.push(aspects.len());
            let g = gap.max(0.).min(w / 20.).min(h / 20.);
            let available_h = h - g * (ends.len() - 1) as f32;
            let mut start = 0;
            let natural: Vec<_> = ends
                .iter()
                .map(|end| {
                    let value = (w - g * (end - start - 1) as f32)
                        / ratios[start..*end].iter().sum::<f32>();
                    start = *end;
                    value
                })
                .collect();
            let scale = available_h / natural.iter().sum::<f32>();
            // Balance legibility as well as cover crop: otherwise a near-perfect
            // aspect fit can give one image almost the entire screen and shrink
            // its three companions into a thin strip.
            let mut row_start = 0;
            let mut areas = Vec::new();
            for (row, end) in ends.iter().enumerate() {
                areas.extend(
                    ratios[row_start..*end]
                        .iter()
                        .map(|ratio| natural[row] * natural[row] * ratio * scale),
                );
                row_start = *end;
            }
            let total: f32 = areas.iter().sum();
            let balance = areas
                .iter()
                .map(|area| (area * areas.len() as f32 / total).ln().powi(2))
                .sum::<f32>()
                / areas.len() as f32;
            let score = scale.ln().abs() + 0.12 * balance;
            if score >= best.0 {
                continue;
            }
            let mut rectangles = Vec::new();
            let mut y = 0.;
            start = 0;
            for (row, end) in ends.iter().enumerate() {
                let row_h = natural[row] * scale;
                let mut x = 0.;
                for ratio in &ratios[start..*end] {
                    let tile_w = natural[row] * ratio;
                    rectangles.push(if transpose {
                        Rect {
                            x: y,
                            y: x,
                            w: row_h,
                            h: tile_w,
                        }
                    } else {
                        Rect {
                            x,
                            y,
                            w: tile_w,
                            h: row_h,
                        }
                    });
                    x += tile_w + g;
                }
                start = *end;
                y += row_h + g;
            }
            best = (score, rectangles);
        }
    }
    best.1
}

#[derive(Default)]
pub struct Clock {
    last: Option<Instant>,
    elapsed: Duration,
    eligible: bool,
}
impl Clock {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn tick(&mut self, now: Instant, eligible: bool, seconds: f32) -> bool {
        if let Some(last) = self.last {
            if self.eligible && eligible {
                self.elapsed += now.saturating_duration_since(last);
            }
        }
        self.last = Some(now);
        self.eligible = eligible;
        eligible && self.elapsed >= Duration::from_secs_f32(seconds.clamp(1., 60.))
    }
    pub fn remaining(&self, seconds: f32) -> Duration {
        Duration::from_secs_f32(seconds.clamp(1., 60.)).saturating_sub(self.elapsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_pages_preserve_order_and_never_group_videos_or_cross_folders() {
        let items: Vec<_> = (0..10)
            .map(|id| Item {
                id,
                kind: if id == 4 {
                    MediaKind::Video
                } else {
                    MediaKind::Image
                },
                parent: if id < 7 { "a".into() } else { "b".into() },
            })
            .collect();
        for size in 2..=5 {
            let pages = pages(&items, size);
            assert_eq!(
                pages.iter().flat_map(|p| p.ids.clone()).collect::<Vec<_>>(),
                (0..10).collect::<Vec<_>>()
            );
            assert!(
                pages
                    .iter()
                    .all(|p| p.ids.len() <= size && (!p.video || p.ids == [4]))
            );
            for p in pages {
                assert!(
                    p.ids
                        .iter()
                        .all(|id| items[*id as usize].parent == items[p.ids[0] as usize].parent)
                );
            }
        }
    }
    #[test]
    fn collage_tiles_all_shapes_without_overlap_or_distortion_of_geometry() {
        for count in 2..=5 {
            for ratio in [0.3, 0.667, 1., 1.778, 4.] {
                let r = collage(&vec![ratio; count], 1600., 900., 4.);
                assert_eq!(r.len(), count);
                for (i, a) in r.iter().enumerate() {
                    assert!(
                        a.x >= 0.
                            && a.y >= 0.
                            && a.w > 0.
                            && a.h > 0.
                            && a.x + a.w <= 1600.01
                            && a.y + a.h <= 900.01
                    );
                    for b in &r[i + 1..] {
                        assert!(
                            a.x + a.w <= b.x + 0.01
                                || b.x + b.w <= a.x + 0.01
                                || a.y + a.h <= b.y + 0.01
                                || b.y + b.h <= a.y + 0.01
                        );
                    }
                }
                assert!((r.iter().map(|a| a.x + a.w).fold(0., f32::max) - 1600.).abs() < 0.01);
                assert!((r.iter().map(|a| a.y + a.h).fold(0., f32::max) - 900.).abs() < 0.01);
            }
        }
    }
    #[test]
    fn mixed_four_photos_all_remain_readable_in_a_full_screen_group() {
        let tiles = collage(&[0.75, 1.5, 2. / 3., 12. / 7.], 1100., 684., 4.);
        for tile in tiles {
            let fraction = tile.w * tile.h / (1100. * 684.);
            assert!(fraction > 0.08 && fraction < 0.5);
        }
    }
    #[test]
    fn clock_excludes_loading_and_blocked_time_and_resets_for_each_page() {
        let now = Instant::now();
        let mut c = Clock::default();
        assert!(!c.tick(now, false, 3.));
        assert!(!c.tick(now + Duration::from_secs(20), true, 3.));
        assert!(!c.tick(now + Duration::from_secs(21), true, 3.));
        assert!(!c.tick(now + Duration::from_secs(22), false, 3.));
        assert!(!c.tick(now + Duration::from_secs(60), true, 3.));
        assert!(c.tick(now + Duration::from_secs(62), true, 3.));
        c.reset();
        assert!(!c.tick(now + Duration::from_secs(70), true, 3.));
    }
}
