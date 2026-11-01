//! Pure view construction, independent from filesystem, UI, and collapsed layout state.
use crate::domain::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MediaFilter {
    #[default]
    All,
    Images,
    Videos,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortField {
    #[default]
    Name,
    Modified,
    Size,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ViewSettings {
    pub filter: MediaFilter,
    pub sort: SortField,
    pub descending: bool,
    pub type_groups: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSection {
    pub kind: Option<MediaKind>,
    pub items: Vec<MediaId>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaGroup {
    pub directory: PathBuf,
    pub title: String,
    pub counts: Counts,
    pub direct_folders: u64,
    pub sections: Vec<MediaSection>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaView {
    pub groups: Vec<MediaGroup>,
    pub sequence: Vec<MediaId>,
    pub shown: Counts,
}

// Group lookup needs path identity, not path ordering. Final presentation order
// still comes exclusively from directory_cmp below. Keep the old map for diagnosis.
struct DirectoryIndex<V> {
    ordered: BTreeMap<PathBuf, V>,
    hashed: Option<HashMap<PathBuf, V>>,
}
impl<V: Default> DirectoryIndex<V> {
    fn new(hashed: bool) -> Self {
        Self {
            ordered: BTreeMap::new(),
            hashed: hashed.then(HashMap::new),
        }
    }
    fn value(&mut self, path: PathBuf) -> &mut V {
        if let Some(map) = &mut self.hashed {
            map.entry(path).or_default()
        } else {
            self.ordered.entry(path).or_default()
        }
    }
    fn keys(&self) -> impl Iterator<Item = &PathBuf> {
        self.ordered
            .keys()
            .chain(self.hashed.iter().flat_map(|map| map.keys()))
    }
    fn get(&self, path: &PathBuf) -> Option<&V> {
        if let Some(map) = &self.hashed {
            map.get(path)
        } else {
            self.ordered.get(path)
        }
    }
    fn remove(&mut self, path: &PathBuf) -> Option<V> {
        if let Some(map) = &mut self.hashed {
            map.remove(path)
        } else {
            self.ordered.remove(path)
        }
    }
    fn into_entries(self) -> Vec<(PathBuf, V)> {
        if let Some(map) = self.hashed {
            map.into_iter().collect()
        } else {
            self.ordered.into_iter().collect()
        }
    }
}

pub fn build(snapshot: &LibrarySnapshot, settings: &ViewSettings) -> MediaView {
    build_with_caches(snapshot, settings, false, true)
}

/// Diagnostic switch preserves the former comparator for same-binary comparisons.
pub fn build_with_sort_cache(
    snapshot: &LibrarySnapshot,
    settings: &ViewSettings,
    cache_keys: bool,
) -> MediaView {
    build_with_caches(snapshot, settings, cache_keys, true)
}
pub fn build_with_caches(
    snapshot: &LibrarySnapshot,
    settings: &ViewSettings,
    cache_keys: bool,
    hash_groups: bool,
) -> MediaView {
    let mut grouped: DirectoryIndex<Vec<&MediaRecord>> = DirectoryIndex::new(hash_groups);
    let mut counts: DirectoryIndex<Counts> = DirectoryIndex::new(hash_groups);
    for item in &snapshot.files {
        counts.value(item.file.parent.clone()).add(item.file.kind);
        if matches!(
            (settings.filter, item.file.kind),
            (MediaFilter::Images, MediaKind::Video) | (MediaFilter::Videos, MediaKind::Image)
        ) {
            continue;
        }
        grouped.value(item.file.parent.clone()).push(item);
    }
    // Keep real parent nodes even when they have no direct media.
    let paths: Vec<_> = grouped
        .keys()
        .cloned()
        .chain(snapshot.directories.iter().map(|d| d.path.clone()))
        .collect();
    for path in paths {
        for ancestor in path
            .ancestors()
            .take_while(|p| p.starts_with(&snapshot.root))
        {
            grouped.value(ancestor.to_owned());
        }
    }
    let mut child_counts: DirectoryIndex<u64> = DirectoryIndex::new(hash_groups);
    for directory in grouped.keys() {
        if directory != &snapshot.root {
            if let Some(parent) = directory.parent() {
                *child_counts.value(parent.to_owned()) += 1;
            }
        }
    }
    let mut groups = grouped.into_entries();
    groups.sort_by(|a, b| directory_cmp(&a.0, &b.0));
    let mut result = MediaView {
        groups: Vec::new(),
        sequence: Vec::new(),
        shown: Counts::default(),
    };
    for (directory, mut records) in groups {
        if cache_keys {
            // Build temporary keys once per record on the existing prepare worker.
            // Keep lossless path bytes as the exact former deterministic tie-break.
            let mut keyed: Vec<_> = records
                .into_iter()
                .map(|record| {
                    let name = (settings.sort == SortField::Name).then(|| {
                        record
                            .file
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase()
                    });
                    (record, name, path_blob(&record.file.path))
                })
                .collect();
            keyed.sort_by(|a, b| {
                let kind = if settings.type_groups {
                    kind_rank(a.0.file.kind).cmp(&kind_rank(b.0.file.kind))
                } else {
                    std::cmp::Ordering::Equal
                };
                kind.then_with(|| {
                    let field = match settings.sort {
                        SortField::Name => natural_cmp_folded(
                            a.1.as_ref().unwrap().as_bytes(),
                            b.1.as_ref().unwrap().as_bytes(),
                        ),
                        SortField::Modified => {
                            a.0.file
                                .fingerprint
                                .modified_ns
                                .cmp(&b.0.file.fingerprint.modified_ns)
                        }
                        SortField::Size => {
                            a.0.file.fingerprint.bytes.cmp(&b.0.file.fingerprint.bytes)
                        }
                    };
                    if settings.descending {
                        field.reverse()
                    } else {
                        field
                    }
                })
                .then_with(|| a.2.cmp(&b.2))
            });
            records = keyed.into_iter().map(|entry| entry.0).collect();
        } else {
            records.sort_by(|a, b| {
                let kind = if settings.type_groups {
                    kind_rank(a.file.kind).cmp(&kind_rank(b.file.kind))
                } else {
                    std::cmp::Ordering::Equal
                };
                let field = match settings.sort {
                    SortField::Name => natural_cmp(
                        &a.file
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                        &b.file
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    ),
                    SortField::Modified => a
                        .file
                        .fingerprint
                        .modified_ns
                        .cmp(&b.file.fingerprint.modified_ns),
                    SortField::Size => a.file.fingerprint.bytes.cmp(&b.file.fingerprint.bytes),
                };
                kind.then(if settings.descending {
                    field.reverse()
                } else {
                    field
                })
                .then_with(|| path_blob(&a.file.path).cmp(&path_blob(&b.file.path)))
            });
        }
        let mut sections: Vec<MediaSection> = Vec::new();
        for record in records {
            result.shown.add(record.file.kind);
            result.sequence.push(record.id);
            let kind = settings.type_groups.then_some(record.file.kind);
            if sections.last().is_none_or(|s| s.kind != kind) {
                sections.push(MediaSection {
                    kind,
                    items: Vec::new(),
                });
            }
            sections.last_mut().unwrap().items.push(record.id);
        }
        let title = directory
            .file_name()
            .unwrap_or(directory.as_os_str())
            .to_string_lossy()
            .into_owned();
        result.groups.push(MediaGroup {
            counts: counts.remove(&directory).unwrap_or_default(),
            direct_folders: child_counts.get(&directory).copied().unwrap_or(0),
            directory,
            title,
            sections,
        });
    }
    result
}
/// Compare components so a subtree stays contiguous (a, a/child, a-2).
pub fn directory_cmp(a: &std::path::Path, b: &std::path::Path) -> std::cmp::Ordering {
    let mut aa = a.components();
    let mut bb = b.components();
    loop {
        match (aa.next(), bb.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => {
                let order = natural_cmp(
                    &a.as_os_str().to_string_lossy(),
                    &b.as_os_str().to_string_lossy(),
                )
                .then_with(|| a.as_os_str().cmp(b.as_os_str()));
                if !order.is_eq() {
                    return order;
                }
            }
        }
    }
}
fn kind_rank(kind: MediaKind) -> u8 {
    match kind {
        MediaKind::Image => 0,
        MediaKind::Video => 1,
    }
}
pub fn neighbor(sequence: &[MediaId], current: MediaId, delta: isize) -> Option<MediaId> {
    let index = sequence.iter().position(|id| *id == current)?;
    index
        .checked_add_signed(delta)
        .and_then(|i| sequence.get(i))
        .copied()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollAnchor {
    pub media_id: MediaId,
    pub offset: f32,
}
#[derive(Debug, Clone)]
pub struct NavigationState {
    pub root: PathBuf,
    pub settings: ViewSettings,
    pub collapsed: HashSet<PathBuf>,
    pub auto_expanded: HashSet<PathBuf>,
    pub anchor: Option<ScrollAnchor>,
}
#[derive(Default)]
pub struct NavigationHistory {
    entries: Vec<NavigationState>,
    cursor: usize,
}
impl NavigationHistory {
    pub fn visit(&mut self, state: NavigationState) {
        if self
            .entries
            .get(self.cursor)
            .is_some_and(|s| s.root == state.root)
        {
            self.entries[self.cursor] = state;
            return;
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.cursor + 1);
        }
        self.entries.push(state);
        self.cursor = self.entries.len() - 1;
    }
    pub fn replace_current(&mut self, state: NavigationState) {
        if let Some(current) = self.entries.get_mut(self.cursor) {
            *current = state;
        } else {
            self.visit(state);
        }
    }
    pub fn can_back(&self) -> bool {
        self.cursor > 0 && !self.entries.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }
    pub fn back(&mut self) -> Option<NavigationState> {
        if !self.can_back() {
            return None;
        }
        self.cursor -= 1;
        Some(self.entries[self.cursor].clone())
    }
    pub fn forward(&mut self) -> Option<NavigationState> {
        if !self.can_forward() {
            return None;
        }
        self.cursor += 1;
        Some(self.entries[self.cursor].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> LibrarySnapshot {
        let root = PathBuf::from("root");
        let rows = [
            (1, "10.jpg", 1, 5),
            (2, "2.mov", 9, 1),
            (3, "sub/1.jpg", 3, 7),
            (4, "sub/2.mov", 2, 9),
        ];
        LibrarySnapshot {
            generation: 1,
            root: root.clone(),
            files: rows
                .iter()
                .map(|(id, name, bytes, time)| {
                    let p = root.join(name);
                    MediaRecord {
                        id: *id,
                        file: MediaFile {
                            parent: p.parent().unwrap().to_owned(),
                            kind: media_kind(&p).unwrap(),
                            path: p,
                            fingerprint: Fingerprint {
                                bytes: *bytes,
                                modified_ns: *time,
                            },
                        },
                        width: None,
                        height: None,
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
    fn folder_count_is_direct_includes_empty_and_does_not_follow_media_filter() {
        let mut s = snapshot();
        for suffix in ["empty", "sub/child", "sub/child/deeper"] {
            let path = s.root.join(suffix);
            s.directories.push(DirectoryRecord {
                parent: path.parent().map(|p| p.to_owned()),
                path,
                direct: Counts::default(),
                recursive: Counts::default(),
                state: ScanState::Complete,
                subtree_complete: true,
            });
        }
        let mut settings = ViewSettings::default();
        settings.filter = MediaFilter::Videos;
        let view = build(&s, &settings);
        assert_eq!(
            view.groups
                .iter()
                .find(|g| g.directory == s.root)
                .unwrap()
                .direct_folders,
            2
        );
        assert_eq!(
            view.groups
                .iter()
                .find(|g| g.directory == s.root.join("sub"))
                .unwrap()
                .direct_folders,
            1
        );
        assert_eq!(
            view.groups
                .iter()
                .find(|g| g.directory == s.root.join("empty"))
                .unwrap()
                .direct_folders,
            0
        );
    }
    #[test]
    fn cached_sort_preserves_all_view_settings_and_unicode_numeric_ties() {
        let mut source = snapshot();
        let prototype = source.files[0].clone();
        source.files.clear();
        let names = [
            "图集2.jpg",
            "图集02.jpg",
            "图集10.JPG",
            "A1.jpg",
            "a1.jpg",
            "İ2.JPG",
            "i̇2.jpg",
            "Σ3.jpg",
            "σ3.jpg",
            "000.jpg",
            "0.jpg",
            "999999999999999999999999999999999.jpg",
            "混合1A.jpg",
            "混合1a.jpg",
        ];
        for i in 0..700 {
            let mut record = prototype.clone();
            record.id = i + 1;
            record.file.parent = source.root.join(if i % 3 == 0 {
                "子目录2"
            } else {
                "子目录10"
            });
            record.file.path = record.file.parent.join(format!(
                "{}-{}",
                names[(i as usize * 37) % names.len()],
                i % 7
            ));
            record.file.kind = if i % 4 == 0 {
                MediaKind::Video
            } else {
                MediaKind::Image
            };
            record.file.fingerprint.bytes = (i % 5) as u64;
            record.file.fingerprint.modified_ns = (i % 3) as _;
            source.files.push(record);
        }
        for filter in [MediaFilter::All, MediaFilter::Images, MediaFilter::Videos] {
            for sort in [SortField::Name, SortField::Modified, SortField::Size] {
                for descending in [false, true] {
                    for type_groups in [false, true] {
                        let settings = ViewSettings {
                            filter,
                            sort,
                            descending,
                            type_groups,
                        };
                        let legacy = build_with_caches(&source, &settings, false, false);
                        for keys in [false, true] {
                            for hashed in [false, true] {
                                assert_eq!(
                                    build_with_caches(&source, &settings, keys, hashed),
                                    legacy,
                                    "{settings:?} keys={keys} hashed={hashed}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn natural_sort_and_detail_crosses_real_groups() {
        let view = build(&snapshot(), &ViewSettings::default());
        assert_eq!(view.sequence, vec![2, 1, 3, 4]);
        assert_eq!(neighbor(&view.sequence, 1, 1), Some(3));
        assert_eq!(neighbor(&view.sequence, 2, -1), None);
        assert_eq!(neighbor(&view.sequence, 4, 1), None);
    }
    #[test]
    fn filter_and_type_grouping_never_merge_folders() {
        let mut options = ViewSettings {
            type_groups: true,
            descending: true,
            ..Default::default()
        };
        let view = build(&snapshot(), &options);
        assert_eq!(view.sequence, vec![1, 2, 3, 4]);
        assert_eq!(view.groups.len(), 2);
        assert_eq!(view.groups[0].sections.len(), 2);
        options.filter = MediaFilter::Videos;
        let view = build(&snapshot(), &options);
        assert_eq!(view.sequence, vec![2, 4]);
        assert_eq!(
            view.shown,
            Counts {
                images: 0,
                videos: 2
            }
        );
        assert_eq!(
            view.groups[0].counts,
            Counts {
                images: 1,
                videos: 1
            }
        );
    }
    #[test]
    fn size_and_time_sort_ascending_descending() {
        let mut options = ViewSettings {
            sort: SortField::Size,
            ..Default::default()
        };
        assert_eq!(build(&snapshot(), &options).sequence, vec![1, 2, 4, 3]);
        options.descending = true;
        assert_eq!(build(&snapshot(), &options).sequence, vec![2, 1, 3, 4]);
        options.sort = SortField::Modified;
        assert_eq!(build(&snapshot(), &options).sequence, vec![1, 2, 4, 3]);
    }
    #[test]
    fn navigation_restores_anchor_and_discards_forward_branch() {
        let mut history = NavigationHistory::default();
        let state = |name: &str| NavigationState {
            root: PathBuf::from(name),
            settings: ViewSettings::default(),
            collapsed: HashSet::new(),
            auto_expanded: HashSet::new(),
            anchor: Some(ScrollAnchor {
                media_id: 42,
                offset: 18.,
            }),
        };
        history.visit(state("A"));
        history.visit(state("B"));
        assert_eq!(
            history.back().unwrap().anchor,
            Some(ScrollAnchor {
                media_id: 42,
                offset: 18.
            })
        );
        history.visit(state("C"));
        assert!(!history.can_forward());
        assert_eq!(history.back().unwrap().root, PathBuf::from("A"));
    }
    #[test]
    fn component_sort_keeps_ancestor_before_subtree_and_natural_siblings() {
        let mut paths: Vec<PathBuf> = ["root/a-2", "root/a/10", "root/a/2", "root/a", "root"]
            .into_iter()
            .map(PathBuf::from)
            .collect();
        paths.sort_by(|a, b| directory_cmp(a, b));
        assert_eq!(
            paths,
            ["root", "root/a", "root/a/2", "root/a/10", "root/a-2"].map(PathBuf::from)
        );
    }
    #[test]
    fn missing_intermediate_directories_are_headers_and_collapse_hides_descendants() {
        let mut s = snapshot();
        let path = s.root.join("123/456/789/photo.jpg");
        s.files[0].file.parent = path.parent().unwrap().to_owned();
        s.files[0].file.path = path;
        let view = build(&s, &ViewSettings::default());
        let parent = s.root.join("123/456");
        assert!(view
            .groups
            .iter()
            .any(|g| g.directory == parent && g.sections.is_empty()));
        let layout = crate::layout::MasonryLayout::build(
            &s,
            &view,
            1200.,
            260.,
            &HashSet::from([parent.clone()]),
        );
        assert!(layout.headers.iter().any(|h| h.directory == parent));
        assert!(!layout
            .headers
            .iter()
            .any(|h| h.directory == s.root.join("123/456/789")));
        assert!(!layout.rectangles.contains_key(&1));
        assert!(view.sequence.contains(&1));
    }
}
