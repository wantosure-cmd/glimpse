//! One index-owning worker, cancellable scanner, bounded snapshots. UI does no disk IO.
use crate::{
    domain::{Fingerprint, LibrarySnapshot, MediaId},
    index::{MetadataWrite, Repository},
    scanner::{self, ScanEvent},
};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, TrySendError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

enum Command {
    Open {
        generation: u64,
        root: PathBuf,
        tree_root: Option<PathBuf>,
    },
    Stop,
    Recycled(crate::trash::TrashRequest),
    Metadata {
        generation: u64,
        id: MediaId,
        fingerprint: Fingerprint,
        width: Option<u32>,
        height: Option<u32>,
        duration_ms: Option<u64>,
        error: Option<String>,
    },
}
#[derive(Debug)]
pub enum LibraryEvent {
    Snapshot(Arc<LibrarySnapshot>),
    Tree(Arc<LibrarySnapshot>),
    Error { generation: u64, message: String },
    IndexRecovered { backup: PathBuf },
}
impl LibraryEvent {
    pub fn generation(&self) -> u64 {
        match self {
            Self::Snapshot(s) | Self::Tree(s) => s.generation,
            Self::Error { generation, .. } => *generation,
            Self::IndexRecovered { .. } => 0,
        }
    }
}
pub struct LibraryService {
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<LibraryEvent>,
    current: Arc<AtomicU64>,
    worker: Option<thread::JoinHandle<()>>,
    metadata_stats: Arc<[AtomicU64; 2]>,
}
impl LibraryService {
    pub fn new(db_path: PathBuf, wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new_excluding(db_path, Vec::new(), wake)
    }
    pub fn new_excluding(
        db_path: PathBuf,
        excluded: Vec<PathBuf>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self::start(db_path, excluded, wake, false)
    }
    pub fn new_recovering_excluding(
        db_path: PathBuf,
        excluded: Vec<PathBuf>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self::start(db_path, excluded, wake, true)
    }
    fn start(
        db_path: PathBuf,
        excluded: Vec<PathBuf>,
        wake: impl Fn() + Send + Sync + 'static,
        recover: bool,
    ) -> Self {
        let (commands, rx) = mpsc::channel();
        let (tx, events) = mpsc::sync_channel(3);
        let current = Arc::new(AtomicU64::new(0));
        let token = current.clone();
        let metadata_stats = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
        let worker_stats = metadata_stats.clone();
        let batch_metadata = std::env::var_os("GLIMPSE_SINGLE_METADATA_WRITES").as_deref()
            != Some(std::ffi::OsStr::new("1"));
        let worker = thread::spawn(move || {
            work(
                db_path,
                rx,
                tx,
                token,
                Arc::new(wake),
                excluded,
                recover,
                batch_metadata,
                worker_stats,
            )
        });
        Self {
            commands,
            events,
            current,
            worker: Some(worker),
            metadata_stats,
        }
    }
    pub fn open(&self, root: &Path) -> u64 {
        self.open_view(root, None)
    }
    pub fn open_view(&self, root: &Path, tree_root: Option<&Path>) -> u64 {
        let generation = self.current.fetch_add(1, Ordering::AcqRel) + 1;
        let _ = self.commands.send(Command::Open {
            generation,
            root: root.to_owned(),
            tree_root: tree_root.map(Path::to_owned),
        });
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        generation
    }
    pub fn recycled(&self, request: crate::trash::TrashRequest) {
        let _ = self.commands.send(Command::Recycled(request));
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
    /// Attempted transactions and submitted statements, not successful commits.
    pub fn metadata_batch_counts(&self) -> [u64; 2] {
        [
            self.metadata_stats[0].load(Ordering::Relaxed),
            self.metadata_stats[1].load(Ordering::Relaxed),
        ]
    }
    pub fn current_generation(&self) -> u64 {
        self.current.load(Ordering::Acquire)
    }
    pub fn update_metadata(
        &self,
        generation: u64,
        id: MediaId,
        fingerprint: Fingerprint,
        width: Option<u32>,
        height: Option<u32>,
        duration_ms: Option<u64>,
        error: Option<String>,
    ) {
        let _ = self.commands.send(Command::Metadata {
            generation,
            id,
            fingerprint,
            width,
            height,
            duration_ms,
            error,
        });
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
    pub fn try_recv(&self) -> Option<LibraryEvent> {
        let event = self.events.try_recv().ok();
        if event.is_some() {
            if let Some(worker) = &self.worker {
                worker.thread().unpark();
            }
        }
        event
    }
    pub fn recv_timeout(&self, timeout: Duration) -> Result<LibraryEvent, mpsc::RecvTimeoutError> {
        let event = self.events.recv_timeout(timeout);
        if event.is_ok() {
            if let Some(worker) = &self.worker {
                worker.thread().unpark();
            }
        }
        event
    }
    pub fn shutdown(mut self) {
        self.current.store(u64::MAX, Ordering::Release);
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}
impl Drop for LibraryService {
    fn drop(&mut self) {
        self.current.store(u64::MAX, Ordering::Release);
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
}

fn queue_error(pending: &mut VecDeque<LibraryEvent>, generation: u64, message: String) {
    pending.clear();
    pending.push_back(LibraryEvent::Error {
        generation,
        message,
    });
}
fn queue_snapshots(
    repository: &Repository,
    view_root: &Path,
    warning: Option<&str>,
    tree_changed: bool,
    pending: &mut VecDeque<LibraryEvent>,
    generation: u64,
) {
    let needs_tree = tree_changed || pending.iter().any(|e| matches!(e, LibraryEvent::Tree(_)));
    let result = (|| -> Result<VecDeque<LibraryEvent>, String> {
        let mut next = VecDeque::new();
        let tree = if needs_tree {
            Some(Arc::new(repository.snapshot().map_err(|e| e.to_string())?))
        } else {
            None
        };
        if let Some(tree) = &tree {
            next.push_back(LibraryEvent::Tree(tree.clone()));
        }
        let view =
            if let Some(tree) = tree.filter(|tree| tree.root == view_root && warning.is_none()) {
                tree
            } else {
                let mut view = repository
                    .snapshot_for(view_root)
                    .map_err(|e| e.to_string())?;
                if let Some(warning) = warning {
                    view.errors.push(warning.into());
                }
                Arc::new(view)
            };
        next.push_back(LibraryEvent::Snapshot(view));
        Ok(next)
    })();
    match result {
        Ok(mut next) => {
            // Publishing recovered metadata must not erase the pending error
            // that explains the failed batch/row. Keep only the latest error.
            if let Some(error) = pending.iter().rev().find_map(|event| match event {
                LibraryEvent::Error {
                    generation: event_generation,
                    message,
                } if *event_generation == generation => Some(LibraryEvent::Error {
                    generation,
                    message: message.clone(),
                }),
                _ => None,
            }) {
                next.push_front(error);
            }
            *pending = next;
        }
        Err(message) => queue_error(pending, generation, message),
    }
}
fn resolve_roots(
    root: &Path,
    tree_root: Option<&Path>,
    excluded: &[PathBuf],
) -> Result<(PathBuf, PathBuf, Option<String>), String> {
    let old_tree = tree_root
        .and_then(|p| crate::media_path::resolve_read(p).ok())
        .filter(|p| p.is_dir());
    let (view, warning) = match crate::media_path::resolve_read(root) {
        Ok(root) => {
            if !root.is_dir() {
                return Err(crate::ui_text!(msg_dfe37f1a98a9).into());
            }
            (root, None)
        }
        Err(error) => {
            if root.is_absolute() && old_tree.as_ref().is_some_and(|base| root.starts_with(base)) {
                (
                    root.to_owned(),
                    Some(crate::ui_format!(msg_b08bb9abfedb, error = error)),
                )
            } else {
                return Err(error.to_string());
            }
        }
    };
    if excluded.iter().any(|p| view.starts_with(p)) {
        return Err(crate::ui_text!(msg_c749be3bdb7f).into());
    }
    let tree = old_tree
        .filter(|base| view.starts_with(base))
        .unwrap_or_else(|| view.clone());
    if excluded.iter().any(|p| tree.starts_with(p)) {
        return Err(crate::ui_text!(msg_056033f28f91).into());
    }
    Ok((view, tree, warning))
}
fn work(
    db_path: PathBuf,
    commands: mpsc::Receiver<Command>,
    tx: mpsc::SyncSender<LibraryEvent>,
    current: Arc<AtomicU64>,
    wake: Arc<dyn Fn() + Send + Sync>,
    excluded: Vec<PathBuf>,
    recover: bool,
    batch_metadata: bool,
    metadata_stats: Arc<[AtomicU64; 2]>,
) {
    let opened = if recover {
        crate::index_recovery::open_cache(&db_path)
    } else {
        Repository::open(&db_path).map(|repo| (repo, None))
    };
    let mut repository = match opened {
        Ok((repository, backup)) => {
            if let Some(backup) = backup {
                let _ = tx.try_send(LibraryEvent::IndexRecovered { backup });
                wake();
            }
            repository
        }
        Err(error) => {
            while let Ok(command) = commands.recv() {
                match command {
                    Command::Open { generation, .. } => {
                        let _ = tx.try_send(LibraryEvent::Error {
                            generation,
                            message: error.to_string(),
                        });
                        wake();
                    }
                    Command::Stop => break,
                    _ => {}
                }
            }
            return;
        }
    };
    let excluded: Vec<_> = excluded
        .into_iter()
        .filter_map(|p| crate::media_path::resolve_read(&p).ok())
        .collect();
    let mut scan: Option<mpsc::Receiver<ScanEvent>> = None;
    let mut active_generation = 0;
    let mut dirty = false;
    let mut tree_dirty = false;
    let mut view_root = None;
    let mut view_warning = None;
    let mut last_publish = Instant::now();
    let mut awaiting_first_metadata = true;
    let mut pending = VecDeque::new();
    let mut deferred_command = None;
    loop {
        let mut first_metadata_ready = false;
        loop {
            match deferred_command
                .take()
                .map(Ok)
                .unwrap_or_else(|| commands.try_recv())
            {
                Ok(Command::Stop) | Err(mpsc::TryRecvError::Disconnected) => return,
                Err(mpsc::TryRecvError::Empty) => break,
                Ok(Command::Open {
                    generation,
                    root,
                    tree_root,
                }) => {
                    if current.load(Ordering::Acquire) != generation {
                        continue;
                    }
                    scan = None;
                    dirty = false;
                    tree_dirty = false;
                    active_generation = generation;
                    awaiting_first_metadata = true;
                    first_metadata_ready = false;
                    pending.clear();
                    let result = resolve_roots(&root, tree_root.as_deref(), &excluded).and_then(
                        |(view, tree, warning)| {
                            repository
                                .begin_scan(&tree, generation)
                                .map_err(|e| e.to_string())?;
                            Ok((view, tree, warning))
                        },
                    );
                    match result {
                        Ok((view, tree, warning)) => {
                            scan = Some(scanner::start_excluding(
                                &tree,
                                generation,
                                current.clone(),
                                excluded.clone(),
                            ));
                            view_root = Some(view);
                            view_warning = warning;
                            queue_snapshots(
                                &repository,
                                view_root.as_deref().unwrap(),
                                view_warning.as_deref(),
                                true,
                                &mut pending,
                                generation,
                            );
                        }
                        Err(message) => {
                            view_root = None;
                            view_warning = None;
                            queue_error(&mut pending, generation, message);
                        }
                    }
                    last_publish = Instant::now();
                }
                Ok(Command::Recycled(request)) => {
                    match repository.remove_recycled(
                        request.id,
                        &request.path,
                        &request.fingerprint,
                    ) {
                        Ok(updated) => {
                            dirty |= updated;
                            tree_dirty |= updated;
                        }
                        Err(error) => {
                            queue_error(&mut pending, active_generation, error.to_string())
                        }
                    }
                }
                Ok(Command::Metadata {
                    generation,
                    id,
                    fingerprint,
                    width,
                    height,
                    duration_ms,
                    error,
                }) => {
                    if batch_metadata {
                        let mut updates = vec![(
                            generation,
                            MetadataWrite {
                                id,
                                fingerprint,
                                width,
                                height,
                                duration_ms,
                                error,
                            },
                        )];
                        for _ in 1..64 {
                            match commands.try_recv() {
                                Ok(Command::Metadata {
                                    generation,
                                    id,
                                    fingerprint,
                                    width,
                                    height,
                                    duration_ms,
                                    error,
                                }) => updates.push((
                                    generation,
                                    MetadataWrite {
                                        id,
                                        fingerprint,
                                        width,
                                        height,
                                        duration_ms,
                                        error,
                                    },
                                )),
                                Ok(control) => {
                                    deferred_command = Some(control);
                                    break;
                                }
                                Err(mpsc::TryRecvError::Empty) => break,
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    deferred_command = Some(Command::Stop);
                                    break;
                                }
                            }
                        }
                        let updates: Vec<_> = updates
                            .into_iter()
                            .filter_map(|(generation, update)| {
                                (generation == active_generation
                                    && generation == current.load(Ordering::Acquire))
                                .then_some(update)
                            })
                            .collect();
                        if !updates.is_empty() {
                            metadata_stats[0].fetch_add(1, Ordering::Relaxed);
                            metadata_stats[1].fetch_add(updates.len() as u64, Ordering::Relaxed);
                            match repository.write_metadata_batch(&updates, || {
                                current.load(Ordering::Acquire) != active_generation
                            }) {
                                Ok(changed) => {
                                    dirty |= changed.iter().any(|updated| *updated);
                                    if awaiting_first_metadata
                                        && changed.iter().zip(&updates).any(|(updated, row)| {
                                            *updated
                                                && row
                                                    .width
                                                    .zip(row.height)
                                                    .is_some_and(|(w, h)| w > 0 && h > 0)
                                        })
                                    {
                                        first_metadata_ready = true;
                                        awaiting_first_metadata = false;
                                    }
                                }
                                Err(error)
                                    if active_generation == current.load(Ordering::Acquire) =>
                                {
                                    queue_error(&mut pending, active_generation, error.to_string());
                                    // A failed transaction has rolled back. Preserve the
                                    // original per-row error behavior so one bad row
                                    // cannot discard unrelated usable metadata.
                                    for row in &updates {
                                        if active_generation != current.load(Ordering::Acquire) {
                                            break;
                                        }
                                        metadata_stats[0].fetch_add(1, Ordering::Relaxed);
                                        metadata_stats[1].fetch_add(1, Ordering::Relaxed);
                                        match repository.write_metadata(
                                            row.id,
                                            &row.fingerprint,
                                            row.width,
                                            row.height,
                                            row.duration_ms,
                                            row.error.as_deref(),
                                        ) {
                                            Ok(updated) => {
                                                dirty |= updated;
                                                if updated
                                                    && awaiting_first_metadata
                                                    && row
                                                        .width
                                                        .zip(row.height)
                                                        .is_some_and(|(w, h)| w > 0 && h > 0)
                                                {
                                                    first_metadata_ready = true;
                                                    awaiting_first_metadata = false;
                                                }
                                            }
                                            Err(error) => queue_error(
                                                &mut pending,
                                                active_generation,
                                                error.to_string(),
                                            ),
                                        }
                                    }
                                }
                                Err(_) => {}
                            }
                        }
                    } else {
                        if generation == active_generation
                            && generation == current.load(Ordering::Acquire)
                        {
                            metadata_stats[0].fetch_add(1, Ordering::Relaxed);
                            metadata_stats[1].fetch_add(1, Ordering::Relaxed);
                            match repository.write_metadata(
                                id,
                                &fingerprint,
                                width,
                                height,
                                duration_ms,
                                error.as_deref(),
                            ) {
                                Ok(updated) => {
                                    dirty |= updated;
                                    if updated
                                        && awaiting_first_metadata
                                        && width.zip(height).is_some_and(|(w, h)| w > 0 && h > 0)
                                    {
                                        // Publish the first usable dimensions without the normal
                                        // batch delay; subsequent metadata keeps its 200ms budget.
                                        first_metadata_ready = true;
                                        awaiting_first_metadata = false;
                                    }
                                }
                                Err(error) => {
                                    queue_error(&mut pending, generation, error.to_string())
                                }
                            }
                        }
                    }
                }
            }
            // Continue draining in the original order. A following Open/Stop/
            // Recycled command is handled before scanner work or publication.
        }
        let mut finished = false;
        let mut consumed = false;
        if active_generation != current.load(Ordering::Acquire) {
            scan = None;
            dirty = false;
            tree_dirty = false;
        }
        if let Some(receiver) = &scan {
            for _ in 0..32 {
                match receiver.try_recv() {
                    Ok(event) => {
                        consumed = true;
                        finished |= matches!(event, ScanEvent::Finished { .. });
                        match repository.apply(event) {
                            Ok(applied) => {
                                dirty |= applied;
                                tree_dirty |= applied;
                            }
                            Err(error) => {
                                queue_error(&mut pending, active_generation, error.to_string());
                                finished = true;
                            }
                        }
                        if finished {
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        if dirty
            && (finished
                || first_metadata_ready
                || last_publish.elapsed() >= Duration::from_millis(200))
        {
            if let Some(view) = &view_root {
                queue_snapshots(
                    &repository,
                    view,
                    view_warning.as_deref(),
                    tree_dirty,
                    &mut pending,
                    active_generation,
                );
            }
            dirty = false;
            tree_dirty = false;
            last_publish = Instant::now();
        }
        if finished {
            scan = None;
        }
        if let Some(event) = pending.pop_front() {
            if event.generation() == current.load(Ordering::Acquire) {
                match tx.try_send(event) {
                    Ok(()) => wake(),
                    Err(TrySendError::Full(event)) => pending.push_front(event),
                    Err(TrySendError::Disconnected(_)) => return,
                }
            }
        }
        if !consumed {
            thread::park_timeout(if scan.is_some() || !pending.is_empty() {
                Duration::from_millis(3)
            } else if dirty {
                Duration::from_millis(200).saturating_sub(last_publish.elapsed())
            } else {
                Duration::from_secs(1)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settled_pair(
        service: &LibraryService,
        generation: u64,
    ) -> (Arc<LibrarySnapshot>, Arc<LibrarySnapshot>) {
        let began = Instant::now();
        let mut view = None;
        let mut tree = None;
        loop {
            assert!(began.elapsed() < Duration::from_secs(8));
            match service.recv_timeout(Duration::from_millis(50)) {
                Ok(LibraryEvent::Snapshot(s)) if s.generation == generation && s.finished => {
                    view = Some(s)
                }
                Ok(LibraryEvent::Tree(s)) if s.generation == generation && s.finished => {
                    tree = Some(s)
                }
                Ok(LibraryEvent::Error {
                    generation: g,
                    message,
                }) if g == generation => panic!("{message}"),
                _ => {}
            }
            if let (Some(view), Some(tree)) = (&view, &tree) {
                return (view.clone(), tree.clone());
            }
        }
    }
    #[test]
    fn descendant_view_reconciles_ancestor_siblings_missing_view_and_scope_reset() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(format!(
                "tree-scope-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let base = fixture.join("media");
        let left = base.join("left");
        let right = base.join("right");
        let outside = fixture.join("other");
        for path in [left.join("deep"), right.clone(), outside.clone()] {
            std::fs::create_dir_all(path).unwrap();
        }
        let original = fixture.join("original.png");
        image::RgbaImage::from_pixel(4, 3, image::Rgba([30, 120, 220, 255]))
            .save(&original)
            .unwrap();
        let bytes = std::fs::read(&original).unwrap();
        for path in [
            left.join("a.png"),
            left.join("deep/b.png"),
            right.join("c.png"),
            outside.join("outside.png"),
        ] {
            std::fs::copy(&original, path).unwrap();
        }
        let base = std::fs::canonicalize(base).unwrap();
        let left = std::fs::canonicalize(left).unwrap();
        let right = std::fs::canonicalize(right).unwrap();
        let service = LibraryService::new(fixture.join("index.sqlite"), || {});
        let generation = service.open(&base);
        let (v, t) = settled_pair(&service, generation);
        assert!(Arc::ptr_eq(&v, &t));
        assert_eq!(v.files.len(), 3);
        let id = v
            .files
            .iter()
            .find(|r| r.file.path == left.join("a.png"))
            .unwrap()
            .id;
        let generation = service.open_view(&left, Some(&base));
        let (v, t) = settled_pair(&service, generation);
        assert_eq!(v.root, left);
        assert_eq!(v.files.len(), 2);
        assert_eq!(t.files.len(), 3);
        assert_eq!(
            v.files
                .iter()
                .find(|r| r.file.path == left.join("a.png"))
                .unwrap()
                .id,
            id
        );
        std::fs::copy(&original, left.join("deep/new.png")).unwrap();
        std::fs::copy(&original, right.join("new.png")).unwrap();
        let generation = service.open_view(&left, Some(&base));
        let (v, t) = settled_pair(&service, generation);
        assert_eq!(v.files.len(), 3);
        assert_eq!(t.files.len(), 5);
        assert_eq!(
            t.directories
                .iter()
                .find(|d| d.path == base)
                .unwrap()
                .recursive
                .images,
            5
        );
        assert_eq!(
            t.directories
                .iter()
                .find(|d| d.path == right)
                .unwrap()
                .recursive
                .images,
            2
        );
        let owned = std::fs::canonicalize(right.join("c.png")).unwrap();
        assert!(owned.starts_with(&base));
        std::fs::remove_file(owned).unwrap();
        let renamed = base.join("renamed");
        let workspace = std::fs::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures"),
        )
        .unwrap();
        assert!(left.starts_with(&workspace) && renamed.parent().unwrap().starts_with(&workspace));
        std::fs::rename(&left, &renamed).unwrap();
        let generation = service.open_view(&left, Some(&base));
        let (v, t) = settled_pair(&service, generation);
        assert!(v.files.is_empty() && !v.errors.is_empty());
        assert_eq!(t.files.len(), 4);
        assert!(!t.directories.iter().any(|d| d.path == left));
        assert_eq!(
            t.directories
                .iter()
                .find(|d| d.path == renamed)
                .unwrap()
                .recursive
                .images,
            3
        );
        let outside = std::fs::canonicalize(outside).unwrap();
        let generation = service.open_view(&outside, Some(&base));
        let (v, t) = settled_pair(&service, generation);
        assert_eq!(t.root, outside);
        assert_eq!(v.files.len(), 1);
        assert!(Arc::ptr_eq(&v, &t));
        assert_eq!(std::fs::read(original).unwrap(), bytes);
        service.shutdown();
    }
    #[test]
    fn metadata_only_publication_does_not_rebuild_unchanged_tree() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(format!(
                "tree-meta-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&root).unwrap();
        image::RgbImage::new(4, 3)
            .save(root.join("image.png"))
            .unwrap();
        let service = LibraryService::new(root.join("index.sqlite"), || {});
        let generation = service.open(&root);
        let (v, _) = settled_pair(&service, generation);
        let record = &v.files[0];
        service.update_metadata(
            generation,
            record.id,
            record.file.fingerprint.clone(),
            Some(4),
            Some(3),
            None,
            None,
        );
        let began = Instant::now();
        loop {
            assert!(began.elapsed() < Duration::from_secs(3));
            match service.recv_timeout(Duration::from_millis(50)) {
                Ok(LibraryEvent::Tree(_)) => panic!("Metadata-only update rebuilt tree"),
                Ok(LibraryEvent::Snapshot(s)) if s.files[0].width == Some(4) => break,
                _ => {}
            }
        }
        service.shutdown();
    }
    #[test]
    fn metadata_batch_failure_keeps_following_valid_feedback() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(format!(
                "metadata-batch-fault-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("generated.png");
        image::RgbImage::new(4, 3).save(&source).unwrap();
        let original = std::fs::read(&source).unwrap();
        let database = root.join("index.sqlite");
        let service = LibraryService::new(database.clone(), || {});
        let generation = service.open(&root);
        let (view, _) = settled_pair(&service, generation);
        let record = &view.files[0];
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch("CREATE TRIGGER reject_fixture_metadata BEFORE UPDATE OF width ON media WHEN NEW.width=777 BEGIN SELECT RAISE(ABORT,'generated metadata fault'); END;").unwrap();
        drop(connection);
        for width in [640, 777, 800] {
            service
                .commands
                .send(Command::Metadata {
                    generation,
                    id: record.id,
                    fingerprint: record.file.fingerprint.clone(),
                    width: Some(width),
                    height: Some(3),
                    duration_ms: None,
                    error: None,
                })
                .unwrap();
        }
        service.worker.as_ref().unwrap().thread().unpark();
        let began = Instant::now();
        let mut observed_error = false;
        loop {
            assert!(began.elapsed() < Duration::from_secs(5));
            match service.recv_timeout(Duration::from_millis(50)) {
                Ok(LibraryEvent::Error { message, .. }) => {
                    assert!(message.contains("generated metadata fault"));
                    observed_error = true;
                }
                Ok(LibraryEvent::Snapshot(s)) if s.files[0].width == Some(800) => {
                    assert!(observed_error);
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(std::fs::read(&source).unwrap(), original);
        service.shutdown();
    }
    #[test]
    fn service_switches_root_without_stale_files_and_persists_ids() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(format!(
                "library-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let a = base.join("A");
        let b = base.join("B");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("old.jpg"), b"x").unwrap();
        std::fs::write(b.join("new.mov"), b"x").unwrap();
        let service = LibraryService::new(base.join("index.sqlite"), || {});
        service.open(&a);
        let generation = service.open(&b);
        let deadline = Instant::now() + Duration::from_secs(5);
        let final_snapshot = loop {
            assert!(Instant::now() < deadline);
            if let Ok(LibraryEvent::Snapshot(s)) = service.recv_timeout(Duration::from_millis(100))
            {
                if s.generation == generation && s.finished {
                    break s;
                }
            }
        };
        assert_eq!(final_snapshot.files.len(), 1);
        assert!(final_snapshot.files[0].file.path.ends_with("new.mov"));
        let id = final_snapshot.files[0].id;
        service.shutdown();
        let service = LibraryService::new(base.join("index.sqlite"), || {});
        let generation = service.open(&b);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline);
            if let Ok(LibraryEvent::Snapshot(s)) = service.recv_timeout(Duration::from_millis(100))
            {
                if s.generation == generation && s.finished {
                    assert_eq!(s.files[0].id, id);
                    break;
                }
            }
        }
        service.shutdown();
    }
}
