//! Single-owner SQLite repository. This module never deletes or edits source media.
use crate::{domain::*, scanner::ScanEvent};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub type IndexResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const TREE: &str = "WITH RECURSIVE tree(path) AS (SELECT path FROM directories WHERE path=?1 UNION ALL SELECT d.path FROM directories d JOIN tree t ON d.parent=t.path)";
struct Session {
    generation: u64,
    scan_id: i64,
    root: PathBuf,
    finished: bool,
    errors: Vec<String>,
    elapsed_ms: u64,
}
pub struct MetadataWrite {
    pub id: MediaId,
    pub fingerprint: Fingerprint,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}
pub struct Repository {
    db: Connection,
    active: Option<Session>,
    recycled: HashMap<Vec<u8>, Fingerprint>,
}

impl Repository {
    pub fn open(path: &Path) -> IndexResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(path)?)
    }
    pub fn memory() -> IndexResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(db: Connection) -> IndexResult<Self> {
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;",
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            return Err(crate::ui_format!(msg_7a61717abbc6, version = version).into());
        }
        if version == 0 {
            db.execute_batch("BEGIN;
            CREATE TABLE directories(path BLOB PRIMARY KEY,parent BLOB,display TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'unknown');
            CREATE INDEX directories_parent ON directories(parent);
            CREATE TABLE scans(id INTEGER PRIMARY KEY AUTOINCREMENT,root BLOB NOT NULL,started_ns INTEGER NOT NULL,finished INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE media(id INTEGER PRIMARY KEY AUTOINCREMENT,path BLOB NOT NULL UNIQUE,parent BLOB NOT NULL REFERENCES directories(path) ON DELETE CASCADE,
                kind TEXT NOT NULL CHECK(kind IN ('image','video')),bytes INTEGER NOT NULL,modified_ns INTEGER NOT NULL,seen_scan INTEGER NOT NULL,
                width INTEGER,height INTEGER,duration_ms INTEGER,revision INTEGER NOT NULL DEFAULT 0,error TEXT);
            CREATE INDEX media_parent ON media(parent);
            CREATE TABLE thumbnails(cache_key TEXT PRIMARY KEY,media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
                variant TEXT NOT NULL,fingerprint BLOB NOT NULL,cache_path BLOB NOT NULL,bytes INTEGER NOT NULL,last_access INTEGER NOT NULL);
            PRAGMA user_version=1; COMMIT;")?;
        }
        Ok(Self {
            db,
            active: None,
            recycled: HashMap::new(),
        })
    }
    fn ensure_dir(db: &Connection, path: &Path) -> IndexResult<()> {
        let parent = path.parent().map(path_blob);
        db.execute("INSERT INTO directories(path,parent,display) VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET parent=excluded.parent,display=excluded.display",params![path_blob(path),parent,path.to_string_lossy()])?;
        Ok(())
    }
    pub fn begin_scan(&mut self, root: &Path, generation: u64) -> IndexResult<()> {
        self.recycled.clear();
        let tx = self.db.transaction()?;
        Self::ensure_dir(&tx, root)?;
        tx.execute(&format!("{TREE} UPDATE directories SET state='scanning' WHERE path IN (SELECT path FROM tree)"),[path_blob(root)])?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
            .min(i64::MAX as u128) as i64;
        tx.execute(
            "INSERT INTO scans(root,started_ns) VALUES(?1,?2)",
            params![path_blob(root), now],
        )?;
        let scan_id = tx.last_insert_rowid();
        tx.commit()?;
        self.active = Some(Session {
            generation,
            scan_id,
            root: root.to_owned(),
            finished: false,
            errors: Vec::new(),
            elapsed_ms: 0,
        });
        Ok(())
    }
    pub fn apply(&mut self, event: ScanEvent) -> IndexResult<bool> {
        let generation = match &event {
            ScanEvent::Batch { generation, .. }
            | ScanEvent::Directory { generation, .. }
            | ScanEvent::Error { generation, .. }
            | ScanEvent::Finished { generation, .. } => *generation,
        };
        let Some(session) = &self.active else {
            return Ok(false);
        };
        if session.generation != generation || session.finished {
            return Ok(false);
        }
        let scan_id = session.scan_id;
        match event {
            ScanEvent::Batch { files, .. } => {
                let tx = self.db.transaction()?;
                for file in files {
                    if self.recycled.get(&path_blob(&file.path)) == Some(&file.fingerprint) {
                        continue;
                    }
                    Self::ensure_dir(&tx, &file.parent)?;
                    let path = path_blob(&file.path);
                    // Changed source data invalidates only its derived metadata and thumbnail references.
                    tx.execute("DELETE FROM thumbnails WHERE media_id IN (SELECT id FROM media WHERE path=?1 AND (bytes<>?2 OR modified_ns<>?3))",params![path,file.fingerprint.bytes,file.fingerprint.modified_ns])?;
                    tx.prepare_cached("INSERT INTO media(path,parent,kind,bytes,modified_ns,seen_scan) VALUES(?1,?2,?3,?4,?5,?6)
                        ON CONFLICT(path) DO UPDATE SET parent=excluded.parent,kind=excluded.kind,bytes=excluded.bytes,modified_ns=excluded.modified_ns,seen_scan=excluded.seen_scan,
                        width=CASE WHEN media.bytes<>excluded.bytes OR media.modified_ns<>excluded.modified_ns THEN NULL ELSE media.width END,
                        height=CASE WHEN media.bytes<>excluded.bytes OR media.modified_ns<>excluded.modified_ns THEN NULL ELSE media.height END,
                        duration_ms=CASE WHEN media.bytes<>excluded.bytes OR media.modified_ns<>excluded.modified_ns THEN NULL ELSE media.duration_ms END,
                        error=CASE WHEN media.bytes<>excluded.bytes OR media.modified_ns<>excluded.modified_ns THEN NULL ELSE media.error END,
                        revision=media.revision+CASE WHEN media.bytes<>excluded.bytes OR media.modified_ns<>excluded.modified_ns THEN 1 ELSE 0 END")?
                        .execute(params![path,path_blob(&file.parent),kind_text(file.kind),file.fingerprint.bytes,file.fingerprint.modified_ns,scan_id])?;
                }
                tx.commit()?;
            }
            ScanEvent::Directory {
                path,
                children,
                complete,
                ..
            } => {
                let tx = self.db.transaction()?;
                Self::ensure_dir(&tx, &path)?;
                for child in &children {
                    Self::ensure_dir(&tx, child)?;
                }
                if complete {
                    tx.execute(
                        "DELETE FROM media WHERE parent=?1 AND seen_scan<>?2",
                        params![path_blob(&path), scan_id],
                    )?;
                    let old: Vec<Vec<u8>> = {
                        let mut q = tx.prepare("SELECT path FROM directories WHERE parent=?1")?;
                        q.query_map([path_blob(&path)], |r| r.get(0))?
                            .collect::<rusqlite::Result<_>>()?
                    };
                    let present: std::collections::HashSet<_> =
                        children.iter().map(|p| path_blob(p)).collect();
                    for missing in old.into_iter().filter(|p| !present.contains(p)) {
                        // Prune the cached subtree only after its parent was completely enumerated.
                        tx.execute(
                            &format!(
                                "{TREE} DELETE FROM media WHERE parent IN (SELECT path FROM tree)"
                            ),
                            [&missing],
                        )?;
                        tx.execute(&format!("{TREE} DELETE FROM directories WHERE path IN (SELECT path FROM tree)"),[&missing])?;
                    }
                }
                tx.execute(
                    "UPDATE directories SET state=?2 WHERE path=?1",
                    params![
                        path_blob(&path),
                        if complete { "complete" } else { "partial" }
                    ],
                )?;
                tx.commit()?;
            }
            ScanEvent::Error { path, message, .. } => self
                .active
                .as_mut()
                .unwrap()
                .errors
                .push(format!("{}: {message}", path.display())),
            ScanEvent::Finished {
                cancelled, elapsed, ..
            } => {
                let session = self.active.as_mut().unwrap();
                if !cancelled {
                    self.db.execute(&format!("{TREE} UPDATE directories SET state='partial' WHERE path IN (SELECT path FROM tree) AND state IN ('unknown','scanning')"),[path_blob(&session.root)])?;
                    self.db
                        .execute("UPDATE scans SET finished=1 WHERE id=?1", [scan_id])?;
                }
                session.finished = true;
                session.elapsed_ms = elapsed.as_millis().min(u64::MAX as u128) as u64;
            }
        }
        Ok(true)
    }
    /// Idempotent database reconciliation after a confirmed Shell recycle receipt.
    /// The tombstone stops an already-enumerated batch from resurrecting this version.
    /// A new scan clears it, allowing an externally restored same-version file to return.
    pub fn remove_recycled(
        &mut self,
        id: MediaId,
        path: &Path,
        fingerprint: &Fingerprint,
    ) -> IndexResult<bool> {
        let path = path_blob(path);
        let removed = self.db.execute(
            "DELETE FROM media WHERE id=?1 AND path=?2 AND bytes=?3 AND modified_ns=?4",
            params![id, &path, fingerprint.bytes, fingerprint.modified_ns],
        )?;
        self.recycled.insert(path, fingerprint.clone());
        Ok(removed > 0)
    }
    pub fn snapshot(&self) -> IndexResult<LibrarySnapshot> {
        let session = self
            .active
            .as_ref()
            .ok_or(crate::ui_text!(msg_2d6bfee39ffb))?;
        self.snapshot_for(&session.root)
    }
    /// A view can select a real descendant while the authorized tree scan keeps
    /// ancestor/sibling counts current. This never switches the scanner session.
    pub fn snapshot_for(&self, view_root: &Path) -> IndexResult<LibrarySnapshot> {
        let session = self
            .active
            .as_ref()
            .ok_or(crate::ui_text!(msg_2d6bfee39ffb))?;
        if !view_root.starts_with(&session.root) {
            return Err(crate::ui_text!(msg_422266baa549).into());
        }
        let root = path_blob(view_root);
        let mut q=self.db.prepare(&format!("{TREE} SELECT m.id,m.path,m.parent,m.kind,m.bytes,m.modified_ns,m.width,m.height,m.duration_ms,m.revision,m.error FROM media m JOIN tree t ON m.parent=t.path"))?;
        let raw = q.query_map([&root], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, u64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<u32>>(6)?,
                r.get::<_, Option<u32>>(7)?,
                r.get::<_, Option<u64>>(8)?,
                r.get::<_, u64>(9)?,
                r.get::<_, Option<String>>(10)?,
            ))
        })?;
        let mut files = Vec::new();
        let mut direct: HashMap<PathBuf, Counts> = HashMap::new();
        for row in raw {
            let (
                id,
                path,
                parent,
                kind,
                bytes,
                modified_ns,
                width,
                height,
                duration_ms,
                revision,
                error,
            ) = row?;
            let parent = path_from_blob(&parent)?;
            let kind = if kind == "image" {
                MediaKind::Image
            } else {
                MediaKind::Video
            };
            direct.entry(parent.clone()).or_default().add(kind);
            files.push(MediaRecord {
                id,
                file: MediaFile {
                    path: path_from_blob(&path)?,
                    parent,
                    kind,
                    fingerprint: Fingerprint { bytes, modified_ns },
                },
                width,
                height,
                duration_ms,
                revision,
                error,
            });
        }
        let mut q = self.db.prepare(&format!(
            "{TREE} SELECT d.path,d.parent,d.state FROM directories d JOIN tree t ON d.path=t.path"
        ))?;
        let rows = q.query_map([root], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Option<Vec<u8>>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut directories = Vec::new();
        for row in rows {
            let (path, parent, state) = row?;
            let path = path_from_blob(&path)?;
            let counts = direct.remove(&path).unwrap_or_default();
            let state = match state.as_str() {
                "complete" => ScanState::Complete,
                "partial" => ScanState::Partial,
                "scanning" => ScanState::Scanning,
                _ => ScanState::Unknown,
            };
            directories.push(DirectoryRecord {
                path,
                parent: parent.map(|b| path_from_blob(&b)).transpose()?,
                direct: counts,
                recursive: counts,
                state,
                subtree_complete: state == ScanState::Complete,
            });
        }
        // Children precede parents so each count and completeness bit is propagated exactly once.
        directories.sort_by_key(|d| std::cmp::Reverse(d.path.components().count()));
        let lookup: HashMap<PathBuf, usize> = directories
            .iter()
            .enumerate()
            .map(|(i, d)| (d.path.clone(), i))
            .collect();
        for i in 0..directories.len() {
            let counts = directories[i].recursive;
            let complete = directories[i].subtree_complete;
            if let Some(parent) = directories[i]
                .parent
                .as_ref()
                .and_then(|p| lookup.get(p))
                .copied()
            {
                directories[parent].recursive.merge(counts);
                directories[parent].subtree_complete &= complete;
            }
        }
        directories.sort_by(|a, b| {
            natural_cmp(&a.path.to_string_lossy(), &b.path.to_string_lossy())
                .then_with(|| path_blob(&a.path).cmp(&path_blob(&b.path)))
        });
        Ok(LibrarySnapshot {
            generation: session.generation,
            root: view_root.to_owned(),
            files,
            directories,
            finished: session.finished,
            errors: session.errors.clone(),
            elapsed_ms: session.elapsed_ms,
        })
    }
    pub fn write_metadata(
        &mut self,
        id: MediaId,
        fp: &Fingerprint,
        width: Option<u32>,
        height: Option<u32>,
        duration_ms: Option<u64>,
        error: Option<&str>,
    ) -> IndexResult<bool> {
        Ok(self.db.execute("UPDATE media SET width=?4,height=?5,duration_ms=?6,error=?7 WHERE id=?1 AND bytes=?2 AND modified_ns=?3 AND (width IS NOT ?4 OR height IS NOT ?5 OR duration_ms IS NOT ?6 OR error IS NOT ?7)",params![id,fp.bytes,fp.modified_ns,width,height,duration_ms,error])?>0)
    }
    pub fn write_metadata_batch(
        &mut self,
        updates: &[MetadataWrite],
        cancelled: impl Fn() -> bool,
    ) -> IndexResult<Vec<bool>> {
        if updates.len() > 64 {
            return Err("Metadata batch exceeds64 entries".into());
        }
        let tx = self.db.transaction()?;
        let mut changed = Vec::with_capacity(updates.len());
        {
            let mut statement = tx.prepare_cached("UPDATE media SET width=?4,height=?5,duration_ms=?6,error=?7 WHERE id=?1 AND bytes=?2 AND modified_ns=?3 AND (width IS NOT ?4 OR height IS NOT ?5 OR duration_ms IS NOT ?6 OR error IS NOT ?7)")?;
            for update in updates {
                if cancelled() {
                    return Err("metadata-batch-cancelled".into());
                }
                changed.push(
                    statement.execute(params![
                        update.id,
                        update.fingerprint.bytes,
                        update.fingerprint.modified_ns,
                        update.width,
                        update.height,
                        update.duration_ms,
                        update.error.as_deref()
                    ])? > 0,
                );
            }
        }
        if cancelled() {
            return Err("metadata-batch-cancelled".into());
        }
        tx.commit()?;
        Ok(changed)
    }
    pub fn integrity_check(&self) -> IndexResult<String> {
        Ok(self
            .db
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?)
    }
    pub fn total_media(&self) -> IndexResult<u64> {
        Ok(self
            .db
            .query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0))?)
    }
    pub fn record_for_path(&self, path: &Path) -> IndexResult<Option<MediaId>> {
        Ok(self
            .db
            .query_row(
                "SELECT id FROM media WHERE path=?1",
                [path_blob(path)],
                |r| r.get(0),
            )
            .optional()?)
    }
}
fn kind_text(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "image",
        MediaKind::Video => "video",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/index-test-root")
    }
    fn file(path: &Path, bytes: u64) -> MediaFile {
        MediaFile {
            path: path.to_owned(),
            parent: path.parent().unwrap().to_owned(),
            kind: media_kind(path).unwrap(),
            fingerprint: Fingerprint {
                bytes,
                modified_ns: 10,
            },
        }
    }
    fn batch(repo: &mut Repository, g: u64, files: Vec<MediaFile>) {
        repo.apply(ScanEvent::Batch {
            generation: g,
            files,
        })
        .unwrap();
    }
    fn dir(repo: &mut Repository, g: u64, path: &Path, children: Vec<PathBuf>, complete: bool) {
        repo.apply(ScanEvent::Directory {
            generation: g,
            path: path.to_owned(),
            children,
            complete,
            counts: Counts::default(),
        })
        .unwrap();
    }
    #[test]
    fn metadata_batch_preserves_order_versions_error_recovery_and_cancel_rollback() {
        let root = root();
        let source = file(&root.join("batch.jpg"), 10);
        let mut repo = Repository::memory().unwrap();
        repo.begin_scan(&root, 1).unwrap();
        batch(&mut repo, 1, vec![source.clone()]);
        let id = repo.snapshot().unwrap().files[0].id;
        let update = |width, error: Option<&str>, stale| MetadataWrite {
            id,
            fingerprint: if stale {
                Fingerprint {
                    bytes: 999,
                    modified_ns: 10,
                }
            } else {
                source.fingerprint.clone()
            },
            width: Some(width),
            height: Some(480),
            duration_ms: Some(1200),
            error: error.map(str::to_owned),
        };
        let calls = std::cell::Cell::new(0);
        let cancelled = repo.write_metadata_batch(
            &[update(400, None, false), update(500, None, false)],
            || {
                calls.set(calls.get() + 1);
                calls.get() >= 2
            },
        );
        assert!(cancelled.is_err());
        assert_eq!(repo.snapshot().unwrap().files[0].width, None);
        let changed = repo
            .write_metadata_batch(
                &[
                    update(400, Some("old error"), false),
                    update(640, None, false),
                    update(999, None, true),
                ],
                || false,
            )
            .unwrap();
        assert_eq!(changed, vec![true, true, false]);
        let result = repo.snapshot().unwrap();
        assert_eq!(result.files[0].width, Some(640));
        assert_eq!(result.files[0].duration_ms, Some(1200));
        assert_eq!(result.files[0].error, None);
        assert_eq!(repo.integrity_check().unwrap(), "ok");
    }

    #[test]
    fn scoped_snapshot_uses_real_parent_tree_and_rejects_prefix_siblings() {
        let root = root();
        let child = root.join("a");
        let sibling = root.join("b");
        let mut repo = Repository::memory().unwrap();
        repo.begin_scan(&root, 1).unwrap();
        batch(
            &mut repo,
            1,
            vec![
                file(&child.join("one.jpg"), 10),
                file(&child.join("deep/two.mov"), 20),
                file(&sibling.join("three.png"), 30),
            ],
        );
        dir(&mut repo, 1, &root, vec![child.clone(), sibling], true);
        dir(&mut repo, 1, &child, vec![child.join("deep")], true);
        dir(&mut repo, 1, &child.join("deep"), vec![], true);
        let snapshot = repo.snapshot_for(&child).unwrap();
        assert_eq!(snapshot.files.len(), 2);
        assert_eq!(
            snapshot
                .directories
                .iter()
                .find(|d| d.path == child)
                .unwrap()
                .recursive,
            Counts {
                images: 1,
                videos: 1
            }
        );
        assert_eq!(repo.snapshot().unwrap().files.len(), 3);
        assert!(repo
            .snapshot_for(&root.with_file_name("index-test-root2"))
            .is_err());
    }
    #[test]
    fn recycled_result_is_idempotent_and_stale_scan_cannot_resurrect_it() {
        let root = root();
        let path = root.join("one.jpg");
        let source = file(&path, 10);
        let mut repo = Repository::memory().unwrap();
        repo.begin_scan(&root, 1).unwrap();
        batch(&mut repo, 1, vec![source.clone()]);
        let id = repo.record_for_path(&path).unwrap().unwrap();
        let wrong = Fingerprint {
            bytes: 11,
            modified_ns: 10,
        };
        assert!(!repo.remove_recycled(id, &path, &wrong).unwrap());
        assert_eq!(repo.snapshot().unwrap().files.len(), 1);
        assert!(repo
            .remove_recycled(id, &path, &source.fingerprint)
            .unwrap());
        assert!(!repo
            .remove_recycled(id, &path, &source.fingerprint)
            .unwrap());
        batch(&mut repo, 1, vec![source.clone()]);
        assert!(repo.snapshot().unwrap().files.is_empty());
        repo.begin_scan(&root, 2).unwrap();
        batch(&mut repo, 2, vec![source]);
        assert_eq!(repo.snapshot().unwrap().files.len(), 1);
    }
    #[test]
    fn overlapping_roots_keep_ids_and_aggregate_once() {
        let root = root();
        let child = root.join("图集");
        let p = child.join("1.heic");
        let mut repo = Repository::memory().unwrap();
        repo.begin_scan(&root, 1).unwrap();
        batch(&mut repo, 1, vec![file(&p, 10)]);
        dir(&mut repo, 1, &root, vec![child.clone()], true);
        dir(&mut repo, 1, &child, vec![], true);
        let id = repo.record_for_path(&p).unwrap().unwrap();
        let snap = repo.snapshot().unwrap();
        assert_eq!(
            snap.directories
                .iter()
                .find(|d| d.path == root)
                .unwrap()
                .recursive
                .images,
            1
        );
        repo.begin_scan(&child, 2).unwrap();
        batch(&mut repo, 2, vec![file(&p, 10)]);
        dir(&mut repo, 2, &child, vec![], true);
        assert_eq!(repo.total_media().unwrap(), 1);
        assert_eq!(repo.record_for_path(&p).unwrap(), Some(id));
        assert_eq!(repo.integrity_check().unwrap(), "ok");
    }
    #[test]
    fn cleanup_only_complete_directory_and_prune_missing_child() {
        let root = root();
        let child = root.join("sub");
        let p = root.join("1.jpg");
        let q = child.join("2.mov");
        let mut repo = Repository::memory().unwrap();
        repo.begin_scan(&root, 1).unwrap();
        batch(&mut repo, 1, vec![file(&p, 1), file(&q, 1)]);
        dir(&mut repo, 1, &root, vec![child.clone()], true);
        dir(&mut repo, 1, &child, vec![], true);
        repo.begin_scan(&root, 2).unwrap();
        dir(&mut repo, 2, &root, vec![], false);
        assert_eq!(repo.total_media().unwrap(), 2);
        assert!(
            !repo
                .snapshot()
                .unwrap()
                .directories
                .iter()
                .find(|d| d.path == root)
                .unwrap()
                .subtree_complete
        );
        repo.begin_scan(&root, 3).unwrap();
        dir(&mut repo, 3, &root, vec![], true);
        assert_eq!(repo.total_media().unwrap(), 0);
        assert_eq!(repo.snapshot().unwrap().directories.len(), 1);
    }
    #[test]
    fn stale_generation_never_changes_index() {
        let root = root();
        let mut repo = Repository::memory().unwrap();
        repo.begin_scan(&root, 5).unwrap();
        assert!(!repo
            .apply(ScanEvent::Batch {
                generation: 4,
                files: vec![file(&root.join("old.jpg"), 1)]
            })
            .unwrap());
        assert_eq!(repo.total_media().unwrap(), 0);
    }
    #[test]
    fn changed_fingerprint_invalidates_metadata_and_late_result() {
        let root = root();
        let p = root.join("1.jpg");
        let mut repo = Repository::memory().unwrap();
        repo.begin_scan(&root, 1).unwrap();
        batch(&mut repo, 1, vec![file(&p, 1)]);
        let id = repo.record_for_path(&p).unwrap().unwrap();
        let fp = file(&p, 1).fingerprint;
        assert!(repo
            .write_metadata(id, &fp, Some(100), Some(200), None, None)
            .unwrap());
        batch(&mut repo, 1, vec![file(&p, 2)]);
        assert!(!repo
            .write_metadata(id, &fp, Some(999), Some(999), None, None)
            .unwrap());
        let snap = repo.snapshot().unwrap();
        assert_eq!(snap.files[0].width, None);
        assert_eq!(snap.files[0].revision, 1);
        assert_eq!(snap.files[0].id, id);
    }
    #[test]
    fn future_schema_is_rejected_without_erasing_data() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-fixtures");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!(
            "future-schema-{}-{}.sqlite",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE preserve(value); INSERT INTO preserve VALUES(42); PRAGMA user_version=99;").unwrap();
        drop(db);
        assert!(Repository::open(&path).is_err());
        let db = Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM preserve", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            42
        );
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            99
        );
    }
    #[test]
    fn root_query_does_not_include_sibling_with_same_prefix() {
        let a = root();
        let b = a.with_file_name("index-test-root-2");
        let mut repo = Repository::memory().unwrap();
        repo.begin_scan(&a, 1).unwrap();
        batch(&mut repo, 1, vec![file(&a.join("1.jpg"), 1)]);
        repo.begin_scan(&b, 2).unwrap();
        batch(&mut repo, 2, vec![file(&b.join("2.mov"), 1)]);
        repo.begin_scan(&a, 3).unwrap();
        let snap = repo.snapshot().unwrap();
        assert_eq!(snap.files.len(), 1);
        assert!(snap.files[0].file.path.ends_with("1.jpg"));
        assert_eq!(repo.total_media().unwrap(), 2);
    }
}
