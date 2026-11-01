//! Variable-height masonry. Geometry is computed off-thread and queried by column.
use crate::{
    domain::*,
    view::{MediaView, ScrollAnchor},
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }
}
#[derive(Debug, Clone)]
pub struct Card {
    pub id: MediaId,
    pub rect: Rect,
}
#[derive(Debug, Clone)]
pub struct Header {
    pub directory: PathBuf,
    pub title: String,
    pub counts: Counts,
    pub direct_folders: u64,
    pub kind: Option<MediaKind>,
    pub rect: Rect,
    pub collapsed: bool,
}
pub const MAX_PREVIEW_WIDTH: f32 = 1600.;
pub fn tree_indent(depth: usize, width: f32) -> f32 {
    (depth as f32 * 8.).min(40.).min(width * 0.12)
}
#[derive(Debug, Clone)]
pub struct SizingRow {
    pub rect: Rect,
    // Earliest possible next card; the label stays below the tallest column.
    pub probe_top: f32,
    pub items: Vec<MediaId>,
}
#[derive(Debug, Clone, Default)]
pub struct MasonryLayout {
    pub columns: Vec<Vec<Card>>,
    pub headers: Vec<Header>,
    pub rectangles: HashMap<MediaId, Rect>,
    pub height: f32,
    pub column_width: f32,
    pub tree_guides: Vec<Rect>,
    pub sizing_rows: Vec<SizingRow>,
    // Keep already allocated video frames stable while async dimensions arrive.
    // This is layout-only: source metadata and fitted playback remain accurate.
    video_geometry: HashMap<MediaId, (Fingerprint, (u32, u32))>,
    geometry_scope: Option<(PathBuf, [f32; 3])>,
}

impl MasonryLayout {
    pub fn build(
        snapshot: &LibrarySnapshot,
        view: &MediaView,
        width: f32,
        target_width: f32,
        collapsed: &HashSet<PathBuf>,
    ) -> Self {
        Self::build_with_gap(snapshot, view, width, target_width, collapsed, 8.)
    }
    pub fn build_with_gap(
        snapshot: &LibrarySnapshot,
        view: &MediaView,
        width: f32,
        target_width: f32,
        collapsed: &HashSet<PathBuf>,
        gap: f32,
    ) -> Self {
        Self::build_stable(snapshot, view, width, target_width, collapsed, gap, None)
    }
    pub fn build_stable(
        snapshot: &LibrarySnapshot,
        view: &MediaView,
        width: f32,
        target_width: f32,
        collapsed: &HashSet<PathBuf>,
        gap: f32,
        previous: Option<&Self>,
    ) -> Self {
        let scope = (snapshot.root.clone(), [width, target_width, gap]);
        let previous = previous.filter(|p| p.geometry_scope.as_ref() == Some(&scope));
        let margin = 6f32;
        let available = (width - margin * 2.).max(40.);
        let columns = if target_width >= MAX_PREVIEW_WIDTH {
            1
        } else {
            ((available + gap) / (target_width.clamp(160., MAX_PREVIEW_WIDTH) + gap))
                .round()
                .max(1.) as usize
        };
        let column_width = (available - gap * (columns - 1) as f32) / columns as f32;
        let mut result = Self {
            columns: vec![Vec::new(); columns],
            column_width,
            geometry_scope: Some(scope),
            ..Default::default()
        };
        let dimensions: HashMap<_, _> = snapshot
            .files
            .iter()
            .map(|m| {
                (
                    m.id,
                    (
                        previous
                            .and_then(|p| p.video_geometry.get(&m.id))
                            .filter(|(fingerprint, _)| {
                                m.file.kind == MediaKind::Video
                                    && *fingerprint == m.file.fingerprint
                            })
                            .map(|(_, size)| *size)
                            .or_else(|| m.width.zip(m.height).filter(|(w, h)| *w > 0 && *h > 0))
                            // A slow first video must not hide every following video.
                            // Only video placeholders use a provisional 16:9 frame;
                            // real pixels still wait for their own display dimensions.
                            .or_else(|| (m.file.kind == MediaKind::Video).then_some((16, 9))),
                        m.error.is_some(),
                    ),
                )
            })
            .collect();
        let video_versions: HashMap<_, _> = snapshot
            .files
            .iter()
            .filter(|m| m.file.kind == MediaKind::Video)
            .map(|m| (m.id, &m.file.fingerprint))
            .collect();
        let mut y = margin;
        for group in &view.groups {
            if group
                .directory
                .ancestors()
                .skip(1)
                .any(|p| collapsed.contains(p))
            {
                continue;
            }
            let depth = group
                .directory
                .strip_prefix(&snapshot.root)
                .map(|p| p.components().count())
                .unwrap_or(0);
            let indent = tree_indent(depth, available);
            let group_width = (available - indent).max(40.);
            let card_width = (group_width - gap * (columns - 1) as f32) / columns as f32;
            let folded = collapsed.contains(&group.directory);
            result.headers.push(Header {
                directory: group.directory.clone(),
                title: group.title.clone(),
                counts: group.counts,
                direct_folders: group.direct_folders,
                kind: None,
                rect: Rect {
                    x: margin,
                    y,
                    w: available,
                    h: 30.,
                },
                collapsed: folded,
            });
            y += 34.;
            if folded {
                continue;
            }
            for section in &group.sections {
                if let Some(kind) = section.kind {
                    result.headers.push(Header {
                        directory: group.directory.clone(),
                        title: match kind {
                            MediaKind::Image => crate::ui_text!(msg_d24c10d37db0).into(),
                            MediaKind::Video => crate::ui_text!(msg_c20f7618d330).into(),
                        },
                        counts: Counts::default(),
                        direct_folders: 0,
                        kind: Some(kind),
                        rect: Rect {
                            x: margin,
                            y,
                            w: available,
                            h: 24.,
                        },
                        collapsed: false,
                    });
                    y += 30.;
                }
                let mut bottoms = vec![y; columns];
                for (position, id) in section.items.iter().enumerate() {
                    let (size, failed) = dimensions.get(id).copied().unwrap_or((None, false));
                    if size.is_none() && !failed {
                        let pending_y = bottoms.iter().copied().fold(y, f32::max);
                        result.sizing_rows.push(SizingRow {
                            probe_top: bottoms.iter().copied().fold(f32::INFINITY, f32::min),
                            rect: Rect {
                                x: margin + indent,
                                y: pending_y,
                                w: group_width,
                                h: 34.,
                            },
                            items: section.items[position..]
                                .iter()
                                .filter(|id| {
                                    dimensions
                                        .get(id)
                                        .is_some_and(|(size, failed)| size.is_none() && !failed)
                                })
                                // A bounded candidate window lets later UI
                                // frames skip acknowledged probes while the
                                // normal200ms index snapshot is coalescing.
                                // Actual per-frame requests remain capped16.
                                .take(64)
                                .copied()
                                .collect(),
                        });
                        bottoms.fill(pending_y + 40.);
                        break;
                    }
                    let column = (0..columns)
                        .min_by(|a, b| bottoms[*a].total_cmp(&bottoms[*b]).then_with(|| a.cmp(b)))
                        .unwrap();
                    let (w, h) = size.unwrap_or((1, 1));
                    if let Some(fingerprint) = video_versions.get(id) {
                        result
                            .video_geometry
                            .insert(*id, ((*fingerprint).clone(), (w, h)));
                    }
                    let height = (card_width * h as f32 / w as f32).clamp(72., card_width * 2.5);
                    let rect = Rect {
                        x: margin + indent + column as f32 * (card_width + gap),
                        y: bottoms[column],
                        w: card_width,
                        h: height,
                    };
                    result.columns[column].push(Card { id: *id, rect });
                    result.rectangles.insert(*id, rect);
                    bottoms[column] = rect.bottom() + gap;
                }
                y = bottoms.into_iter().fold(y, f32::max) + 4.;
            }
            y += 12.;
        }
        result.height = (y + margin).max(1.);
        // An unfinished dimension prefix is not a smaller library. Retain the
        // previous scroll extent while its cards are still in this visible view,
        // otherwise ScrollArea clamps the restored anchor before metadata arrives.
        // Do not reserve it for removals, filtering, folding, or a resized scope.
        if !result.sizing_rows.is_empty() {
            if let Some(previous) = previous {
                let ids: HashSet<_> = view
                    .groups
                    .iter()
                    .flat_map(|group| &group.sections)
                    .flat_map(|section| section.items.iter().copied())
                    .collect();
                let groups: HashMap<_, _> = result
                    .headers
                    .iter()
                    .map(|header| ((&header.directory, header.kind), header.collapsed))
                    .collect();
                let same_groups = previous
                    .headers
                    .iter()
                    .all(|old| groups.get(&(&old.directory, old.kind)) == Some(&old.collapsed));
                if same_groups && previous.rectangles.keys().all(|id| ids.contains(id)) {
                    result.height = result.height.max(previous.height);
                }
            }
        }
        let mut open: Vec<(&Header, usize)> = Vec::new();
        for header in result.headers.iter().filter(|h| h.kind.is_none()) {
            while open
                .last()
                .is_some_and(|(parent, _)| !header.directory.starts_with(&parent.directory))
            {
                let (parent, depth) = open.pop().unwrap();
                result.tree_guides.push(Rect {
                    x: margin + 3. + tree_indent(depth, available),
                    y: parent.rect.y + parent.rect.h,
                    w: 1.,
                    h: (header.rect.y - parent.rect.y - parent.rect.h).max(0.),
                });
            }
            let depth = header
                .directory
                .strip_prefix(&snapshot.root)
                .map(|p| p.components().count())
                .unwrap_or(0);
            open.push((header, depth));
        }
        for (parent, depth) in open {
            result.tree_guides.push(Rect {
                x: margin + 3. + tree_indent(depth, available),
                y: parent.rect.y + parent.rect.h,
                w: 1.,
                h: (y - parent.rect.y - parent.rect.h).max(0.),
            });
        }
        result
    }
    pub fn visible_cards(&self, top: f32, bottom: f32) -> Vec<&Card> {
        let mut result = Vec::new();
        for column in &self.columns {
            let start = column.partition_point(|card| card.rect.bottom() < top);
            for card in &column[start..] {
                if card.rect.y > bottom {
                    break;
                }
                result.push(card);
            }
        }
        result.sort_by(|a, b| {
            a.rect
                .y
                .total_cmp(&b.rect.y)
                .then_with(|| a.rect.x.total_cmp(&b.rect.x))
        });
        result
    }
    pub fn visible_headers(&self, top: f32, bottom: f32) -> &[Header] {
        let start = self.headers.partition_point(|h| h.rect.bottom() < top);
        let end = self.headers.partition_point(|h| h.rect.y <= bottom);
        &self.headers[start.min(end)..end]
    }
    pub fn anchor(&self, scroll: f32) -> Option<ScrollAnchor> {
        self.visible_cards(scroll, scroll + 1.)
            .into_iter()
            .min_by(|a, b| a.rect.y.total_cmp(&b.rect.y))
            .map(|card| ScrollAnchor {
                media_id: card.id,
                offset: card.rect.y - scroll,
            })
    }
    pub fn restore_anchor(&self, anchor: &ScrollAnchor) -> Option<f32> {
        self.rectangles
            .get(&anchor.media_id)
            .map(|r| (r.y - anchor.offset).max(0.))
    }
}

/// Reciprocal interpolation makes the slider approximately linear in column count.
pub fn preview_width(scale: f32) -> f32 {
    if scale >= 1. {
        MAX_PREVIEW_WIDTH
    } else {
        160. / (1. - scale.clamp(0., 1.) * 0.9)
    }
}
pub fn preview_scale(width: f32) -> f32 {
    ((1. - 160. / width.clamp(160., MAX_PREVIEW_WIDTH)) / 0.9).clamp(0., 1.)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{self, ViewSettings};
    fn snapshot(n: usize) -> LibrarySnapshot {
        let root = PathBuf::from("root");
        LibrarySnapshot {
            generation: 1,
            root: root.clone(),
            files: (0..n)
                .map(|i| {
                    let path = root.join(format!("{i}.jpg"));
                    MediaRecord {
                        id: i as i64,
                        file: MediaFile {
                            path,
                            parent: root.clone(),
                            kind: MediaKind::Image,
                            fingerprint: Fingerprint {
                                bytes: 1,
                                modified_ns: 1,
                            },
                        },
                        width: Some(if i % 2 == 0 { 100 } else { 200 }),
                        height: Some(150),
                        duration_ms: None,
                        revision: 0,
                        error: None,
                    }
                })
                .collect(),
            directories: vec![],
            finished: true,
            errors: vec![],
            elapsed_ms: 0,
        }
    }
    #[test]
    fn maximum_preview_is_single_column_even_on_8k_screen() {
        let snapshot = snapshot(5);
        let view = view::build(&snapshot, &ViewSettings::default());
        for width in [800., 3840., 7680.] {
            let layout =
                MasonryLayout::build(&snapshot, &view, width, MAX_PREVIEW_WIDTH, &HashSet::new());
            assert_eq!(layout.columns.len(), 1);
            assert_eq!(layout.rectangles.len(), 5);
        }
    }
    #[test]
    fn incomplete_refresh_keeps_scroll_extent_but_removal_and_resize_do_not() {
        let complete = snapshot(40);
        let view = view::build(&complete, &ViewSettings::default());
        let before = MasonryLayout::build(&complete, &view, 800., 260., &HashSet::new());
        let mut partial = complete.clone();
        for record in &mut partial.files[32..] {
            record.width = None;
            record.height = None;
        }
        let partial_view = view::build(&partial, &ViewSettings::default());
        let refreshed = MasonryLayout::build_stable(
            &partial,
            &partial_view,
            800.,
            260.,
            &HashSet::new(),
            8.,
            Some(&before),
        );
        assert_eq!(refreshed.rectangles.len(), 32);
        assert!(!refreshed.sizing_rows.is_empty());
        assert_eq!(refreshed.height, before.height);
        let resized = MasonryLayout::build_stable(
            &partial,
            &partial_view,
            1200.,
            260.,
            &HashSet::new(),
            8.,
            Some(&before),
        );
        assert!(resized.height < before.height);
        partial.files.truncate(20);
        partial.files.last_mut().unwrap().width = None;
        let removed_view = view::build(&partial, &ViewSettings::default());
        let removed = MasonryLayout::build_stable(
            &partial,
            &removed_view,
            800.,
            260.,
            &HashSet::new(),
            8.,
            Some(&before),
        );
        assert!(removed.height < before.height);
        let resolved = MasonryLayout::build_stable(
            &complete,
            &view,
            800.,
            260.,
            &HashSet::new(),
            8.,
            Some(&refreshed),
        );
        assert_eq!(resolved.height, before.height);
    }
    #[test]
    fn unknown_geometry_has_no_square_slot_and_ready_prefix_never_moves() {
        let mut snapshot = snapshot(6);
        snapshot.files[2].width = None;
        snapshot.files[2].height = None;
        let view = view::build(&snapshot, &ViewSettings::default());
        let before = MasonryLayout::build(&snapshot, &view, 1200., 260., &HashSet::new());
        assert_eq!(before.rectangles.len(), 2);
        assert_eq!(before.sizing_rows.len(), 1);
        assert_eq!(before.sizing_rows[0].items, vec![2]);
        snapshot.files[2].width = Some(200);
        snapshot.files[2].height = Some(500);
        let after = MasonryLayout::build(&snapshot, &view, 1200., 260., &HashSet::new());
        assert_eq!(after.rectangles.len(), 6);
        for (id, rect) in &before.rectangles {
            assert_eq!(after.rectangles[id], *rect);
        }
        assert!(after.sizing_rows.is_empty());
    }
    #[test]
    fn empty_column_probes_before_the_label_without_allocating_unknown_geometry() {
        let mut snapshot = snapshot(5);
        for record in &mut snapshot.files[..4] {
            record.width = Some(1000);
            record.height = Some(3000);
        }
        snapshot.files[4].width = None;
        snapshot.files[4].height = None;
        let view = view::build(&snapshot, &ViewSettings::default());
        let before = MasonryLayout::build(&snapshot, &view, 1600., 320., &HashSet::new());
        assert_eq!(before.columns.len(), 5);
        assert_eq!(before.rectangles.len(), 4);
        let pending = &before.sizing_rows[0];
        assert!(pending.rect.y > 700., "label must not overlap tall cards");
        assert!(pending.probe_top < 80., "the empty fifth column is visible");
        snapshot.files[4].width = Some(1000);
        snapshot.files[4].height = Some(3000);
        let after = MasonryLayout::build_stable(
            &snapshot,
            &view,
            1600.,
            320.,
            &HashSet::new(),
            8.,
            Some(&before),
        );
        assert_eq!(after.rectangles.len(), 5);
        for (id, rect) in &before.rectangles {
            assert_eq!(after.rectangles[id], *rect);
        }
        assert!(after.rectangles[&4].y < 80.);
    }
    #[test]
    fn unknown_video_dimensions_do_not_block_the_first_screen() {
        let mut snapshot = snapshot(580);
        for record in &mut snapshot.files {
            record.file.kind = MediaKind::Video;
            record.width = None;
            record.height = None;
        }
        let view = view::build(&snapshot, &ViewSettings::default());
        let layout = MasonryLayout::build(&snapshot, &view, 1200., 260., &HashSet::new());
        assert_eq!(layout.rectangles.len(), 580);
        assert!(layout.sizing_rows.is_empty());
        let visible = layout.visible_cards(0., 700.);
        assert!(visible.len() > 4 && visible.len() < 40);
        assert!(visible
            .windows(2)
            .all(|pair| pair[0].rect.y <= pair[1].rect.y));
    }
    #[test]
    fn staggered_video_metadata_does_not_reflow_allocated_cards() {
        let mut snapshot = snapshot(580);
        for record in &mut snapshot.files {
            record.file.kind = MediaKind::Video;
            record.width = None;
            record.height = None;
        }
        let view = view::build(&snapshot, &ViewSettings::default());
        let mut previous = MasonryLayout::build(&snapshot, &view, 1200., 260., &HashSet::new());
        let rectangles = previous.rectangles.clone();
        let height = previous.height;
        let anchor = previous.anchor(1700.).unwrap();
        for batch in 0..10 {
            for record in snapshot.files.iter_mut().skip(batch * 58).take(58) {
                let (w, h) = match record.id % 3 {
                    0 => (1080, 1920),
                    1 => (1920, 1080),
                    _ => (1080, 1080),
                };
                record.width = Some(w);
                record.height = Some(h);
                record.duration_ms = Some(65000);
            }
            let next = MasonryLayout::build_stable(
                &snapshot,
                &view,
                1200.,
                260.,
                &HashSet::new(),
                8.,
                Some(&previous),
            );
            assert_eq!(next.rectangles, rectangles);
            assert_eq!(next.height, height);
            assert!((next.restore_anchor(&anchor).unwrap() - 1700.).abs() < 0.01);
            previous = next;
        }
        // Deliberate zoom uses the now-known dimensions, retaining portrait proportions.
        let zoomed = MasonryLayout::build_stable(
            &snapshot,
            &view,
            1200.,
            400.,
            &HashSet::new(),
            8.,
            Some(&previous),
        );
        let portrait = zoomed.rectangles[&0];
        assert!((portrait.h / portrait.w - 1920. / 1080.).abs() < 0.001);
        // A replaced video must not inherit the old file's reserved aspect ratio.
        snapshot.files[0].file.fingerprint.modified_ns += 1;
        let replaced = MasonryLayout::build_stable(
            &snapshot,
            &view,
            1200.,
            260.,
            &HashSet::new(),
            8.,
            Some(&previous),
        );
        let portrait = replaced.rectangles[&0];
        assert!((portrait.h / portrait.w - 1920. / 1080.).abs() < 0.001);
    }
    #[test]
    fn variable_heights_do_not_overlap_and_only_visible_cards_are_queried() {
        let s = snapshot(5000);
        let v = view::build(&s, &ViewSettings::default());
        let layout = MasonryLayout::build(&s, &v, 1200., 260., &HashSet::new());
        assert_eq!(layout.rectangles.len(), 5000);
        for c in &layout.columns {
            assert!(c
                .windows(2)
                .all(|pair| pair[0].rect.bottom() < pair[1].rect.y));
        }
        assert!(layout.visible_cards(2000., 2800.).len() < 100);
        assert!(layout
            .visible_cards(2000., 2800.)
            .iter()
            .all(|c| c.rect.bottom() >= 2000. && c.rect.y <= 2800.));
    }
    #[test]
    fn collapse_affects_geometry_not_detail_sequence() {
        let s = snapshot(20);
        let v = view::build(&s, &ViewSettings::default());
        let collapsed = HashSet::from([s.root.clone()]);
        let layout = MasonryLayout::build(&s, &v, 1000., 260., &collapsed);
        assert!(layout.rectangles.is_empty());
        assert_eq!(v.sequence.len(), 20);
    }
    #[test]
    fn anchor_survives_column_width_change() {
        let s = snapshot(80);
        let v = view::build(&s, &ViewSettings::default());
        let a = MasonryLayout::build(&s, &v, 1000., 260., &HashSet::new());
        let anchor = a.anchor(700.).unwrap();
        let b = MasonryLayout::build(&s, &v, 1400., 260., &HashSet::new());
        let scroll = b.restore_anchor(&anchor).unwrap();
        assert!((b.rectangles[&anchor.media_id].y - scroll - anchor.offset).abs() < 0.1);
    }
}

#[cfg(test)]
mod density_tests {
    #[test]
    fn preview_slider_midpoint_keeps_multiple_columns() {
        use super::*;
        assert_eq!(preview_width(1.), MAX_PREVIEW_WIDTH);
        assert!((preview_width(0.6) - 347.82608).abs() < 0.01);
        for width in [160., 260., 512., 1024., 1600.] {
            assert!((preview_width(preview_scale(width)) - width).abs() < 0.01);
        }
    }
}
