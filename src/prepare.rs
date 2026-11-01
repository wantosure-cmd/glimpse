//! Latest-request-only background sorting/layout. No O(N) work in every UI frame.
use crate::{
    domain::*,
    layout::MasonryLayout,
    view::{self, MediaView, ViewSettings},
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Instant,
};

pub struct PrepareRequest {
    pub revision: u64,
    pub snapshot: Arc<LibrarySnapshot>,
    pub settings: ViewSettings,
    pub collapsed: HashSet<PathBuf>,
    pub auto_fold_empty: bool,
    pub auto_expanded: HashSet<PathBuf>,
    pub width: f32,
    pub target_width: f32,
    pub gap: f32,
    pub previous: Option<Arc<Prepared>>,
    pub reuse_view: bool,
}
pub struct Prepared {
    pub revision: u64,
    pub snapshot: Arc<LibrarySnapshot>,
    pub view: Arc<MediaView>,
    pub layout: MasonryLayout,
    pub lookup: Arc<HashMap<MediaId, usize>>,
    pub media_directories: Arc<crate::media_jump::MediaDirectoryIndex>,
    pub automatic_collapsed: Arc<HashSet<PathBuf>>,
    pub effective_collapsed: HashSet<PathBuf>,
    pub build_ms: f64,
    pub needs_full_font: bool,
    pub view_reused: bool,
    settings: ViewSettings,
}

// Dimensions, duration, errors and scan progress do not change ordering/grouping.
// Compare every input that does, on the worker; never trust generation alone.
fn same_view_inputs(a: &LibrarySnapshot, b: &LibrarySnapshot) -> bool {
    a.generation == b.generation
        && a.root == b.root
        && a.files.len() == b.files.len()
        && a.directories.len() == b.directories.len()
        && a.files.iter().zip(&b.files).all(|(a, b)| {
            a.id == b.id
                && a.file.path == b.file.path
                && a.file.parent == b.file.parent
                && a.file.kind == b.file.kind
                && a.file.fingerprint == b.file.fingerprint
        })
        && a.directories
            .iter()
            .zip(&b.directories)
            .all(|(a, b)| a.path == b.path)
}
#[derive(Default)]
struct State {
    pending: Option<PrepareRequest>,
    ready: Option<Arc<Prepared>>,
    stop: bool,
}
pub struct PrepareService {
    shared: Arc<(Mutex<State>, Condvar)>,
}
impl PrepareService {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        Self::new_with_completed_cache(wake, true)
    }
    pub fn new_with_completed_cache(
        wake: impl Fn() + Send + 'static,
        cache_completed: bool,
    ) -> Self {
        Self::new_with_sort_cache(wake, cache_completed, false)
    }
    pub fn new_with_sort_cache(
        wake: impl Fn() + Send + 'static,
        cache_completed: bool,
        cache_sort_keys: bool,
    ) -> Self {
        Self::new_with_group_index(wake, cache_completed, cache_sort_keys, true)
    }
    pub fn new_with_group_index(
        wake: impl Fn() + Send + 'static,
        cache_completed: bool,
        cache_sort_keys: bool,
        hash_groups: bool,
    ) -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = shared.clone();
        thread::spawn(move || {
            // A completed view remains reusable even when a newer metadata snapshot
            // overtook its UI delivery. One shared result only, never a history.
            let mut completed: Option<Arc<Prepared>> = None;
            loop {
                let request = {
                    let (state, condition) = (&worker.0, &worker.1);
                    let mut state = state.lock().unwrap();
                    while state.pending.is_none() && !state.stop {
                        state = condition.wait(state).unwrap();
                    }
                    if state.stop {
                        return;
                    }
                    state.pending.take().unwrap()
                };
                let began = Instant::now();
                let reusable = [request.previous.as_ref(), completed.as_ref()]
                    .into_iter()
                    .flatten()
                    .find(|previous| {
                        request.reuse_view
                            && previous.settings == request.settings
                            && same_view_inputs(&previous.snapshot, &request.snapshot)
                    });
                let view = reusable
                    .map(|previous| previous.view.clone())
                    .unwrap_or_else(|| {
                        Arc::new(view::build_with_caches(
                            &request.snapshot,
                            &request.settings,
                            cache_sort_keys,
                            hash_groups,
                        ))
                    });
                let automatic_collapsed = crate::empty_folders::automatic_folds(
                    &request.snapshot,
                    request.auto_fold_empty,
                    &request.auto_expanded,
                );
                let mut effective_collapsed = request.collapsed.clone();
                effective_collapsed.extend(automatic_collapsed.iter().cloned());
                let layout = MasonryLayout::build_stable(
                    &request.snapshot,
                    &view,
                    request.width,
                    request.target_width,
                    &effective_collapsed,
                    request.gap,
                    request.previous.as_ref().map(|p| &p.layout),
                );
                let lookup = reusable
                    .map(|previous| previous.lookup.clone())
                    .unwrap_or_else(|| {
                        Arc::new(
                            request
                                .snapshot
                                .files
                                .iter()
                                .enumerate()
                                .map(|(i, m)| (m.id, i))
                                .collect(),
                        )
                    });
                let charset = CHARSET.get_or_init(|| {
                    include_str!("../assets/ui-charset.txt")
                        .chars()
                        .collect::<HashSet<_>>()
                });
                let needs_full_font = reusable
                    .map(|previous| previous.needs_full_font)
                    .unwrap_or_else(|| {
                        request
                            .snapshot
                            .files
                            .iter()
                            .map(|m| &m.file.path)
                            .chain(request.snapshot.directories.iter().map(|d| &d.path))
                            .any(|p| {
                                p.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .chars()
                                    .any(|ch| (ch as u32) >= 0x2e80 && !charset.contains(&ch))
                            })
                    });
                let prepared = Arc::new(Prepared {
                    automatic_collapsed: Arc::new(automatic_collapsed),
                    effective_collapsed,
                    media_directories: reusable
                        .map(|p| p.media_directories.clone())
                        .unwrap_or_else(|| {
                            Arc::new(crate::media_jump::MediaDirectoryIndex::build(&view))
                        }),
                    revision: request.revision,
                    snapshot: request.snapshot,
                    view,
                    layout,
                    lookup,
                    build_ms: began.elapsed().as_secs_f64() * 1000.,
                    needs_full_font,
                    view_reused: reusable.is_some(),
                    settings: request.settings,
                });
                completed = (cache_completed && request.reuse_view).then(|| prepared.clone());
                let mut state = worker.0.lock().unwrap();
                if state.stop {
                    return;
                }
                if state
                    .pending
                    .as_ref()
                    .is_none_or(|next| next.revision <= prepared.revision)
                {
                    state.ready = Some(prepared);
                    drop(state);
                    wake();
                }
            }
        });
        Self { shared }
    }
    pub fn submit(&self, request: PrepareRequest) {
        self.shared.0.lock().unwrap().pending = Some(request);
        self.shared.1.notify_one();
    }
    pub fn poll(&self) -> Option<Arc<Prepared>> {
        self.shared.0.lock().unwrap().ready.take()
    }
}
static CHARSET: std::sync::OnceLock<HashSet<char>> = std::sync::OnceLock::new();
impl Drop for PrepareService {
    fn drop(&mut self) {
        self.shared.0.lock().unwrap().stop = true;
        self.shared.1.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn snapshot() -> LibrarySnapshot {
        let root = PathBuf::from("root");
        LibrarySnapshot {
            generation: 1,
            root: root.clone(),
            finished: true,
            errors: vec![],
            elapsed_ms: 0,
            files: (1..=3)
                .map(|id| MediaRecord {
                    id,
                    file: MediaFile {
                        path: root.join(format!("{id}.png")),
                        parent: root.clone(),
                        kind: MediaKind::Image,
                        fingerprint: Fingerprint {
                            bytes: 100,
                            modified_ns: 1,
                        },
                    },
                    width: Some(400),
                    height: Some(300),
                    duration_ms: None,
                    revision: 0,
                    error: None,
                })
                .collect(),
            directories: vec![DirectoryRecord {
                path: root,
                parent: None,
                direct: Counts::default(),
                recursive: Counts::default(),
                state: ScanState::Complete,
                subtree_complete: true,
            }],
        }
    }

    fn prepare(
        service: &PrepareService,
        snapshot: LibrarySnapshot,
        settings: ViewSettings,
        previous: Option<Arc<Prepared>>,
        revision: u64,
    ) -> Arc<Prepared> {
        service.submit(PrepareRequest {
            revision,
            snapshot: Arc::new(snapshot),
            settings,
            collapsed: HashSet::new(),
            auto_fold_empty: false,
            auto_expanded: HashSet::new(),
            width: 1000.,
            target_width: 240.,
            gap: 8.,
            previous,
            reuse_view: true,
        });
        let began = Instant::now();
        loop {
            if let Some(prepared) = service.poll() {
                assert_eq!(prepared.revision, revision);
                return prepared;
            }
            assert!(
                began.elapsed() < Duration::from_secs(5),
                "prepare worker did not return"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn completed_view_is_reused_without_a_ui_delivered_previous_result() {
        for cache_completed in [false, true] {
            let service = PrepareService::new_with_completed_cache(|| {}, cache_completed);
            let original = snapshot();
            let first = prepare(&service, original.clone(), ViewSettings::default(), None, 1);
            let mut newer = original;
            newer.files[0].height = Some(1200);
            newer.files[0].revision += 1;
            let second = prepare(&service, newer.clone(), ViewSettings::default(), None, 2);
            assert_eq!(second.view_reused, cache_completed);
            assert_eq!(Arc::ptr_eq(&first.view, &second.view), cache_completed);
            assert_eq!(*second.view, view::build(&newer, &ViewSettings::default()));
            assert_eq!(second.snapshot.files[0].height, Some(1200));
            assert!(second.layout.rectangles[&1].h > first.layout.rectangles[&1].h);
        }
    }
    #[test]
    fn metadata_only_prepare_reuses_ordering_but_rebuilds_real_geometry() {
        let service = PrepareService::new(|| {});
        let original = snapshot();
        let first = prepare(&service, original.clone(), ViewSettings::default(), None, 1);
        let mut changed = original;
        changed.files[0].height = Some(1200);
        changed.files[0].duration_ms = Some(9000);
        changed.files[0].revision += 1;
        changed.files[1].error = Some("test decode failure".into());
        changed.directories[0].state = ScanState::Partial;
        changed.directories[0].subtree_complete = false;
        let second = prepare(
            &service,
            changed.clone(),
            ViewSettings::default(),
            Some(first.clone()),
            2,
        );
        assert!(second.view_reused);
        assert!(Arc::ptr_eq(&first.view, &second.view));
        assert!(Arc::ptr_eq(&first.lookup, &second.lookup));
        assert_eq!(
            *second.view,
            view::build(&changed, &ViewSettings::default())
        );
        assert_eq!(second.snapshot.files[0].height, Some(1200));
        assert!(second.layout.rectangles[&1].h > first.layout.rectangles[&1].h);
    }

    #[test]
    fn changed_membership_version_root_filter_or_directory_never_reuses_old_ordering() {
        let service = PrepareService::new(|| {});
        let original = snapshot();
        let first = prepare(&service, original.clone(), ViewSettings::default(), None, 1);
        for case in 0..10 {
            let mut changed = original.clone();
            let mut settings = ViewSettings::default();
            match case {
                0 => {
                    changed.files.pop();
                }
                1 => {
                    changed.files[0].file.fingerprint.modified_ns += 1;
                }
                2 => {
                    changed.files[0].file.path = PathBuf::from("root/renamed.png");
                }
                3 => {
                    changed.files[0].file.kind = MediaKind::Video;
                }
                4 => {
                    changed.generation += 1;
                }
                5 => {
                    changed.root = PathBuf::from("different-root");
                }
                6 => {
                    settings.descending = true;
                }
                7 => {
                    settings.filter = view::MediaFilter::Videos;
                }
                8 => {
                    let mut directory = changed.directories[0].clone();
                    directory.path.push("empty");
                    changed.directories.push(directory);
                }
                9 => {
                    changed.files.swap(0, 1);
                }
                _ => unreachable!(),
            }
            let prepared = prepare(
                &service,
                changed.clone(),
                settings.clone(),
                Some(first.clone()),
                case + 2,
            );
            assert!(
                !prepared.view_reused,
                "case {case} reused incompatible input"
            );
            assert!(!Arc::ptr_eq(&first.view, &prepared.view));
            assert_eq!(
                *prepared.view,
                view::build(&changed, &settings),
                "case {case}"
            );
            for (index, record) in changed.files.iter().enumerate() {
                assert_eq!(prepared.lookup[&record.id], index);
            }
        }
    }
}
