//! Conservative automatic folds from completed recursive scan counts, no file I/O.
use crate::domain::{LibrarySnapshot, ScanState};
use std::{collections::HashSet, path::PathBuf};
pub fn automatic_folds(
    snapshot: &LibrarySnapshot,
    enabled: bool,
    expanded: &HashSet<PathBuf>,
) -> HashSet<PathBuf> {
    if !enabled {
        return HashSet::new();
    }
    snapshot
        .directories
        .iter()
        .filter(|d| {
            d.subtree_complete
                && d.state == ScanState::Complete
                && d.recursive.images == 0
                && d.recursive.videos == 0
                && !expanded.contains(&d.path)
        })
        .map(|d| d.path.clone())
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Counts, DirectoryRecord};
    #[test]
    fn only_known_empty_subtrees_fold_and_explicit_expansion_wins() {
        let directory = |name: &str, images, videos, state, complete| DirectoryRecord {
            path: name.into(),
            modified_ns: None,
            recursive_bytes: 0,
            parent: None,
            direct: Counts::default(),
            recursive: Counts { images, videos },
            state,
            subtree_complete: complete,
        };
        let snapshot = LibrarySnapshot {
            generation: 1,
            root: "root".into(),
            files: vec![],
            directories: vec![
                directory("root/media-descendant", 1, 0, ScanState::Complete, true),
                directory("root/video", 0, 1, ScanState::Complete, true),
                directory("root/empty", 0, 0, ScanState::Complete, true),
                directory("root/scanning", 0, 0, ScanState::Scanning, false),
                directory("root/partial", 0, 0, ScanState::Partial, false),
            ],
            finished: false,
            errors: vec![],
            elapsed_ms: 0,
        };
        assert_eq!(
            automatic_folds(&snapshot, true, &HashSet::new()),
            HashSet::from([PathBuf::from("root/empty")])
        );
        assert!(automatic_folds(&snapshot, false, &HashSet::new()).is_empty());
        assert!(automatic_folds(
            &snapshot,
            true,
            &HashSet::from([PathBuf::from("root/empty")])
        )
        .is_empty());
        let mut changed = snapshot;
        changed.directories[2].recursive.images = 1;
        assert!(automatic_folds(&changed, true, &HashSet::new()).is_empty());
    }
}
