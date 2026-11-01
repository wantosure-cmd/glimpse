use media_browser::{
    index::{IndexResult, Repository},
    library::{LibraryEvent, LibraryService},
    view::{self, ViewSettings},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub fn run(args: &[String]) -> IndexResult<()> {
    let root = PathBuf::from(
        args.get(2)
            .ok_or(media_browser::ui_text!(msg_c51b65ed862f))?,
    );
    let db = PathBuf::from(
        args.iter()
            .position(|s| s == "--db")
            .and_then(|i| args.get(i + 1))
            .ok_or(media_browser::ui_text!(msg_71f9fdc46664))?,
    );
    let started = Instant::now();
    let service = LibraryService::new(db.clone(), || {});
    let generation = service.open(&root);
    let mut first_media_ms = None;
    let mut snapshots = 0u64;
    let final_snapshot = loop {
        if started.elapsed() > Duration::from_secs(60) {
            return Err(media_browser::ui_text!(msg_5328bccc5e62).into());
        }
        match service.recv_timeout(Duration::from_millis(100)) {
            Ok(LibraryEvent::Snapshot(s)) if s.generation == generation => {
                snapshots += 1;
                if !s.files.is_empty() && first_media_ms.is_none() {
                    first_media_ms = Some(started.elapsed().as_millis());
                }
                if s.finished {
                    break s;
                }
            }
            Ok(LibraryEvent::Error {
                generation: g,
                message,
            }) if g == generation => return Err(message.into()),
            _ => {}
        }
    };
    let pipeline_ms = started.elapsed().as_millis();
    let view_started = Instant::now();
    let view = view::build(&final_snapshot, &ViewSettings::default());
    let view_ms = view_started.elapsed().as_secs_f64() * 1000.;
    service.shutdown();
    let repo = Repository::open(&db)?;
    let report = serde_json::json!({
        "scope":"directory enumeration/stat/index/view only; no image decoding or thumbnail generation",
        "root":final_snapshot.root.display().to_string(),"media_count":final_snapshot.files.len(),"directory_count":final_snapshot.directories.len(),
        "image_count":view.shown.images,"video_count":view.shown.videos,"groups":view.groups.len(),"first_media_ms":first_media_ms,
        "enumeration_ms":final_snapshot.elapsed_ms,"pipeline_ms":pipeline_ms,"view_build_ms":view_ms,"snapshots":snapshots,"errors":final_snapshot.errors,
        "integrity_check":repo.integrity_check()?,
        "directories":final_snapshot.directories.iter().map(|d|serde_json::json!({"relative":d.path.strip_prefix(&final_snapshot.root).unwrap_or(&d.path).display().to_string(),"direct_images":d.direct.images,"direct_videos":d.direct.videos,"recursive_images":d.recursive.images,"recursive_videos":d.recursive.videos,"state":format!("{:?}",d.state),"subtree_complete":d.subtree_complete})).collect::<Vec<_>>(),
        "sample_ids":view.sequence.iter().take(50).collect::<Vec<_>>()
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
