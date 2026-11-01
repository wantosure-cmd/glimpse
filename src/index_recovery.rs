//! Recovery is opt-in for the application's derived index, never arbitrary --db files.
use crate::index::{IndexResult, Repository};
use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn regular_file(path: &Path) -> std::io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err(std::io::Error::other(
                        "Index recovery refuses reparse files",
                    ));
                }
            }
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(std::io::Error::other(
                    "Index recovery requires regular files",
                ));
            }
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn checked_open(path: &Path) -> IndexResult<Repository> {
    if regular_file(path)? {
        // A clearly invalid header can be classified without SQLite touching
        // hot journals/sidecars (which can otherwise yield READONLY_ROLLBACK).
        use std::io::Read;
        let mut header = [0u8; 16];
        let mut file = fs::File::open(path)?;
        let length = file.metadata()?.len();
        if length >= 16 {
            file.read_exact(&mut header)?;
        }
        if length != 0 && (length < 16 || &header != b"SQLite format 3\0") {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB),
                Some("Invalid managed index header".into()),
            )
            .into());
        }
        if length == 0 {
            return Repository::open(path);
        }
        // Read-only validation catches page corruption before normal WAL setup.
        // Future schemas and non-corruption IO/lock failures must not be rebuilt.
        let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            return Err(crate::ui_format!(msg_7a61717abbc6, version = version).into());
        }
        let check: String = db.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
                Some(check),
            )
            .into());
        }
    }
    Repository::open(path)
}

pub fn open_cache(path: &Path) -> IndexResult<(Repository, Option<PathBuf>)> {
    let error = match checked_open(path) {
        Ok(repo) => return Ok((repo, None)),
        Err(error) => error,
    };
    let corrupt = error.downcast_ref::<rusqlite::Error>().is_some_and(|e| {
        matches!(e, rusqlite::Error::SqliteFailure(code, _) if matches!(code.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase))
    });
    if !corrupt {
        return Err(error);
    }
    let parent = path
        .parent()
        .ok_or("Index cache requires a parent directory")?;
    if !regular_file(path)? {
        return Err(error);
    }
    let mut originals = Vec::new();
    for suffix in ["-wal", "-shm", "-journal", ""] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let source = PathBuf::from(name);
        if regular_file(&source)? {
            originals.push(source);
        }
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let backup = parent.join(format!(
        "index-recovery-{stamp}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&backup)?; // Unique, same filesystem; never overwrite an old backup.
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for source in originals {
        let destination = backup.join(source.file_name().ok_or("Missing cache filename")?);
        let moved_result = if destination.exists() {
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Index backup destination exists",
            ))
        } else {
            fs::rename(&source, &destination)
        };
        if let Err(move_error) = moved_result {
            let mut rollback_errors = Vec::new();
            for (from, to) in moved.iter().rev() {
                if from.exists() {
                    rollback_errors.push(format!("{} already exists", from.display()));
                } else if let Err(e) = fs::rename(to, from) {
                    rollback_errors.push(e.to_string());
                }
            }
            return Err(format!(
                "Index recovery failed: {move_error}; backup: {}; rollback: {rollback_errors:?}",
                backup.display()
            )
            .into());
        }
        moved.push((source, destination));
    }
    let repository = Repository::open(path).map_err(|e| {
        format!(
            "Index rebuild failed: {e}; original cache preserved: {}",
            backup.display()
        )
    })?;
    Ok((repository, Some(backup)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(format!(
                "index-recovery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn invalid_index_is_preserved_and_fresh_database_can_reopen() {
        let base = fixture();
        let path = base.join("index.sqlite");
        let bad = b"Dedicated invalid cache, never source media";
        fs::write(&path, bad).unwrap();
        let (repo, backup) = open_cache(&path).unwrap();
        let backup = backup.unwrap();
        assert_eq!(fs::read(backup.join("index.sqlite")).unwrap(), bad);
        assert_eq!(repo.integrity_check().unwrap(), "ok");
        drop(repo);
        assert!(open_cache(&path).unwrap().1.is_none());
        assert_eq!(fs::read(backup.join("index.sqlite")).unwrap(), bad);
    }
    #[test]
    fn page_corruption_is_backed_up_byte_for_byte() {
        let base = fixture();
        let path = base.join("index.sqlite");
        drop(Repository::open(&path).unwrap());
        let mut bytes = fs::read(&path).unwrap();
        assert!(bytes.len() > 100);
        bytes[100] = 0xff;
        fs::write(&path, &bytes).unwrap();
        let (repo, backup) = open_cache(&path).unwrap();
        assert_eq!(repo.integrity_check().unwrap(), "ok");
        assert_eq!(
            fs::read(backup.unwrap().join("index.sqlite")).unwrap(),
            bytes
        );
    }
    #[test]
    fn future_schema_and_non_database_destinations_are_not_replaced() {
        let base = fixture();
        let path = base.join("future.sqlite");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE preserved(value);INSERT INTO preserved VALUES(42);PRAGMA user_version=99;").unwrap();
        drop(db);
        let before = fs::read(&path).unwrap();
        assert!(open_cache(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let directory = base.join("directory.sqlite");
        fs::create_dir(&directory).unwrap();
        assert!(open_cache(&directory).is_err());
        assert!(!fs::read_dir(&base).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("index-recovery-")
        }));
    }
    #[test]
    fn invalid_sidecar_prevents_partial_quarantine() {
        let base = fixture();
        let path = base.join("index.sqlite");
        let bytes = b"not a database";
        fs::write(&path, bytes).unwrap();
        fs::create_dir(base.join("index.sqlite-journal")).unwrap();
        assert!(open_cache(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!fs::read_dir(&base).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("index-recovery-")
        }));
    }

    #[test]
    fn existing_sidecar_bytes_are_retained_with_the_bad_index() {
        let base = fixture();
        let path = base.join("index.sqlite");
        fs::write(&path, b"invalid managed index").unwrap();
        for suffix in ["-wal", "-shm", "-journal"] {
            fs::write(
                base.join(format!("index.sqlite{suffix}")),
                suffix.as_bytes(),
            )
            .unwrap();
        }
        let (repo, backup) = open_cache(&path).unwrap();
        assert_eq!(repo.integrity_check().unwrap(), "ok");
        for suffix in ["-wal", "-shm", "-journal"] {
            assert_eq!(
                fs::read(
                    backup
                        .as_ref()
                        .unwrap()
                        .join(format!("index.sqlite{suffix}"))
                )
                .unwrap(),
                suffix.as_bytes()
            );
        }
    }

    #[test]
    fn busy_database_is_not_treated_as_corruption() {
        let base = fixture();
        let path = base.join("index.sqlite");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE preserved(value); INSERT INTO preserved VALUES(42); PRAGMA user_version=1; BEGIN EXCLUSIVE;").unwrap();
        assert!(open_cache(&path).is_err());
        assert!(!fs::read_dir(&base).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("index-recovery-")
        }));
        db.execute_batch("ROLLBACK;").unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM preserved", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            42
        );
    }
}
