//! Worker-built navigation index; UI queries never scan the library or filesystem.
use crate::view::MediaView;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
#[derive(Default)]
pub struct MediaDirectoryIndex {
    ordinals: HashMap<PathBuf, usize>,
    media: Vec<(usize, PathBuf)>,
}
impl MediaDirectoryIndex {
    pub fn build(view: &MediaView) -> Self {
        Self {
            ordinals: view
                .groups
                .iter()
                .enumerate()
                .map(|(i, g)| (g.directory.clone(), i))
                .collect(),
            media: view
                .groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.sections.iter().any(|s| !s.items.is_empty()))
                .map(|(i, g)| (i, g.directory.clone()))
                .collect(),
        }
    }
    pub fn target(&self, anchor: Option<&Path>, up: bool, include_current: bool) -> Option<&Path> {
        let Some(ordinal) = anchor.and_then(|p| self.ordinals.get(p)).copied() else {
            return (!up)
                .then(|| self.media.first())
                .flatten()
                .map(|(_, p)| p.as_path());
        };
        let slot = if up {
            self.media
                .partition_point(|(i, _)| *i < ordinal || (include_current && *i == ordinal))
                .checked_sub(1)?
        } else {
            self.media
                .partition_point(|(i, _)| *i < ordinal || (!include_current && *i == ordinal))
        };
        self.media.get(slot).map(|(_, p)| p.as_path())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{Counts, MediaKind},
        view::{MediaGroup, MediaSection},
    };
    #[test]
    fn skips_empty_groups_preserves_filtered_order_and_stops_at_boundaries() {
        let view = MediaView {
            groups: [
                ("root", vec![]),
                ("root/a", vec![1]),
                ("root/b", vec![]),
                ("root/b/empty", vec![]),
                ("root/c", vec![2]),
            ]
            .into_iter()
            .map(|(p, items)| MediaGroup {
                directory: p.into(),
                title: p.into(),
                counts: Counts::default(),
                direct_folders: 0,
                sections: vec![MediaSection {
                    kind: Some(MediaKind::Image),
                    items,
                }],
            })
            .collect(),
            sequence: vec![1, 2],
            shown: Counts::default(),
        };
        let i = MediaDirectoryIndex::build(&view);
        assert_eq!(
            i.target(Some(Path::new("root/b/empty")), false, false),
            Some(Path::new("root/c"))
        );
        assert_eq!(
            i.target(Some(Path::new("root/b/empty")), true, true),
            Some(Path::new("root/a"))
        );
        assert_eq!(i.target(Some(Path::new("root/a")), true, false), None);
        assert_eq!(i.target(Some(Path::new("root/c")), false, false), None);
        assert_eq!(
            i.target(Some(Path::new("root/c")), true, true),
            Some(Path::new("root/c"))
        );
        let mut filtered = view;
        filtered.groups[4].sections.clear();
        let i = MediaDirectoryIndex::build(&filtered);
        assert_eq!(i.target(Some(Path::new("root/b")), false, false), None);
    }
}
