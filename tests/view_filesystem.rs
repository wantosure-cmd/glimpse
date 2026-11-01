//! Real owned files -> scanner/SQLite -> view ordering. No UI or user-file mutations.
use media_browser::{
    domain::*,
    library::{LibraryEvent, LibraryService},
    view::{self, MediaFilter, SortField, ViewSettings},
};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn digest(path: &Path) -> String {
    format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
}
#[test]
fn real_file_properties_sort_filter_and_keep_parent_groups() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let base = workspace.join("target/test-fixtures").join(format!(
        "view-files-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&base).unwrap();
    let root = base.join("media");
    std::fs::create_dir(&root).unwrap();
    // LibraryService publishes canonical paths (including Windows' extended prefix).
    let root = root.canonicalize().unwrap();
    for (name, width, time) in [
        ("1.bmp", 1, 3),
        ("2.bmp", 3, 1),
        ("10.bmp", 2, 2),
        ("图集 2/1.bmp", 1, 5),
        ("图集 10/1.bmp", 1, 6),
    ] {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbImage::from_pixel(width, width, image::Rgb([30, 90, 180]))
            .save(&path)
            .unwrap();
        assert_eq!(image::image_dimensions(&path).unwrap(), (width, width));
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000 + time * 10))
            .unwrap();
    }
    let source = workspace.join("test-media/test-h264-aac.mov");
    let original = digest(&source);
    std::fs::copy(&source, root.join("3.mov")).unwrap();
    std::fs::File::options()
        .write(true)
        .open(root.join("3.mov"))
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_040))
        .unwrap();
    let library = LibraryService::new(base.join("index.sqlite3"), || {});
    let generation = library.open(&root);
    let deadline = Instant::now() + Duration::from_secs(10);
    let snapshot: Arc<LibrarySnapshot> = loop {
        assert!(Instant::now() < deadline, "scan did not finish");
        match library.recv_timeout(Duration::from_millis(50)) {
            Ok(LibraryEvent::Snapshot(s)) if s.generation == generation && s.finished => break s,
            Ok(LibraryEvent::Error {
                generation: g,
                message,
            }) if g == generation => panic!("{message}"),
            _ => {}
        }
    };
    assert_eq!(snapshot.files.len(), 6);
    let before: Vec<_> = snapshot
        .files
        .iter()
        .map(|r| {
            assert_eq!(r.file.fingerprint, Fingerprint::read(&r.file.path).unwrap());
            (r.file.path.clone(), digest(&r.file.path))
        })
        .collect();
    let name = |id: &MediaId| {
        snapshot
            .files
            .iter()
            .find(|r| r.id == *id)
            .unwrap()
            .file
            .path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    };
    let cases = [
        (
            SortField::Name,
            false,
            vec!["1.bmp", "2.bmp", "3.mov", "10.bmp"],
        ),
        (
            SortField::Name,
            true,
            vec!["10.bmp", "3.mov", "2.bmp", "1.bmp"],
        ),
        (
            SortField::Modified,
            false,
            vec!["2.bmp", "10.bmp", "1.bmp", "3.mov"],
        ),
        (
            SortField::Modified,
            true,
            vec!["3.mov", "1.bmp", "10.bmp", "2.bmp"],
        ),
        (
            SortField::Size,
            false,
            vec!["1.bmp", "10.bmp", "2.bmp", "3.mov"],
        ),
        (
            SortField::Size,
            true,
            vec!["3.mov", "2.bmp", "10.bmp", "1.bmp"],
        ),
    ];
    let mut rows = Vec::new();
    for (sort, descending, expected_root) in cases {
        for filter in [MediaFilter::All, MediaFilter::Images, MediaFilter::Videos] {
            for type_groups in [false, true] {
                let settings = ViewSettings {
                    sort,
                    descending,
                    filter,
                    type_groups,
                };
                let v = view::build(&snapshot, &settings);
                let mut expected: Vec<String> = expected_root
                    .iter()
                    .filter(|n| match filter {
                        MediaFilter::All => true,
                        MediaFilter::Images => n.ends_with(".bmp"),
                        MediaFilter::Videos => n.ends_with(".mov"),
                    })
                    .map(|n| n.to_string())
                    .collect();
                if type_groups {
                    let video: Vec<_> = expected
                        .iter()
                        .filter(|n| n.ends_with(".mov"))
                        .cloned()
                        .collect();
                    expected.retain(|n| n.ends_with(".bmp"));
                    expected.extend(video);
                }
                if filter != MediaFilter::Videos {
                    expected.extend([String::from("图集 2/1.bmp"), String::from("图集 10/1.bmp")]);
                }
                let actual: Vec<_> = v.sequence.iter().map(&name).collect();
                assert_eq!(actual, expected, "{settings:?}");
                for group in &v.groups {
                    for section in &group.sections {
                        for id in &section.items {
                            let record = snapshot.files.iter().find(|r| r.id == *id).unwrap();
                            assert_eq!(record.file.parent, group.directory);
                            if type_groups {
                                assert_eq!(section.kind, Some(record.file.kind));
                            }
                        }
                    }
                }
                for pair in v.sequence.windows(2) {
                    assert_eq!(view::neighbor(&v.sequence, pair[0], 1), Some(pair[1]));
                    assert_eq!(view::neighbor(&v.sequence, pair[1], -1), Some(pair[0]));
                }
                rows.push(serde_json::json!({"sort":format!("{sort:?}"),"descending":descending,"filter":format!("{filter:?}"),"type_groups":type_groups,"sequence":actual}));
            }
        }
    }
    assert_eq!(rows.len(), 36);
    for (path, hash) in &before {
        assert_eq!(&digest(path), hash);
    }
    assert_eq!(digest(&source), original);
    let report = serde_json::json!({"checks_pass":true,"scope":"Owned actual BMP/MOV file properties via scanner/SQLite and view; 36 combinations; no GUI click or physical input proof","fixture":base,"cases":rows,"source_original_unchanged":true,"generated_files_unchanged":true,"metadata":snapshot.files.iter().map(|r|serde_json::json!({"path":name(&r.id),"bytes":r.file.fingerprint.bytes,"modified_ns":r.file.fingerprint.modified_ns})).collect::<Vec<_>>()});
    std::fs::write(
        workspace.join("artifacts/glimpse-view-filesystem.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
    library.shutdown();
}
