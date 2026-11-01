//! Notifications invalidate a snapshot; only a subsequent scan decides file existence.
//! One pending root, one pending UI event, and constant-size debouncing state.
use notify::{EventKind, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct WatchEvent {
    pub epoch: u64,
    pub changed: bool,
    pub warning: Option<String>,
}
struct Request {
    epoch: u64,
    root: PathBuf,
    excluded: PathBuf,
}
#[derive(Default)]
struct State {
    epoch: u64,
    request: Option<Request>,
    dirty: Option<(Instant, Instant)>,
    pending: Option<WatchEvent>,
    backend_warning: Option<String>,
    stop: bool,
}
type Shared = Arc<(Mutex<State>, Condvar)>;
pub struct WatchService {
    shared: Shared,
    worker: Option<thread::JoinHandle<()>>,
}
impl WatchService {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let shared: Shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let state = shared.clone();
        let worker = thread::spawn(move || work(state, wake));
        Self {
            shared,
            worker: Some(worker),
        }
    }
    pub fn open(&self, root: &Path, excluded: &Path) -> u64 {
        let (mutex, signal) = &*self.shared;
        let mut s = mutex.lock().unwrap();
        s.epoch = s.epoch.wrapping_add(1).max(1);
        let epoch = s.epoch;
        s.request = Some(Request {
            epoch,
            root: root.to_owned(),
            excluded: excluded.to_owned(),
        });
        s.dirty = None;
        s.pending = None;
        s.backend_warning = None;
        signal.notify_one();
        epoch
    }
    pub fn poll(&self) -> Option<WatchEvent> {
        self.shared.0.lock().unwrap().pending.take()
    }
}
impl Drop for WatchService {
    fn drop(&mut self) {
        self.shared.0.lock().unwrap().stop = true;
        self.shared.1.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

// Do not canonicalize in the OS callback: paths may already have been removed.
fn lexical(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    #[cfg(windows)]
    {
        let text = path.as_os_str().to_string_lossy();
        if let Some(tail) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{tail}"));
        }
        if let Some(tail) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(tail);
        }
    }
    path
}
fn resolved(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let mut parent = absolute.as_path();
    let mut tail = Vec::new();
    loop {
        if let Ok(mut actual) = std::fs::canonicalize(parent) {
            for name in tail.iter().rev() {
                actual.push(name);
            }
            return lexical(&actual);
        }
        match (parent.file_name(), parent.parent()) {
            (Some(name), Some(next)) => {
                tail.push(name);
                parent = next;
            }
            _ => return lexical(&absolute),
        }
    }
}
fn relevant(kind: EventKind, paths: &[PathBuf], root: &Path, excluded: &Path) -> bool {
    !matches!(kind, EventKind::Access(_))
        && (paths.is_empty()
            || paths.iter().any(|p| {
                let p = lexical(p);
                p.starts_with(root) && !p.starts_with(excluded)
            }))
}
fn publish(shared: &Shared, epoch: u64, changed: bool, warning: Option<String>) -> bool {
    let mut s = shared.0.lock().unwrap();
    if s.epoch != epoch || s.stop {
        return false;
    }
    let changed = changed || s.pending.as_ref().is_some_and(|e| e.changed);
    s.pending = Some(WatchEvent {
        epoch,
        changed,
        warning,
    });
    true
}
fn work(shared: Shared, wake: impl Fn()) {
    let mut watcher: Option<notify::RecommendedWatcher> = None;
    let mut active: Option<(u64, PathBuf, bool)> = None;
    let mut warning = None;
    loop {
        let mut s = shared.0.lock().unwrap();
        if s.stop {
            return;
        }
        if let Some(request) = s.request.take() {
            drop(s);
            watcher.take();
            let root = resolved(&request.root);
            let excluded = resolved(&request.excluded);
            let callback_root = root.clone();
            let callback_state = shared.clone();
            let epoch = request.epoch;
            let result =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    let event = match event {
                        Ok(e) if relevant(e.kind, &e.paths, &callback_root, &excluded) => Ok(()),
                        Ok(_) => return,
                        Err(e) => Err(e.to_string()),
                    };
                    let mut s = callback_state.0.lock().unwrap();
                    if s.epoch != epoch || s.stop {
                        return;
                    }
                    let now = Instant::now();
                    let first = s.dirty.map_or(now, |d| d.0);
                    s.dirty = Some((first, now));
                    if let Err(message) = event {
                        s.backend_warning = Some(message.clone());
                        s.pending = Some(WatchEvent {
                            epoch,
                            changed: true,
                            warning: Some(message),
                        });
                    }
                    callback_state.1.notify_one();
                });
            warning = None;
            let mut bound = false;
            match result {
                Ok(mut w) => {
                    if let Some(parent) = root.parent() {
                        if let Err(e) = w.watch(parent, RecursiveMode::NonRecursive) {
                            warning = Some(crate::ui_format!(msg_c7fcbcad7615, e = e));
                        }
                    }
                    match w.watch(&root, RecursiveMode::Recursive) {
                        Ok(()) => bound = true,
                        Err(e) => warning = Some(crate::ui_format!(msg_3bfd72863ddb, e = e)),
                    }
                    watcher = Some(w);
                }
                Err(e) => warning = Some(crate::ui_format!(msg_f569d1a45265, e = e)),
            }
            active = Some((epoch, root, bound));
            // Subscribe first, then request reconciliation to cover the initial scan race.
            if publish(&shared, epoch, true, warning.clone()) {
                wake();
            }
            continue;
        }
        if let Some((first, last)) = s.dirty {
            if last.elapsed() >= Duration::from_millis(500)
                || first.elapsed() >= Duration::from_secs(2)
            {
                s.dirty = None;
                warning = s.backend_warning.clone().or(warning);
                drop(s);
                if let Some((epoch, root, bound)) = &mut active {
                    if let Some(w) = &mut watcher {
                        if !root.is_dir() {
                            if *bound {
                                let _ = w.unwatch(root);
                            }
                            *bound = false;
                            warning = Some(crate::ui_text!(msg_7dfb57afc881).into());
                        } else if !*bound {
                            match w.watch(root, RecursiveMode::Recursive) {
                                Ok(()) => {
                                    *bound = true;
                                    warning = None;
                                }
                                Err(e) => {
                                    warning =
                                        Some(crate::ui_format!(msg_667c88b35495, e = e))
                                }
                            }
                        }
                    }
                    if publish(&shared, *epoch, true, warning.clone()) {
                        wake();
                    }
                }
                continue;
            }
            let delay = Duration::from_millis(500)
                .saturating_sub(last.elapsed())
                .min(Duration::from_secs(2).saturating_sub(first.elapsed()));
            drop(shared.1.wait_timeout(s, delay).unwrap());
        } else {
            drop(shared.1.wait(s).unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_cache_access_and_sibling_paths() {
        let root = resolved(Path::new("target/watch-policy"));
        let excluded = root.join("app-data");
        assert!(!relevant(
            EventKind::Any,
            &[excluded.join("thumbnails/a.png")],
            &root,
            &excluded
        ));
        assert!(!relevant(
            EventKind::Access(notify::event::AccessKind::Any),
            &[root.join("a.jpg")],
            &root,
            &excluded
        ));
        assert!(!relevant(
            EventKind::Any,
            &[root.with_file_name("watch-policy-other").join("a.jpg")],
            &root,
            &excluded
        ));
        assert!(relevant(
            EventKind::Any,
            &[root.join("a.jpg")],
            &root,
            &excluded
        ));
    }
}
