//! Notification outage + persistent SQLite + explicit refresh, only owned copied PNGs.
//! Deliberately no WatchService: this tests recovery, not native notification delivery or UI input.
use media_browser::{
    domain::{LibrarySnapshot, MediaRecord},
    index::Repository,
    library::{LibraryEvent, LibraryService},
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn digest(path: &Path) -> String {
    format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
}
fn wait_view(
    service: &LibraryService,
    generation: u64,
    matches: impl Fn(&LibrarySnapshot) -> bool,
) -> Arc<LibrarySnapshot> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match service.recv_timeout(Duration::from_millis(50)) {
            Ok(LibraryEvent::Snapshot(s))
                if s.generation == generation && s.finished && matches(&s) =>
            {
                return s;
            }
            Ok(LibraryEvent::Error {
                generation: g,
                message,
            }) if g == generation => panic!("{message}"),
            _ => {}
        }
        assert!(
            Instant::now() < deadline,
            "No expected refreshed snapshot for generation {generation}"
        );
    }
}
fn pair(service: &LibraryService, generation: u64) -> (Arc<LibrarySnapshot>, Arc<LibrarySnapshot>) {
    let deadline = Instant::now() + Duration::from_secs(8);
    let (mut view, mut tree) = (None, None);
    loop {
        match service.recv_timeout(Duration::from_millis(50)) {
            Ok(LibraryEvent::Snapshot(s)) if s.generation == generation && s.finished => {
                view = Some(s)
            }
            Ok(LibraryEvent::Tree(s)) if s.generation == generation && s.finished => tree = Some(s),
            Ok(LibraryEvent::Error {
                generation: g,
                message,
            }) if g == generation => panic!("{message}"),
            _ => {}
        }
        if let (Some(v), Some(t)) = (&view, &tree) {
            return (v.clone(), t.clone());
        }
        assert!(Instant::now() < deadline, "No settled view/tree pair");
    }
}
fn record<'a>(s: &'a LibrarySnapshot, path: &Path) -> &'a MediaRecord {
    s.files.iter().find(|r| r.file.path == path).unwrap()
}

#[test]
fn explicit_refresh_after_closed_service_repairs_missed_changes_and_rejects_late_metadata() {
    let workspace = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
    let base = workspace.join("target/test-fixtures").join(format!(
        "manual-refresh-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("media");
    let left = root.join("相册 A");
    let right = root.join("相册 B");
    let quarantine = base.join("held-copies");
    for dir in [&left, &right, &quarantine] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(
        base.join(".manual-refresh-owned"),
        b"Generated originals and dedicated PNG copies; no user library",
    )
    .unwrap();
    let original = base.join("original.png");
    let replacement = base.join("replacement.png");
    image::RgbaImage::from_pixel(4, 3, image::Rgba([20, 80, 180, 255]))
        .save(&original)
        .unwrap();
    image::RgbaImage::from_fn(24, 18, |x, y| {
        image::Rgba([(x * 7) as u8, (y * 11) as u8, 170, 255])
    })
    .save(&replacement)
    .unwrap();
    let hashes = [digest(&original), digest(&replacement)];
    let root = std::fs::canonicalize(&root).unwrap();
    let left = std::fs::canonicalize(&left).unwrap();
    let right = std::fs::canonicalize(&right).unwrap();
    let changed = left.join("1.png");
    let moved = left.join("2.png");
    let sibling = right.join("1.png");
    for path in [&changed, &moved, &sibling] {
        std::fs::copy(&original, path).unwrap();
    }
    let db = base.join("index.sqlite");
    let service = LibraryService::new(db.clone(), || {});
    let generation = service.open(&root);
    let (before, _) = pair(&service, generation);
    assert_eq!(before.files.len(), 3);
    let old = record(&before, &changed).clone();
    let moved_id = record(&before, &moved).id;
    service.update_metadata(
        generation,
        old.id,
        old.file.fingerprint.clone(),
        Some(4),
        Some(3),
        None,
        None,
    );
    let seeded = wait_view(&service, generation, |s| {
        record(s, &changed).width == Some(4)
    });
    assert_eq!(record(&seeded, &changed).height, Some(3));
    service.shutdown(); // No worker, watcher or notification can reconcile the following changes.

    assert!(changed.starts_with(&base) && moved.starts_with(&base));
    std::fs::copy(&replacement, &changed).unwrap();
    let held = quarantine.join("2.png");
    assert!(
        std::fs::canonicalize(&quarantine)
            .unwrap()
            .starts_with(&base)
    );
    std::fs::rename(&moved, &held).unwrap(); // Keep the copy; do not permanently delete it.
    let added = left.join("new.png");
    let sibling_added = right.join("new.png");
    std::fs::copy(&original, &added).unwrap();
    std::fs::copy(&original, &sibling_added).unwrap();
    {
        let repository = Repository::open(&db).unwrap();
        assert_eq!(repository.total_media().unwrap(), 3);
        assert_eq!(repository.record_for_path(&moved).unwrap(), Some(moved_id));
        assert_eq!(repository.record_for_path(&added).unwrap(), None);
    } // Prove the persisted index really was stale, rather than assuming missed notifications.

    let service = LibraryService::new(db.clone(), || {});
    let generation = service.open_view(&left, Some(&root)); // Same boundary called by Browser::refresh_in_place.
    let (refreshed, tree) = pair(&service, generation);
    assert!(refreshed.errors.is_empty() && tree.errors.is_empty());
    assert_eq!(refreshed.files.len(), 2);
    assert_eq!(tree.files.len(), 4);
    for (path, count) in [(&root, 4), (&left, 2), (&right, 2)] {
        let dir = tree.directories.iter().find(|d| d.path == *path).unwrap();
        assert_eq!(dir.recursive.images, count);
        assert!(dir.subtree_complete);
    }
    assert!(!tree.files.iter().any(|r| r.id == moved_id));
    let fresh = record(&refreshed, &changed).clone();
    assert_eq!(fresh.id, old.id);
    assert_ne!(fresh.file.fingerprint, old.file.fingerprint);
    assert!(fresh.width.is_none() && fresh.height.is_none() && fresh.error.is_none());
    let barrier = record(&refreshed, &added).clone();
    // Same current generation, wrong old fingerprint: reject even a decoder finishing late.
    service.update_metadata(
        generation,
        fresh.id,
        old.file.fingerprint.clone(),
        Some(4),
        Some(3),
        None,
        Some("late obsolete decode".into()),
    );
    // A valid metadata result for a visible file is a queue barrier, avoiding sleeps.
    service.update_metadata(
        generation,
        barrier.id,
        barrier.file.fingerprint.clone(),
        Some(4),
        Some(3),
        None,
        None,
    );
    let after_late = wait_view(&service, generation, |s| record(s, &added).width == Some(4));
    assert!(
        record(&after_late, &changed).width.is_none()
            && record(&after_late, &changed).error.is_none()
    );
    let size = image::image_dimensions(&changed).unwrap();
    assert_eq!(size, (24, 18));
    service.update_metadata(
        generation,
        fresh.id,
        fresh.file.fingerprint.clone(),
        Some(size.0),
        Some(size.1),
        None,
        None,
    );
    let valid = wait_view(&service, generation, |s| {
        record(s, &changed).width == Some(24)
    });
    assert_eq!(record(&valid, &changed).height, Some(18));
    assert_eq!(
        record(&valid, &changed).file.fingerprint,
        fresh.file.fingerprint
    );
    service.shutdown();
    let repository = Repository::open(&db).unwrap();
    assert_eq!(repository.total_media().unwrap(), 4);
    assert_eq!(repository.record_for_path(&moved).unwrap(), None);
    assert_eq!(repository.record_for_path(&changed).unwrap(), Some(old.id));
    assert_eq!(hashes, [digest(&original), digest(&replacement)]);
    assert_eq!(digest(&held), hashes[0]);
    assert_eq!(digest(&changed), hashes[1]);
    let result = serde_json::json!({"checks_pass":true,"scope":"Persistent SQLite and LibraryService explicit rescan after service shutdown; no WatchService, OS input or toolbar click. Not a full A17 UI acceptance.","fixture":base,"before_count":3,"persisted_stale_count_before_refresh":3,"refreshed_view_count":2,"refreshed_tree_count":4,"recursive_counts":{"root":4,"left":2,"right":2},"stable_changed_media_id":old.id,"changed_fingerprint_invalidated_metadata":true,"current_generation_old_fingerprint_rejected":true,"valid_new_metadata":[24,18],"removed_copy_kept_at":held,"original_hashes":hashes,"originals_unchanged":true,"product_delete_exercised":false});
    std::fs::create_dir_all(workspace.join("artifacts")).unwrap();
    std::fs::write(
        workspace.join("artifacts/manual-refresh-verification.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    println!("{result}");
}
