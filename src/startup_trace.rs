//! Opt-in wall-clock stage tracing. Disabled launches allocate/write nothing here.
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Instant,
};

struct Trace {
    started: Instant,
    path: PathBuf,
    events: Mutex<Vec<(&'static str, f64)>>,
}
static TRACE: OnceLock<Option<Trace>> = OnceLock::new();

pub fn init(started: Instant, path: Option<PathBuf>) {
    let _ = TRACE.set(path.map(|path| Trace {
        started,
        path,
        events: Mutex::new(vec![("main_enter", 0.)]),
    }));
}
pub fn enabled() -> bool {
    TRACE.get().is_some_and(Option::is_some)
}
pub fn mark(stage: &'static str) {
    if let Some(trace) = TRACE.get().and_then(Option::as_ref) {
        let elapsed = trace.started.elapsed().as_secs_f64() * 1000.;
        if let Ok(mut events) = trace.events.lock() {
            if !events.iter().any(|(existing, _)| *existing == stage) {
                events.push((stage, elapsed));
            }
        }
    }
}
pub fn finish() -> Result<(), Box<dyn std::error::Error>> {
    let Some(trace) = TRACE.get().and_then(Option::as_ref) else {
        return Ok(());
    };
    let events = trace
        .events
        .lock()
        .map_err(|_| "Startup trace lock poisoned")?;
    let rows: Vec<_> = events
        .iter()
        .map(|(stage, elapsed)| serde_json::json!({"stage":stage,"elapsed_ms":elapsed}))
        .collect();
    let report = serde_json::json!({
        "version":env!("CARGO_PKG_VERSION"),"pid":std::process::id(),"events":rows,
        "scope":"Opt-in wall-clock stages since main entry. File written only after native event-loop return. GL paint callback is not swap/display presentation or physical interactivity. Windows DLL loading before main is excluded."
    });
    if let Some(parent) = trace.path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&trace.path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
