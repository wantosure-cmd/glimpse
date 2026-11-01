//! Low cost per-process counters. No filesystem scan or network I/O.
use std::time::{Duration, Instant};
#[derive(Default)]
pub struct Telemetry {
    pub cpu: Option<f64>,
    pub memory: Option<u64>,
    pub read: Option<f64>,
    pub write: Option<f64>,
    last: Option<(Instant, u64, u64, u64)>,
}
impl Telemetry {
    pub fn tick(&mut self) {
        if self
            .last
            .is_some_and(|v| v.0.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        if let Some((cpu, memory, read, write)) = counters() {
            let now = Instant::now();
            if let Some((at, c, r, w)) = self.last {
                let dt = now.duration_since(at).as_secs_f64().max(0.001);
                let cores = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1) as f64;
                self.cpu = Some(
                    (cpu.saturating_sub(c) as f64 / 10_000_000. / dt / cores * 100.)
                        .clamp(0., 100.),
                );
                self.read = Some(read.saturating_sub(r) as f64 / dt);
                self.write = Some(write.saturating_sub(w) as f64 / dt);
            }
            self.memory = Some(memory);
            self.last = Some((now, cpu, read, write));
        }
    }
}
#[cfg(windows)]
fn counters() -> Option<(u64, u64, u64, u64)> {
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Default)]
    struct Io {
        ro: u64,
        wo: u64,
        other: u64,
        read: u64,
        write: u64,
        other_bytes: u64,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Mem {
        cb: u32,
        faults: u32,
        peak: usize,
        working: usize,
        pp: usize,
        p: usize,
        np: usize,
        n: usize,
        page: usize,
        peakpage: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            h: *mut c_void,
            c: *mut u64,
            e: *mut u64,
            k: *mut u64,
            u: *mut u64,
        ) -> i32;
        fn GetProcessIoCounters(h: *mut c_void, io: *mut Io) -> i32;
        fn K32GetProcessMemoryInfo(h: *mut c_void, m: *mut Mem, n: u32) -> i32;
    }
    unsafe {
        let h = GetCurrentProcess();
        let (mut c, mut e, mut k, mut u) = (0, 0, 0, 0);
        let mut io = Io::default();
        let mut mem = Mem::default();
        mem.cb = std::mem::size_of::<Mem>() as u32;
        if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) == 0
            || GetProcessIoCounters(h, &mut io) == 0
            || K32GetProcessMemoryInfo(h, &mut mem, mem.cb) == 0
        {
            return None;
        }
        Some((k + u, mem.working as u64, io.read, io.write))
    }
}
#[cfg(not(windows))]
fn counters() -> Option<(u64, u64, u64, u64)> {
    None
}
