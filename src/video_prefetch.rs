//! Bounded read-only OS-cache warming. One lazy worker, one latest pending file.
//! No second player, retained media buffers, writes or whole-video downloads.
use crate::domain::Fingerprint;
use serde::Serialize;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
};

#[derive(Clone, PartialEq)]
struct Request {
    path: PathBuf,
    version: Fingerprint,
    head: u64,
    tail: u64,
}
#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub path: Option<PathBuf>,
    pub bytes: u64,
    pub busy: bool,
    pub error: Option<String>,
}
#[derive(Default)]
struct State {
    epoch: u64,
    stop: bool,
    pending: Option<Request>,
    last: Option<Request>,
    status: Status,
}
pub struct VideoPrefetch {
    shared: Arc<(Mutex<State>, Condvar)>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Default for VideoPrefetch {
    fn default() -> Self {
        Self {
            shared: Arc::new((Mutex::new(State::default()), Condvar::new())),
            worker: None,
        }
    }
}
impl VideoPrefetch {
    pub fn request(
        &mut self,
        path: PathBuf,
        version: Fingerprint,
        virtual_drive: bool,
        wake: impl Fn() + Send + 'static,
    ) {
        let request = Request {
            path,
            version,
            head: if virtual_drive {
                512 * 1024
            } else {
                4 * 1024 * 1024
            },
            tail: if virtual_drive {
                128 * 1024
            } else {
                512 * 1024
            },
        };
        let mut state = self.shared.0.lock().unwrap();
        if state.last.as_ref() == Some(&request) {
            return;
        }
        state.epoch = state.epoch.wrapping_add(1);
        state.status.path = Some(request.path.clone());
        state.status.bytes = 0;
        state.status.error = None;
        state.last = Some(request.clone());
        state.pending = Some(request);
        drop(state);
        if self.worker.is_none() {
            let shared = self.shared.clone();
            self.worker = Some(thread::spawn(move || {
                loop {
                    let (request, epoch) = {
                        let mut state = shared.0.lock().unwrap();
                        while state.pending.is_none() && !state.stop {
                            state = shared.1.wait(state).unwrap();
                        }
                        if state.stop {
                            return;
                        }
                        state.status.busy = true;
                        (state.pending.take().unwrap(), state.epoch)
                    };
                    let valid = || {
                        let state = shared.0.lock().unwrap();
                        !state.stop && state.epoch == epoch
                    };
                    let result = warm(&request, valid);
                    let mut state = shared.0.lock().unwrap();
                    state.status.busy = false;
                    if state.epoch == epoch {
                        match result {
                            Ok(bytes) => state.status.bytes = bytes,
                            Err(e) => state.status.error = Some(e.to_string()),
                        }
                    }
                    drop(state);
                    wake();
                }
            }));
        }
        self.shared.1.notify_one();
    }
    pub fn cancel(&self) {
        let mut state = self.shared.0.lock().unwrap();
        state.epoch = state.epoch.wrapping_add(1);
        state.pending = None;
        state.last = None;
        state.status.path = None;
        state.status.bytes = 0;
        state.status.error = None;
    }
    pub fn idle(&self) -> bool {
        let state = self.shared.0.lock().unwrap();
        !state.status.busy && state.pending.is_none()
    }
    pub fn status(&self) -> Status {
        self.shared.0.lock().unwrap().status.clone()
    }
}
impl Drop for VideoPrefetch {
    fn drop(&mut self) {
        self.cancel();
        self.shared.0.lock().unwrap().stop = true;
        self.shared.1.notify_one();
        // Detach rather than freeze application exit on a blocked cloud filesystem read.
    }
}
fn warm(request: &Request, valid: impl Fn() -> bool) -> std::io::Result<u64> {
    if !valid() {
        return Ok(0);
    }
    let path = crate::media_path::resolve_read(&request.path)?;
    let version = Fingerprint::read(&path).map_err(std::io::Error::other)?;
    if version != request.version {
        return Ok(0);
    }
    let mut file = File::open(&path)?;
    let length = version.bytes;
    let head = length.min(request.head);
    let tail_start = length.saturating_sub(request.tail).max(head);
    let mut buffer = [0u8; 64 * 1024];
    let mut count = 0;
    for (offset, mut remaining) in [(0, head), (tail_start, length - tail_start)] {
        if !valid() {
            return Ok(0);
        }
        file.seek(SeekFrom::Start(offset))?;
        while remaining > 0 && valid() {
            let wanted = remaining.min(buffer.len() as u64) as usize;
            let got = file.read(&mut buffer[..wanted])?;
            if got == 0 {
                break;
            }
            remaining -= got as u64;
            count += got as u64;
        }
    }
    drop(file);
    if !valid() || Fingerprint::read(&path).map_err(std::io::Error::other)? != version {
        return Ok(0);
    }
    Ok(count)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_head_tail_cancellation_and_stale_version_are_read_only() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "prefetch-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("generated.bin");
        let file = File::create(&path).unwrap();
        file.set_len(9 * 1024 * 1024).unwrap();
        drop(file);
        let version = Fingerprint::read(&path).unwrap();
        use sha2::Digest;
        let before = sha2::Sha256::digest(std::fs::read(&path).unwrap());
        let request = Request {
            path: path.clone(),
            version: version.clone(),
            head: 4 * 1024 * 1024,
            tail: 512 * 1024,
        };
        assert_eq!(
            warm(&request, || true).unwrap(),
            4 * 1024 * 1024 + 512 * 1024
        );
        assert_eq!(warm(&request, || false).unwrap(), 0);
        let mut stale = request.clone();
        stale.version.bytes += 1;
        assert_eq!(warm(&stale, || true).unwrap(), 0);
        assert_eq!(Fingerprint::read(&path).unwrap(), version);
        let mut service = VideoPrefetch::default();
        let (tx, rx) = std::sync::mpsc::channel();
        service.request(path, version, false, move || {
            let _ = tx.send(());
        });
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(service.idle());
        assert_eq!(service.status().bytes, 4 * 1024 * 1024 + 512 * 1024);
        assert_eq!(
            sha2::Sha256::digest(std::fs::read(&request.path).unwrap()),
            before
        );
        service.cancel();
        assert!(service.status().path.is_none());
    }
}
