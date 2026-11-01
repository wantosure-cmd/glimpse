//! Real OS notifications + scanner + SQLite, using only generated/copied fixtures.
use media_browser::{
    library::{LibraryEvent, LibraryService},
    watch::WatchService,
};
use std::{
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn change(w: &WatchService, epoch: u64) -> media_browser::watch::WatchEvent {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(event) = w.poll() {
            assert_eq!(event.epoch, epoch);
            if event.changed {
                return event;
            }
        }
        assert!(
            Instant::now() < deadline,
            "no native notification for epoch {epoch}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
fn scan(l: &LibraryService, root: &Path) -> Arc<media_browser::domain::LibrarySnapshot> {
    let generation = l.open(root);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match l.recv_timeout(Duration::from_millis(100)) {
            Ok(LibraryEvent::Snapshot(s)) if s.generation == generation && s.finished => return s,
            Ok(LibraryEvent::Error {
                generation: g,
                message,
            }) if g == generation => panic!("{message}"),
            _ => (),
        }
        assert!(Instant::now() < deadline, "scan timed out");
    }
}
fn quiet(w: &WatchService, epoch: u64) {
    thread::sleep(Duration::from_millis(700));
    if let Some(e) = w.poll() {
        assert_eq!(e.epoch, epoch);
    }
}
#[test]
fn native_changes_reconcile_counts_versions_and_exclusions() {
    let workspace = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
    let base = workspace.join("target/test-fixtures").join(format!(
        "native-watch-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&base).unwrap();
    let root = base.join("A");
    let other = base.join("B");
    let data = root.join("app-data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let source = base.join("original.png");
    image::RgbaImage::from_pixel(4, 3, image::Rgba([20, 80, 180, 255]))
        .save(&source)
        .unwrap();
    let original = std::fs::read(&source).unwrap();
    let library =
        LibraryService::new_excluding(data.join("index.sqlite"), vec![data.clone()], || {});
    let watcher = WatchService::new(|| {});
    let epoch = watcher.open(&root, &data);
    let ready = change(&watcher, epoch);
    assert!(ready.warning.is_none(), "{ready:?}");
    assert!(scan(&library, &root).files.is_empty());
    quiet(&watcher, epoch);
    let started = Instant::now();
    for i in 0..12 {
        std::fs::copy(&source, root.join(format!("copy-{i}.png"))).unwrap();
    }
    change(&watcher, epoch);
    let s = scan(&library, &root);
    assert_eq!(s.files.len(), 12);
    assert_eq!(
        s.directories.iter().map(|d| d.direct.images).sum::<u64>(),
        12
    );
    assert!(!s.directories.iter().any(|d| d.path.starts_with(&data)));
    let burst_ms = started.elapsed().as_millis();
    quiet(&watcher, epoch);

    let from = root.join("copy-0.png");
    let to = root.join("renamed.png");
    assert!(std::fs::canonicalize(&from).unwrap().starts_with(&base));
    assert!(
        std::fs::canonicalize(to.parent().unwrap())
            .unwrap()
            .starts_with(&base)
    );
    std::fs::rename(&from, &to).unwrap();
    change(&watcher, epoch);
    let renamed = scan(&library, &root);
    assert_eq!(renamed.files.len(), 12);
    assert!(
        renamed
            .files
            .iter()
            .any(|r| r.file.path.ends_with("renamed.png"))
    );
    assert!(
        !renamed
            .files
            .iter()
            .any(|r| r.file.path.ends_with("copy-0.png"))
    );
    let old = renamed
        .files
        .iter()
        .find(|r| r.file.path.ends_with("renamed.png"))
        .unwrap()
        .clone();
    quiet(&watcher, epoch);
    let replacement = base.join("replacement.png");
    image::RgbaImage::from_pixel(12, 9, image::Rgba([210, 30, 70, 255]))
        .save(&replacement)
        .unwrap();
    std::fs::copy(&replacement, &to).unwrap();
    change(&watcher, epoch);
    let replaced = scan(&library, &root);
    let new = replaced.files.iter().find(|r| r.id == old.id).unwrap();
    assert_ne!(old.file.fingerprint, new.file.fingerprint);
    quiet(&watcher, epoch);
    // Simulate an external actor removing only the copy created above, not product Delete.
    assert!(std::fs::canonicalize(&to).unwrap().starts_with(&base));
    std::fs::remove_file(&to).unwrap();
    change(&watcher, epoch);
    assert_eq!(scan(&library, &root).files.len(), 11);
    quiet(&watcher, epoch);

    std::fs::create_dir_all(data.join("thumbnails")).unwrap();
    std::fs::copy(&source, data.join("thumbnails/internal.png")).unwrap();
    std::fs::write(data.join("settings.json"), b"{}").unwrap();
    thread::sleep(Duration::from_millis(900));
    assert!(
        watcher.poll().is_none(),
        "internal cache caused feedback refresh"
    );
    assert_eq!(scan(&library, &root).files.len(), 11);
    quiet(&watcher, epoch);

    let epoch2 = watcher.open(&other, &data);
    change(&watcher, epoch2);
    quiet(&watcher, epoch2);
    std::fs::copy(&source, root.join("old-root-event.png")).unwrap();
    thread::sleep(Duration::from_millis(900));
    assert!(watcher.poll().is_none(), "old root contaminated new root");
    // Remove only a known empty directory; never recursively remove fixtures.
    assert!(std::fs::canonicalize(&other).unwrap().starts_with(&base));
    std::fs::remove_dir(&other).unwrap();
    assert!(change(&watcher, epoch2).warning.is_some());
    std::fs::create_dir(&other).unwrap();
    change(&watcher, epoch2);
    quiet(&watcher, epoch2);
    std::fs::copy(&source, other.join("restored.png")).unwrap();
    change(&watcher, epoch2);
    assert_eq!(scan(&library, &other).files.len(), 1);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let report = serde_json::json!({"passed":true,"fixture":base,"backend":format!("{:?}",<notify::RecommendedWatcher as notify::Watcher>::kind()),"burst_files":12,"burst_reconcile_ms":burst_ms,"rename":true,"same_path_replacement":true,"external_copy_removal":true,"cache_exclusion":true,"old_root_isolation":true,"root_recreation":true,"original_unchanged":true,"scope":"Real OS notifications plus library/scanner/SQLite; not UI or product recycle-bin Delete"});
    std::fs::create_dir_all(workspace.join("artifacts")).unwrap();
    std::fs::write(
        workspace.join("artifacts/p4-watch-native.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
    library.shutdown();
}
