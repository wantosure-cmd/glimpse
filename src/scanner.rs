//! Iterative directory scan: no decoding, no links, bounded backpressure and cancellation.
use crate::domain::{Counts, Fingerprint, MediaFile, media_kind};
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, TrySendError},
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

#[derive(Debug)]
pub enum ScanEvent {
    Batch {
        generation: u64,
        files: Vec<MediaFile>,
    },
    Directory {
        generation: u64,
        path: PathBuf,
        counts: Counts,
        complete: bool,
        children: Vec<PathBuf>,
    },
    Error {
        generation: u64,
        path: PathBuf,
        message: String,
    },
    Finished {
        generation: u64,
        cancelled: bool,
        elapsed: Duration,
    },
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn send(
    tx: &SyncSender<ScanEvent>,
    current: &AtomicU64,
    generation: u64,
    mut event: ScanEvent,
) -> bool {
    loop {
        if current.load(Ordering::Acquire) != generation {
            return false;
        }
        match tx.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(returned)) => {
                event = returned;
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

pub fn scan(root: PathBuf, generation: u64, current: Arc<AtomicU64>, tx: SyncSender<ScanEvent>) {
    scan_excluding(root, generation, current, tx, Vec::new());
}

fn scan_excluding(
    root: PathBuf,
    generation: u64,
    current: Arc<AtomicU64>,
    tx: SyncSender<ScanEvent>,
    excluded: Vec<PathBuf>,
) {
    let began = Instant::now();
    let root = match crate::media_path::resolve_read(&root) {
        Ok(p) => p,
        Err(e) => {
            let _ = send(
                &tx,
                &current,
                generation,
                ScanEvent::Error {
                    generation,
                    path: root,
                    message: e.to_string(),
                },
            );
            let _ = send(
                &tx,
                &current,
                generation,
                ScanEvent::Finished {
                    generation,
                    cancelled: false,
                    elapsed: began.elapsed(),
                },
            );
            return;
        }
    };
    let mut queue = VecDeque::from([root]);
    let mut batch = Vec::with_capacity(100);
    let mut last_batch = Instant::now();
    while let Some(dir) = queue.pop_front() {
        if current.load(Ordering::Acquire) != generation {
            return;
        }
        let mut counts = Counts::default();
        let mut children = Vec::new();
        let mut complete = true;
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                if !send(
                    &tx,
                    &current,
                    generation,
                    ScanEvent::Error {
                        generation,
                        path: dir.clone(),
                        message: e.to_string(),
                    },
                ) {
                    return;
                }
                if !send(
                    &tx,
                    &current,
                    generation,
                    ScanEvent::Directory {
                        generation,
                        path: dir,
                        counts,
                        complete: false,
                        children: vec![],
                    },
                ) {
                    return;
                }
                continue;
            }
        };
        for entry in entries {
            if current.load(Ordering::Acquire) != generation {
                return;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    complete = false;
                    if !send(
                        &tx,
                        &current,
                        generation,
                        ScanEvent::Error {
                            generation,
                            path: dir.clone(),
                            message: e.to_string(),
                        },
                    ) {
                        return;
                    }
                    continue;
                }
            };
            let path = entry.path();
            if excluded.iter().any(|excluded| path.starts_with(excluded)) {
                continue;
            }
            // Windows directory enumeration already provides these attributes.
            // Avoid one extra SMB stat per entry. DirEntry::metadata does not
            // follow symlinks; keep the reparse-point rejection below intact.
            // This is a scan snapshot, not authorization for reads or deletion:
            // those paths independently check the current source fingerprint.
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    complete = false;
                    if !send(
                        &tx,
                        &current,
                        generation,
                        ScanEvent::Error {
                            generation,
                            path,
                            message: e.to_string(),
                        },
                    ) {
                        return;
                    }
                    continue;
                }
            };
            if is_link(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                children.push(path.clone());
                queue.push_back(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            if let Some(kind) = media_kind(&path) {
                let modified_ns = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
                    .unwrap_or(0);
                counts.add(kind);
                batch.push(MediaFile {
                    path,
                    parent: dir.clone(),
                    kind,
                    fingerprint: Fingerprint {
                        bytes: metadata.len(),
                        modified_ns,
                    },
                });
                if batch.len() >= 100 || last_batch.elapsed() >= Duration::from_millis(50) {
                    if !send(
                        &tx,
                        &current,
                        generation,
                        ScanEvent::Batch {
                            generation,
                            files: std::mem::replace(&mut batch, Vec::with_capacity(100)),
                        },
                    ) {
                        return;
                    }
                    last_batch = Instant::now();
                }
            }
        }
        // All files in this directory must be committed before completion permits cleanup.
        if !batch.is_empty()
            && !send(
                &tx,
                &current,
                generation,
                ScanEvent::Batch {
                    generation,
                    files: std::mem::replace(&mut batch, Vec::with_capacity(100)),
                },
            )
        {
            return;
        }
        if !send(
            &tx,
            &current,
            generation,
            ScanEvent::Directory {
                generation,
                path: dir,
                counts,
                complete,
                children,
            },
        ) {
            return;
        }
    }
    let _ = send(
        &tx,
        &current,
        generation,
        ScanEvent::Finished {
            generation,
            cancelled: false,
            elapsed: began.elapsed(),
        },
    );
}

pub fn start(
    root: &Path,
    generation: u64,
    current: Arc<AtomicU64>,
) -> std::sync::mpsc::Receiver<ScanEvent> {
    start_excluding(root, generation, current, Vec::new())
}

pub fn start_excluding(
    root: &Path,
    generation: u64,
    current: Arc<AtomicU64>,
    excluded: Vec<PathBuf>,
) -> std::sync::mpsc::Receiver<ScanEvent> {
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    let root = root.to_owned();
    std::thread::spawn(move || scan_excluding(root, generation, current, tx, excluded));
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> PathBuf {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(format!("{name}-{}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn recursive_counts_and_real_parent_groups() {
        let root = fixture("scan");
        fs::create_dir_all(root.join("图集/视频")).unwrap();
        fs::write(root.join("1.JPG"), b"test").unwrap();
        fs::write(root.join("图集/2.HEIC"), b"test").unwrap();
        fs::write(root.join("图集/视频/3.MOV"), b"test").unwrap();
        fs::write(root.join("ignored.zip"), b"test").unwrap();
        let rx = start(&root, 7, Arc::new(AtomicU64::new(7)));
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        let mut finished = false;
        while let Ok(event) = rx.recv_timeout(Duration::from_secs(5)) {
            match event {
                ScanEvent::Batch { files: batch, .. } => files.extend(batch),
                ScanEvent::Directory {
                    counts, complete, ..
                } => {
                    assert!(complete);
                    dirs.push(counts);
                }
                ScanEvent::Finished { .. } => {
                    finished = true;
                    break;
                }
                ScanEvent::Error { message, .. } => panic!("{message}"),
            }
        }
        assert!(finished);
        assert_eq!(files.len(), 3);
        assert_eq!(dirs.len(), 3);
        assert_eq!(dirs.iter().map(|d| d.images).sum::<u64>(), 2);
        assert_eq!(dirs.iter().map(|d| d.videos).sum::<u64>(), 1);
        assert!(files.iter().any(|f| f.parent.ends_with("图集/视频")));
    }
    #[test]
    fn cancellation_breaks_backpressure_without_receiver_drain() {
        let current = Arc::new(AtomicU64::new(4));
        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
        let token = current.clone();
        let task = std::thread::spawn(move || {
            send(
                &tx,
                &token,
                4,
                ScanEvent::Batch {
                    generation: 4,
                    files: vec![],
                },
            );
            send(
                &tx,
                &token,
                4,
                ScanEvent::Batch {
                    generation: 4,
                    files: vec![],
                },
            )
        });
        std::thread::sleep(Duration::from_millis(20));
        current.store(5, Ordering::Release);
        assert!(!task.join().unwrap());
    }
    #[test]
    fn missing_root_is_error_not_empty_success() {
        let root = fixture("missing").join("not-created");
        let rx = start(&root, 9, Arc::new(AtomicU64::new(9)));
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(3)).unwrap(),
            ScanEvent::Error { generation: 9, .. }
        ));
    }

    #[cfg(windows)]
    #[test]
    fn enumerated_metadata_still_excludes_directory_junctions() {
        use std::os::windows::process::CommandExt;
        let base = fixture("scan-entry-junction");
        let root = base.join("library");
        let outside = base.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join("visible.jpg"), b"visible").unwrap();
        fs::write(outside.join("must-not-scan.jpg"), b"outside").unwrap();
        let link = root.join("linked-directory");
        if !link.exists() {
            let status = std::process::Command::new("cmd.exe")
                .args(["/d", "/c"])
                // cmd builtins use cmd quoting rather than the CRT argument rules.
                // Both paths are fixed children of this generated workspace fixture.
                .raw_arg(format!(
                    "mklink /J \"{}\" \"{}\"",
                    link.display(),
                    outside.display()
                ))
                .creation_flags(0x08000000)
                .output()
                .unwrap();
            assert!(status.status.success(), "{:?}", status);
        }
        let entry = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .find(|entry| entry.path() == link)
            .unwrap();
        assert!(is_link(&entry.metadata().unwrap()));
        let rx = start(&root, 17, Arc::new(AtomicU64::new(17)));
        let mut files = Vec::new();
        let mut directories = 0;
        loop {
            match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
                ScanEvent::Batch { files: batch, .. } => files.extend(batch),
                ScanEvent::Directory { children, .. } => {
                    assert!(children.is_empty());
                    directories += 1;
                }
                ScanEvent::Finished { cancelled, .. } => {
                    assert!(!cancelled);
                    break;
                }
                ScanEvent::Error { message, .. } => panic!("{message}"),
            }
        }
        assert_eq!(directories, 1);
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("visible.jpg"));
        assert_eq!(files[0].fingerprint.bytes, 7);
        assert_eq!(
            fs::read(outside.join("must-not-scan.jpg")).unwrap(),
            b"outside"
        );
    }
}
