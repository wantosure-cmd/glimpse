use media_browser::{
    index::Repository,
    library::{LibraryEvent, LibraryService},
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[test]
fn recovered_service_rescans_media_publishes_notice_and_reopens_persistent_index() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-fixtures")
        .join(format!(
            "recovered-service-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    let media = root.join("media");
    let data = root.join("data");
    fs::create_dir_all(&media).unwrap();
    fs::create_dir_all(&data).unwrap();
    let photo = media.join("dedicated.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([12, 34, 56, 255]))
        .save(&photo)
        .unwrap();
    let before = fs::read(&photo).unwrap();
    let bad = b"Deliberately corrupt derived index";
    let path = data.join("index.sqlite");
    fs::write(&path, bad).unwrap();
    let strict = LibraryService::new(path.clone(), || {});
    let strict_generation = strict.open(&media);
    let strict_started = Instant::now();
    loop {
        assert!(strict_started.elapsed() < Duration::from_secs(10));
        if let Ok(LibraryEvent::Error { generation, .. }) =
            strict.recv_timeout(Duration::from_millis(100))
        {
            assert_eq!(generation, strict_generation);
            break;
        }
    }
    strict.shutdown();
    assert_eq!(fs::read(&path).unwrap(), bad);
    let service = LibraryService::new_recovering_excluding(path.clone(), vec![data.clone()], || {});
    let generation = service.open(&media);
    let started = Instant::now();
    let mut notice = None;
    loop {
        assert!(started.elapsed() < Duration::from_secs(10));
        match service.recv_timeout(Duration::from_millis(100)) {
            Ok(LibraryEvent::IndexRecovered { backup }) => {
                assert!(notice.replace(backup).is_none());
            }
            Ok(LibraryEvent::Snapshot(s)) if s.generation == generation && s.finished => {
                assert_eq!(s.files.len(), 1);
                assert_eq!(
                    fs::canonicalize(&s.files[0].file.path).unwrap(),
                    fs::canonicalize(&photo).unwrap()
                );
                break;
            }
            Ok(LibraryEvent::Error { message, .. }) => panic!("{message}"),
            _ => {}
        }
    }
    service.shutdown();
    assert_eq!(fs::read(notice.unwrap().join("index.sqlite")).unwrap(), bad);
    assert_eq!(
        Repository::open(&path).unwrap().integrity_check().unwrap(),
        "ok"
    );
    assert_eq!(fs::read(&photo).unwrap(), before);
    let (_, second_notice) = media_browser::index_recovery::open_cache(&path).unwrap();
    assert!(second_notice.is_none());
}
