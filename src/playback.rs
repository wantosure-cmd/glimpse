//! libmpv C ABI is private to this module. No raw handles escape to application logic.
use eframe::{
    egui,
    glow::{self, HasContext},
};
use libloading::Library;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::{c_char, c_void, CStr, CString},
    path::Path,
    ptr,
    sync::Arc,
};

#[repr(C)]
struct Param {
    kind: i32,
    data: *mut c_void,
}
#[repr(C)]
struct GlInit {
    get_proc: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    ctx: *mut c_void,
}
#[repr(C)]
struct Fbo {
    fbo: i32,
    width: i32,
    height: i32,
    format: i32,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct RenderFrameInfo {
    flags: u64,
    target_time: i64,
}
// This pinned0.41-dev implementation returns NEXT_FRAME_INFO.pts in ns,
// matching mpv_get_time_ns(handle); its render.h prose still says microseconds.
fn host_video_deadline(info: RenderFrameInfo, now_ns: i64) -> Option<std::time::Duration> {
    if info.flags & 1 == 0 || info.flags & 2 != 0 {
        return None;
    }
    let delta = info.target_time.saturating_sub(now_ns);
    // A clock/ABI mismatch must fall back to native blocking, never freeze UI
    // video for an absurd host deadline. Actual outliers remain diagnostic data.
    (delta > 0 && delta <= 2_000_000_000).then(|| std::time::Duration::from_nanos(delta as u64))
}
fn video_frame_is_due(info: RenderFrameInfo, now_ns: i64) -> bool {
    info.flags == 1 && info.target_time > 0 && info.target_time <= now_ns
}
type Proc = Arc<dyn Fn(&CStr) -> *const c_void + Send + Sync>;
type SetOpt = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> i32;
type Command = unsafe extern "C" fn(*mut c_void, u64, *const *const c_char) -> i32;
type Observe = unsafe extern "C" fn(*mut c_void, u64, *const c_char, i32) -> i32;
#[repr(C)]
struct Event {
    id: i32,
    error: i32,
    userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct Property {
    name: *const c_char,
    format: i32,
    data: *mut c_void,
}
// Prefix layouts from locked libmpv client.h, API >= 1.108.
#[repr(C)]
struct StartFile {
    playlist_entry_id: i64,
}
#[repr(C)]
struct EndFile {
    reason: i32,
    error: i32,
    playlist_entry_id: i64,
}
#[repr(C)]
union NodeValue {
    integer: i64,
    list: *const NodeList,
    pointer: *const c_void,
}
#[repr(C)]
struct Node {
    value: NodeValue,
    format: i32,
}
#[repr(C)]
struct NodeList {
    count: i32,
    values: *const Node,
    keys: *const *const c_char,
}
unsafe fn entry_from_reply(data: *const c_void) -> Option<i64> {
    unsafe {
        if data.is_null() {
            return None;
        }
        let node = &*(data as *const Node);
        if node.format != 8 || node.value.list.is_null() {
            return None;
        }
        let list = &*node.value.list;
        if !(1..=32).contains(&list.count) || list.values.is_null() || list.keys.is_null() {
            return None;
        }
        for index in 0..list.count as usize {
            let key = *list.keys.add(index);
            let value = &*list.values.add(index);
            if !key.is_null()
                && CStr::from_ptr(key).to_bytes() == b"playlist_entry_id"
                && value.format == 4
            {
                return Some(value.value.integer);
            }
        }
        None
    }
}
#[derive(Clone)]
enum Value {
    Number(f64),
    Text(String),
}
type Render = unsafe extern "C" fn(*mut c_void, *mut Param) -> i32;

pub struct Player {
    _lib: Library,
    handle: *mut c_void,
    render: *mut c_void,
    command: Command,
    wait: unsafe extern "C" fn(*mut c_void, f64) -> *const Event,
    properties: RefCell<HashMap<String, Value>>,
    loaded: Cell<bool>,
    requested_pause: Cell<bool>,
    active_entry: Cell<i64>,
    last_error: RefCell<Option<(i64, String)>>,
    load_cookie: Cell<u64>,
    subtitle_cookie: Cell<u64>,
    subtitle_result: RefCell<Option<Result<(), String>>>,
    unload_cookie: Cell<u64>,
    unload_command_done: Cell<bool>,
    unload_idle: Cell<bool>,
    unload_path_clear: Cell<bool>,
    unload_error: RefCell<Option<String>>,
    expected_entry: Cell<Option<i64>>,
    observe: Observe,
    unobserve: unsafe extern "C" fn(*mut c_void, u64) -> i32,
    observed_cookie: Cell<u64>,
    error_string: unsafe extern "C" fn(i32) -> *const c_char,
    render_fn: Render,
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    repeat_render: bool,
    render_info: unsafe extern "C" fn(*mut c_void, Param) -> i32,
    mpv_time_ns: unsafe extern "C" fn(*mut c_void) -> i64,
    defer_target: bool,
    bypass_due_wait: bool,
    last_seek: Cell<Option<std::time::Instant>>,
    frame_render_pending: bool,
    render_content_valid: bool,
    render_free: unsafe extern "C" fn(*mut c_void),
    destroy: unsafe extern "C" fn(*mut c_void),
    // Box gives the address passed to libmpv a stable lifetime.
    _proc: Option<Box<Proc>>,
    _wake: Option<Box<egui::Context>>,
    fbo: Option<glow::Framebuffer>,
    texture: Option<glow::Texture>,
    dimensions: (i32, i32),
    pub source: Option<std::path::PathBuf>,
    load_requested: Option<std::time::Instant>,
    pub first_ready_render_ms: Option<f64>,
    // Opt-in bounded diagnostics; no per-frame allocation or file I/O.
    render_timing: Option<RenderTiming>,
}

#[derive(Default, serde::Serialize)]
struct RenderTiming {
    #[serde(skip)]
    first_paint: Option<std::time::Instant>,
    long_render_events: Vec<serde_json::Value>,
    calls: u64,
    paint_calls: u64,
    target_info_calls: u64,
    target_clock_outliers: u64,
    guarded_due_calls: u64,
    guarded_due_min_lateness_ms: f64,
    deadline_deferrals: u64,
    target_ahead_max_ms: f64,
    reused_frame_blits: u64,
    render_total_ms: f64,
    render_max_ms: f64,
    render_over_20ms: u64,
    paint_max_ms: f64,
    last_render_ms: f64,
    // 2ms buckets; last bucket includes everything >=62ms.
    render_histogram_2ms: [u64; 32],
    avsync_samples: u64,
    avsync_abs_max_seconds: f64,
}
// Player is accessed only on the GL/UI thread; Mutex is used by egui callback ownership.
// libmpv's callbacks only request redraw and never access this struct.
unsafe impl Send for Player {}

unsafe extern "C" fn get_proc(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { (*(ctx as *const Proc))(CStr::from_ptr(name)) as *mut c_void }
}
unsafe extern "C" fn wake(ctx: *mut c_void) {
    unsafe {
        (*(ctx as *const egui::Context)).request_repaint();
    }
}

impl Player {
    pub fn new(proc: Option<Proc>, ctx: Option<egui::Context>) -> Result<Self, String> {
        unsafe {
            let lib = crate::native::load("libmpv-2.dll")?;
            macro_rules! sym {
                ($name:literal, $ty:ty) => {
                    *lib.get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            let create = sym!("mpv_create", unsafe extern "C" fn() -> *mut c_void);
            let init = sym!("mpv_initialize", unsafe extern "C" fn(*mut c_void) -> i32);
            let opt = sym!("mpv_set_option_string", SetOpt);
            let command = sym!("mpv_command_async", Command);
            let wait = sym!(
                "mpv_wait_event",
                unsafe extern "C" fn(*mut c_void, f64) -> *const Event
            );
            let observe = sym!(
                "mpv_observe_property",
                unsafe extern "C" fn(*mut c_void, u64, *const c_char, i32) -> i32
            );
            let render_create = sym!(
                "mpv_render_context_create",
                unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut Param) -> i32
            );
            let render_fn = sym!("mpv_render_context_render", Render);
            let render_update = sym!(
                "mpv_render_context_update",
                unsafe extern "C" fn(*mut c_void) -> u64
            );
            let render_info = sym!(
                "mpv_render_context_get_info",
                unsafe extern "C" fn(*mut c_void, Param) -> i32
            );
            let mpv_time_ns = sym!("mpv_get_time_ns", unsafe extern "C" fn(*mut c_void) -> i64);
            let render_free = sym!("mpv_render_context_free", unsafe extern "C" fn(*mut c_void));
            let destroy = sym!("mpv_terminate_destroy", unsafe extern "C" fn(*mut c_void));
            let set_wake = sym!(
                "mpv_render_context_set_update_callback",
                unsafe extern "C" fn(
                    *mut c_void,
                    Option<unsafe extern "C" fn(*mut c_void)>,
                    *mut c_void,
                )
            );
            let handle = create();
            let unobserve = sym!(
                "mpv_unobserve_property",
                unsafe extern "C" fn(*mut c_void, u64) -> i32
            );
            let error_string = sym!(
                "mpv_error_string",
                unsafe extern "C" fn(i32) -> *const c_char
            );
            if handle.is_null() {
                return Err(media_browser::ui_text!(msg_4dc35d1072e9).into());
            }
            let mut p = Self {
                _lib: lib,
                handle,
                render: ptr::null_mut(),
                command,
                wait,
                properties: RefCell::new(HashMap::new()),
                loaded: Cell::new(false),
                requested_pause: Cell::new(false),
                active_entry: Cell::new(-1),
                last_error: RefCell::new(None),
                load_cookie: Cell::new(0),
                subtitle_cookie: Cell::new(0),
                subtitle_result: RefCell::new(None),
                unload_cookie: Cell::new(0),
                unload_command_done: Cell::new(true),
                unload_idle: Cell::new(true),
                unload_path_clear: Cell::new(true),
                unload_error: RefCell::new(None),
                expected_entry: Cell::new(None),
                observe,
                unobserve,
                observed_cookie: Cell::new(0),
                error_string,
                render_fn,
                render_update,
                render_info,
                mpv_time_ns,
                defer_target: std::env::var_os("GLIMPSE_HOST_DEFER_VIDEO_FRAME").as_deref()
                    == Some(std::ffi::OsStr::new("1"))
                    && std::env::var_os("GLIMPSE_LEGACY_VIDEO_TARGET_WAIT").as_deref()
                        != Some(std::ffi::OsStr::new("1")),
                bypass_due_wait: std::env::var_os("GLIMPSE_GUARDED_DUE_VIDEO_FRAME").as_deref()
                    == Some(std::ffi::OsStr::new("1"))
                    && std::env::var_os("GLIMPSE_KEEP_NATIVE_TARGET_BLOCK").as_deref()
                        != Some(std::ffi::OsStr::new("1")),
                last_seek: Cell::new(None),
                frame_render_pending: false,
                repeat_render: std::env::var_os("GLIMPSE_REPEAT_VIDEO_RENDER").as_deref()
                    == Some(std::ffi::OsStr::new("1")),
                render_content_valid: false,
                render_free,
                destroy,
                _proc: proc.map(Box::new),
                _wake: ctx.map(Box::new),
                fbo: None,
                texture: None,
                dimensions: (0, 0),
                source: None,
                load_requested: None,
                first_ready_render_ms: None,
                render_timing: (std::env::var_os("GLIMPSE_RENDER_TIMING").as_deref()
                    == Some(std::ffi::OsStr::new("1")))
                .then(RenderTiming::default),
            };
            for (key, val) in [
                ("config", "no"),
                ("terminal", "no"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("osc", "no"),
                ("sub-auto", "exact"),
                ("sub-codepage", "auto"),
                ("keep-open", "yes"),
                ("volume-max", "400"),
                ("audio-pitch-correction", "yes"),
                ("idle", "yes"),
                ("hwdec", "auto-safe"),
                // The host owns UI pacing. Let mpv schedule frames at their
                // audio-clock target instead of entering render 50ms early
                // and blocking the shared UI/GL thread until that target.
                // Keep native A/V synchronization and render timing enabled.
                ("video-timing-offset", "0"),
            ] {
                let (k, v) = (CString::new(key).unwrap(), CString::new(val).unwrap());
                let code = opt(handle, k.as_ptr(), v.as_ptr());
                if code < 0 {
                    return Err(media_browser::ui_format!(
                        msg_1c58c859889a,
                        code = code,
                        key = key
                    ));
                }
            }
            let vo = CString::new("vo").unwrap();
            let value = CString::new(if p._proc.is_some() { "libmpv" } else { "null" }).unwrap();
            opt(handle, vo.as_ptr(), value.as_ptr());
            if p._proc.is_none() {
                opt(handle, c"ao".as_ptr(), c"null".as_ptr());
            }
            let code = init(handle);
            if code < 0 {
                return Err(media_browser::ui_format!(msg_665e30cdc42e, code = code));
            }
            for name in ["volume", "speed"] {
                let key = CString::new(name).unwrap();
                observe(handle, 0, key.as_ptr(), 5);
            }
            if p.render_timing.is_some() {
                for name in [
                    "avsync",
                    "total-avsync-change",
                    "frame-drop-count",
                    "decoder-frame-drop-count",
                    "mistimed-frame-count",
                    "vo-delayed-frame-count",
                ] {
                    let key = CString::new(name).unwrap();
                    observe(handle, 0, key.as_ptr(), 5);
                }
            }
            for name in [
                "mute",
                "pause",
                "loop-file",
                "mpv-version",
                "current-ao",
                "current-vo",
            ] {
                let key = CString::new(name).unwrap();
                observe(handle, 0, key.as_ptr(), 1);
            }
            if let Some(proc) = &mut p._proc {
                let mut glinit = GlInit {
                    get_proc,
                    ctx: (&mut **proc as *mut Proc).cast(),
                };
                let mut params = [
                    Param {
                        kind: 1,
                        data: c"opengl".as_ptr() as *mut c_void,
                    },
                    Param {
                        kind: 2,
                        data: (&mut glinit as *mut GlInit).cast(),
                    },
                    Param {
                        kind: 0,
                        data: ptr::null_mut(),
                    },
                ];
                let code = render_create(&mut p.render, handle, params.as_mut_ptr());
                if code < 0 {
                    return Err(media_browser::ui_format!(msg_241f20389b16, code = code));
                }
                if let Some(ctx) = &mut p._wake {
                    set_wake(
                        p.render,
                        Some(wake),
                        (&mut **ctx as *mut egui::Context).cast(),
                    );
                }
            }
            Ok(p)
        }
    }
    pub fn cmd(&self, args: &[&str]) -> Result<(), String> {
        self.tagged_command(args, 0)
    }
    fn tagged_command(&self, args: &[&str], cookie: u64) -> Result<(), String> {
        if args.first() == Some(&"seek") {
            self.last_seek.set(Some(std::time::Instant::now()));
        }
        let values: Result<Vec<_>, _> = args.iter().map(|s| CString::new(*s)).collect();
        let values = values.map_err(|e| e.to_string())?;
        let mut argv: Vec<_> = values.iter().map(|s| s.as_ptr()).collect();
        argv.push(ptr::null());
        let code = unsafe { (self.command)(self.handle, cookie, argv.as_ptr()) };
        if code < 0 {
            Err(media_browser::ui_format!(
                msg_cc43b50c3426,
                args = args,
                code = code
            ))
        } else {
            Ok(())
        }
    }
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        self.load_requested = Some(std::time::Instant::now());
        self.first_ready_render_ms = None;
        self.subtitle_cookie
            .set(self.subtitle_cookie.get().wrapping_add(1));
        self.subtitle_result.borrow_mut().take();
        self.poll();
        self.cancel_release_observer();
        self.unload_command_done.set(false);
        self.unload_idle.set(false);
        self.unload_path_clear.set(false);
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            media_browser::media_path::resolve_read(path).map_err(|e| e.to_string())?
        };
        // Pass the canonical UTF-8 path intact. Stripping the Windows extended
        // prefix makes CreateFileW depend on the host's longPathAware manifest
        // and system policy, so thumbnails may work while libmpv cannot open it.
        // CloudDrive's readable non-verbatim fallback is already handled above.
        let text = path
            .to_str()
            .ok_or(media_browser::ui_text!(msg_f6b9e2fa1138))?
            .to_owned();
        if self.observed_cookie.get() != 0 {
            unsafe {
                (self.unobserve)(self.handle, self.observed_cookie.get());
            }
        }
        self.observed_cookie.set(0);
        for name in [
            "time-pos",
            "dwidth",
            "dheight",
            "video-out-params/rotate",
            "video-rotate",
            "duration",
            "path",
            "seekable",
            "eof-reached",
            "video-codec",
            "audio-codec-name",
            "hwdec-current",
            "sub-text",
            "sid",
        ] {
            self.properties.borrow_mut().remove(name);
        }
        self.loaded.set(false);
        self.active_entry.set(-1);
        *self.last_error.borrow_mut() = None;
        let cookie = self.load_cookie.get().wrapping_add(1).max(1);
        self.load_cookie.set(cookie);
        self.expected_entry.set(None);
        self.tagged_command(&["loadfile", &text, "replace"], cookie)?;
        self.render_content_valid = false;
        self.frame_render_pending = false;
        self.source = Some(path);
        Ok(())
    }
    pub fn unload(&mut self) -> Result<(), String> {
        self.subtitle_cookie
            .set(self.subtitle_cookie.get().wrapping_add(1));
        self.subtitle_result.borrow_mut().take();
        if self.observed_cookie.get() != 0 {
            unsafe {
                (self.unobserve)(self.handle, self.observed_cookie.get());
            }
        }
        self.observed_cookie.set(0);
        self.cancel_release_observer();
        self.load_cookie
            .set(self.load_cookie.get().wrapping_add(1).max(1));
        let cookie = self.load_cookie.get() | (1u64 << 63);
        self.unload_cookie.set(cookie);
        self.unload_command_done.set(false);
        self.unload_idle.set(false);
        self.unload_path_clear.set(false);
        *self.unload_error.borrow_mut() = None;
        self.tagged_command(&["stop"], cookie)?;
        self.expected_entry.set(None);
        self.render_content_valid = false;
        self.first_ready_render_ms = None;
        self.frame_render_pending = false;
        self.source = None;
        self.loaded.set(false);
        self.active_entry.set(-1);
        *self.last_error.borrow_mut() = None;
        // These options belong to the player, not the current file. mpv does not
        // re-notify an observer when setting an already-equal value on reopen.
        self.properties.borrow_mut().retain(|key, _| {
            matches!(
                key.as_str(),
                "mute"
                    | "volume"
                    | "speed"
                    | "pause"
                    | "loop-file"
                    | "mpv-version"
                    | "current-ao"
                    | "current-vo"
            )
        });
        Ok(())
    }
    fn cancel_release_observer(&self) {
        let cookie = self.unload_cookie.replace(0);
        if cookie != 0 {
            unsafe {
                (self.unobserve)(self.handle, cookie);
            }
        }
        *self.unload_error.borrow_mut() = None;
    }
    /// PT_STOP (idle-active) can be visible before the demuxer is closed. Require
    /// both idle and unavailable path after the stop reply. The locked mpv clears
    /// filename only after uninit_demuxer; cookies reject old observer events.
    pub fn release_complete(&self) -> Result<bool, String> {
        self.poll();
        if let Some(error) = self.unload_error.borrow().clone() {
            return Err(error);
        }
        Ok(self.source.is_none()
            && self.unload_command_done.get()
            && self.unload_idle.get()
            && self.unload_path_clear.get())
    }
    fn poll(&self) {
        // Only nonblocking event retrieval and asynchronous commands run on the GL thread.
        // Synchronous mpv_get_property/mpv_command are forbidden by render.h threading rules.
        unsafe {
            for _ in 0..512 {
                let event = &*(self.wait)(self.handle, 0.);
                if event.id == 0 {
                    break;
                }
                if event.userdata & (1u64 << 62) != 0 && event.userdata & (1u64 << 63) == 0 {
                    if event.id == 5 && event.userdata == self.subtitle_cookie.get() {
                        *self.subtitle_result.borrow_mut() = Some(if event.error < 0 {
                            Err(media_browser::ui_format!(msg_1d5f3dde3764, event.error))
                        } else {
                            Ok(())
                        });
                    }
                    continue;
                }
                if event.userdata & (1u64 << 63) != 0 {
                    if event.userdata != self.unload_cookie.get() {
                        continue;
                    }
                    if event.id == 5 {
                        if event.error < 0 {
                            *self.unload_error.borrow_mut() =
                                Some(media_browser::ui_format!(msg_9a32734e7e80, event.error));
                        } else {
                            self.unload_command_done.set(true);
                            for name in [c"idle-active", c"path"] {
                                let code =
                                    (self.observe)(self.handle, event.userdata, name.as_ptr(), 1);
                                if code < 0 {
                                    *self.unload_error.borrow_mut() = Some(
                                        media_browser::ui_format!(msg_bc469be028ec, code = code),
                                    );
                                }
                            }
                        }
                    } else if event.id == 22 && !event.data.is_null() {
                        let property = &*(event.data as *const Property);
                        if property.name.is_null() {
                            continue;
                        }
                        match CStr::from_ptr(property.name).to_bytes() {
                            b"path" => {
                                self.unload_path_clear.set(property.format == 0);
                            }
                            b"idle-active" if property.format == 1 && !property.data.is_null() => {
                                let value = *(property.data as *const *const c_char);
                                self.unload_idle.set(
                                    !value.is_null() && CStr::from_ptr(value).to_bytes() == b"yes",
                                );
                            }
                            _ => {}
                        }
                    }
                    continue;
                }
                if event.id == 5 && event.userdata != 0 {
                    if event.userdata == self.load_cookie.get() {
                        if event.error < 0 {
                            *self.last_error.borrow_mut() = Some((
                                -1,
                                media_browser::ui_format!(msg_1254e6914ec0, event.error),
                            ));
                        } else if let Some(entry) = entry_from_reply(event.data) {
                            self.expected_entry.set(Some(entry));
                            self.observe_current_file();
                        } else {
                            *self.last_error.borrow_mut() =
                                Some((-1, media_browser::ui_text!(msg_81e143ce1819).into()));
                        }
                    }
                } else if event.id == 6 {
                    self.loaded.set(false);
                    if !event.data.is_null() {
                        self.active_entry
                            .set((*(event.data as *const StartFile)).playlist_entry_id);
                    }
                    *self.last_error.borrow_mut() = None;
                    let mut cache = self.properties.borrow_mut();
                    cache.remove("time-pos");
                    cache.remove("duration");
                } else if event.id == 8 {
                    self.loaded.set(true);
                    self.observe_current_file();
                } else if event.id == 7 && !event.data.is_null() {
                    let ended = &*(event.data as *const EndFile);
                    if ended.playlist_entry_id == self.active_entry.get() && ended.reason == 4 {
                        let message =
                            CStr::from_ptr((self.error_string)(ended.error)).to_string_lossy();
                        *self.last_error.borrow_mut() = Some((
                            ended.playlist_entry_id,
                            media_browser::ui_format!(
                                msg_9415129a537b,
                                ended.error,
                                message = message
                            ),
                        ));
                        self.loaded.set(false);
                    }
                } else if event.id == 22 && !event.data.is_null() {
                    if event.userdata != 0 && event.userdata != self.load_cookie.get() {
                        continue;
                    }
                    let prop = &*(event.data as *const Property);
                    if prop.name.is_null() {
                        continue;
                    }
                    let key = CStr::from_ptr(prop.name).to_string_lossy().into_owned();
                    let mut cache = self.properties.borrow_mut();
                    if prop.data.is_null() {
                        cache.remove(&key);
                        continue;
                    }
                    match prop.format {
                        5 => {
                            cache.insert(key, Value::Number(*(prop.data as *const f64)));
                        }
                        1 => {
                            let value = *(prop.data as *const *const c_char);
                            if !value.is_null() {
                                cache.insert(
                                    key,
                                    Value::Text(
                                        CStr::from_ptr(value).to_string_lossy().into_owned(),
                                    ),
                                );
                            }
                        }
                        _ => {}
                    }
                } else if event.id == 5 && event.error < 0 {
                    eprintln!(
                        "{}",
                        media_browser::ui_format!(msg_0445156ba326, event.error)
                    );
                }
            }
        }
    }
    pub fn error(&self) -> Option<String> {
        self.poll();
        self.last_error
            .borrow()
            .as_ref()
            .filter(|(entry, _)| *entry == -1 || Some(*entry) == self.expected_entry.get())
            .map(|(_, message)| message.clone())
    }
    fn observe_current_file(&self) {
        if !self.loaded.get()
            || self.expected_entry.get() != Some(self.active_entry.get())
            || self.observed_cookie.get() == self.load_cookie.get()
        {
            return;
        }
        self.observed_cookie.set(self.load_cookie.get());
        for (name, format) in [
            ("time-pos", 5),
            ("dwidth", 5),
            ("dheight", 5),
            ("video-out-params/rotate", 5),
            ("video-rotate", 5),
            ("duration", 5),
            ("path", 1),
            ("seekable", 1),
            ("eof-reached", 1),
            ("video-codec", 1),
            ("audio-codec-name", 1),
            ("hwdec-current", 1),
            ("sub-text", 1),
            ("sid", 1),
            ("sub-pos", 5),
            ("sub-font-size", 5),
            ("sub-color", 1),
            ("sub-visibility", 1),
            ("sub-border-style", 1),
            ("sub-back-color", 1),
        ] {
            let name = CString::new(name).unwrap();
            unsafe {
                (self.observe)(self.handle, self.load_cookie.get(), name.as_ptr(), format);
            }
        }
    }
    pub fn number(&self, name: &str) -> Option<f64> {
        self.poll();
        match self.properties.borrow().get(name) {
            Some(Value::Number(n)) => Some(*n),
            _ => None,
        }
    }
    pub fn add_subtitle(&self, path: &Path) -> Result<(), String> {
        if !self.ready() {
            return Err(media_browser::ui_text!(msg_2b6c18a3b086).into());
        }
        // Do not read cloud files or canonicalize them on the rendering thread.
        let text = path
            .to_str()
            .ok_or(media_browser::ui_text!(msg_5459b4e5d44e))?;
        let cookie = (1u64 << 62)
            | ((self.subtitle_cookie.get().wrapping_add(1)) & ((1u64 << 62) - 1)).max(1);
        self.subtitle_cookie.set(cookie);
        self.subtitle_result.borrow_mut().take();
        self.set("sub-visibility", "yes")?;
        self.tagged_command(&["sub-add", text, "select"], cookie)
    }
    pub fn take_subtitle_result(&self) -> Option<Result<(), String>> {
        self.poll();
        self.subtitle_result.borrow_mut().take()
    }
    pub fn display_size(&self) -> Option<(f32, f32)> {
        if !self.ready() {
            return None;
        }
        let (mut w, mut h) = (self.number("dwidth")?, self.number("dheight")?);
        if !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0. {
            return None;
        }
        if (self.number("video-out-params/rotate").unwrap_or(0.).round() as i64).unsigned_abs()
            % 180
            == 90
        {
            std::mem::swap(&mut w, &mut h);
        }
        Some((w as f32, h as f32))
    }
    pub fn has_presented_frame(&self) -> bool {
        self.render_content_valid && self.first_ready_render_ms.is_some() && self.ready()
    }
    pub fn ready(&self) -> bool {
        self.poll();
        if !self.loaded.get() || self.expected_entry.get() != Some(self.active_entry.get()) {
            return false;
        }
        let Some(path) = self.text("path") else {
            return false;
        };
        self.source.as_ref().is_some_and(|source| {
            let normalize = |s: &str| {
                let s = if let Some(unc) = s.strip_prefix("\\\\?\\UNC\\") {
                    format!("\\\\{unc}")
                } else {
                    s.strip_prefix("\\\\?\\").unwrap_or(s).to_owned()
                };
                if cfg!(windows) {
                    s.replace('/', "\\").to_lowercase()
                } else {
                    s
                }
            };
            normalize(&path) == normalize(&source.to_string_lossy())
        })
    }
    pub fn text(&self, name: &str) -> Option<String> {
        self.poll();
        match self.properties.borrow().get(name) {
            Some(Value::Text(s)) => Some(s.clone()),
            _ => None,
        }
    }
    pub fn seek(&self, time: f64) -> Result<(), String> {
        self.cmd(&["seek", &format!("{time:.3}"), "absolute+exact"])
    }
    pub fn toggle_pause(&self) -> Result<(), String> {
        if self.text("eof-reached").as_deref() == Some("yes") {
            self.seek(0.)?;
            self.set("pause", "no")
        } else {
            self.cmd(&["cycle", "pause"])?;
            self.requested_pause.set(!self.requested_pause.get());
            Ok(())
        }
    }
    pub fn paused_intent(&self) -> bool {
        self.requested_pause.get()
    }
    pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.cmd(&["set", key, value])?;
        if key == "pause" {
            self.requested_pause.set(matches!(value, "yes" | "true"));
        }
        Ok(())
    }

    /// Called in the GL paint callback. Render into a reusable GPU FBO then blit.
    pub fn render_timing_diagnostic(&self) -> serde_json::Value {
        if self.render_timing.is_none() {
            return serde_json::Value::Null;
        }
        serde_json::json!({"timing":self.render_timing,"first_ready_render_ms":self.first_ready_render_ms,
            "render_policy":if self.repeat_render {"every-ui-paint"} else {"mpv-update-or-invalid-fbo"},
            "frame_target_policy":if self.defer_target && !self.repeat_render {"host-defer-keep-blocking"} else if self.bypass_due_wait && !self.repeat_render {"guarded-due-native-fallback"} else {"legacy-block-in-render"},
            "avsync":self.number("avsync"),"total_avsync_change":self.number("total-avsync-change"),
            "frame_drop_count":self.number("frame-drop-count"),
            "decoder_frame_drop_count":self.number("decoder-frame-drop-count"),
            "mistimed_frame_count":self.number("mistimed-frame-count"),
            "vo_delayed_frame_count":self.number("vo-delayed-frame-count")})
    }

    pub fn paint(
        &mut self,
        gl: &glow::Context,
        width: i32,
        height: i32,
        viewport: [i32; 4],
        target: Option<glow::Framebuffer>,
    ) -> Result<(), String> {
        self.paint_clipped(gl, width, height, viewport, target, viewport)
    }
    pub fn paint_clipped(
        &mut self,
        gl: &glow::Context,
        width: i32,
        height: i32,
        viewport: [i32; 4],
        target: Option<glow::Framebuffer>,
        clip: [i32; 4],
    ) -> Result<(), String> {
        if width <= 0 || height <= 0 || self.render.is_null() {
            return Ok(());
        }
        let paint_began = self
            .render_timing
            .as_ref()
            .map(|_| std::time::Instant::now());
        unsafe {
            // egui leaves blending and scissoring enabled for its meshes.
            // libmpv's OpenGL render API requires standard defaults on entry;
            // egui_glow restores its own painting state after this callback.
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            if self.dimensions != (width, height) {
                self.render_content_valid = false;
                if let Some(f) = self.fbo.take() {
                    gl.delete_framebuffer(f);
                }
                if let Some(t) = self.texture.take() {
                    gl.delete_texture(t);
                }
                let texture = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    width,
                    height,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
                let fbo = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(texture),
                    0,
                );
                if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                    return Err(media_browser::ui_text!(msg_72033e09bb7f).into());
                }
                self.texture = Some(texture);
                self.fbo = Some(fbo);
                self.dimensions = (width, height);
            }
            // The pinned render API reports new frames and display changes.
            // Repaint the UI from the existing GPU FBO when video has no update;
            // size/source invalidation still forces a real render. Always blit.
            let frame_update = if self.repeat_render {
                false
            } else {
                (self.render_update)(self.render) & 1 != 0
            };
            self.frame_render_pending |= frame_update;
            let mut should_render =
                self.repeat_render || !self.render_content_valid || self.frame_render_pending;
            let mut frame_info = RenderFrameInfo::default();
            let mut target_ahead_ms = 0.;
            let mut host_delay = None;
            if should_render
                && (self.first_ready_render_ms.is_none()
                    || (!self.repeat_render
                        && (self.defer_target || self.bypass_due_wait || self.render_timing.is_some())))
            {
                let code = (self.render_info)(
                    self.render,
                    Param {
                        kind: 11,
                        data: (&mut frame_info as *mut RenderFrameInfo).cast(),
                    },
                );
                if code >= 0 {
                    let now_ns = (self.mpv_time_ns)(self.handle);
                    if frame_info.flags & 1 != 0 && frame_info.flags & 2 == 0 {
                        target_ahead_ms = frame_info.target_time.saturating_sub(now_ns).max(0)
                            as f64
                            / 1_000_000.;
                    }
                    host_delay = host_video_deadline(frame_info, now_ns);
                }
                if let Some(timing) = &mut self.render_timing {
                    timing.target_info_calls += 1;
                    timing.target_ahead_max_ms = timing.target_ahead_max_ms.max(target_ahead_ms);
                    timing.target_clock_outliers += u64::from(target_ahead_ms > 2000.);
                }
            }
            if should_render && self.defer_target && !self.repeat_render {
                if let (Some(ctx), Some(delay)) = (self._wake.as_deref(), host_delay) {
                    ctx.request_repaint_after(delay);
                    self.frame_render_pending = true;
                    should_render = false;
                    if let Some(timing) = &mut self.render_timing {
                        timing.deadline_deferrals += 1;
                    }
                }
            }
            let recent_seek = self
                .last_seek
                .get()
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(2));
            let mut skip_target_block = false;
            let mut due_lateness_ms = 0.;
            if should_render
                && self.bypass_due_wait
                && !self.repeat_render
                && self.render_content_valid
                && self.loaded.get()
                && !self.paused_intent()
                && !recent_seek
            {
                // Re-read next-frame information immediately before render. If
                // asynchronous state changed, retain native blocking instead.
                let mut fresh = RenderFrameInfo::default();
                let info_result = (self.render_info)(
                    self.render,
                    Param {
                        kind: 11,
                        data: (&mut fresh as *mut RenderFrameInfo).cast(),
                    },
                );
                let now_ns = (self.mpv_time_ns)(self.handle);
                if info_result >= 0
                    && fresh.flags == frame_info.flags
                    && fresh.target_time == frame_info.target_time
                    && video_frame_is_due(fresh, now_ns)
                {
                    skip_target_block = true;
                    due_lateness_ms = now_ns.saturating_sub(fresh.target_time) as f64 / 1_000_000.;
                }
            }
            let mut fbo = Fbo {
                fbo: self.fbo.unwrap().0.get() as i32,
                width,
                height,
                format: glow::RGBA8 as i32,
            };
            let mut flip = 1i32;
            let mut block_for_target = i32::from(!skip_target_block);
            let mut params = [
                Param {
                    kind: 3,
                    data: (&mut fbo as *mut Fbo).cast(),
                },
                Param {
                    kind: 4,
                    data: (&mut flip as *mut i32).cast(),
                },
                Param {
                    kind: if self.bypass_due_wait { 12 } else { 0 },
                    data: (&mut block_for_target as *mut i32).cast(),
                },
                Param {
                    kind: 0,
                    data: ptr::null_mut(),
                },
            ];
            let render_began =
                (should_render && self.render_timing.is_some()).then(std::time::Instant::now);
            let code = if should_render {
                (self.render_fn)(self.render, params.as_mut_ptr())
            } else {
                0
            };
            if should_render && code >= 0 {
                self.render_content_valid = true;
                self.frame_render_pending = false;
            }
            let render_ms = render_began.map(|at| at.elapsed().as_secs_f64() * 1000.);
            let avsync = if self.render_timing.is_some() && !self.paused_intent() {
                self.number("avsync").filter(|v| v.is_finite())
            } else {
                None
            };
            if let (Some(timing), Some(ms)) = (&mut self.render_timing, render_ms) {
                let since_first_paint_ms = timing
                    .first_paint
                    .get_or_insert_with(std::time::Instant::now)
                    .elapsed()
                    .as_secs_f64()
                    * 1000.;
                timing.calls += 1;
                if ms > 20. && timing.long_render_events.len() < 16 {
                    timing
                        .long_render_events
                        .push(serde_json::json!({"call":timing.calls,
                        "since_first_paint_ms":since_first_paint_ms,"render_ms":ms,
                        "load_age_ms":self.load_requested.map(|at|at.elapsed().as_secs_f64()*1000.),
                        "width":width,"height":height,"target_ahead_before_ms":target_ahead_ms,"next_frame_flags":frame_info.flags,"target_block_skipped":skip_target_block,"due_lateness_ms":due_lateness_ms}));
                }
                timing.render_total_ms += ms;
                timing.render_max_ms = timing.render_max_ms.max(ms);
                timing.render_over_20ms += u64::from(ms > 20.);
                timing.last_render_ms = ms;
                timing.render_histogram_2ms[((ms / 2.) as usize).min(31)] += 1;
            }
            if let Some(timing) = &mut self.render_timing {
                timing.paint_calls += 1;
                if should_render && skip_target_block {
                    if timing.guarded_due_calls == 0 {
                        timing.guarded_due_min_lateness_ms = due_lateness_ms;
                    } else {
                        timing.guarded_due_min_lateness_ms =
                            timing.guarded_due_min_lateness_ms.min(due_lateness_ms);
                    }
                    timing.guarded_due_calls += 1;
                }
                timing.reused_frame_blits += u64::from(!should_render && self.render_content_valid);
                // Continue sampling A/V sync on every paint in both policies.
                if let Some(avsync) = avsync {
                    timing.avsync_samples += 1;
                    timing.avsync_abs_max_seconds = timing.avsync_abs_max_seconds.max(avsync.abs());
                }
            }
            if should_render
                && code >= 0
                && self.first_ready_render_ms.is_none()
                && self.ready()
                && self.display_size().is_some()
                && frame_info.flags & 1 != 0
                && frame_info.flags & (2 | 4) == 0
            {
                self.first_ready_render_ms = self
                    .load_requested
                    .map(|at| at.elapsed().as_secs_f64() * 1000.);
            }
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(clip[0], clip[1], clip[2], clip[3]);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, self.fbo);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, target);
            // While the first frame is deferred, leave the UI's existing poster
            // visible rather than blitting an uninitialized or previous-source FBO.
            if self.has_presented_frame() {
                gl.blit_framebuffer(
                    0,
                    0,
                    width,
                    height,
                    viewport[0],
                    viewport[1],
                    viewport[0] + viewport[2],
                    viewport[1] + viewport[3],
                    glow::COLOR_BUFFER_BIT,
                    glow::LINEAR,
                );
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            if let (Some(timing), Some(at)) = (&mut self.render_timing, paint_began) {
                timing.paint_max_ms = timing.paint_max_ms.max(at.elapsed().as_secs_f64() * 1000.);
            }
            if code < 0 {
                Err(media_browser::ui_format!(msg_eccd7255100d, code = code))
            } else {
                Ok(())
            }
        }
    }
    pub fn release_gl(&mut self, gl: &glow::Context) {
        self.render_content_valid = false;
        self.frame_render_pending = false;
        unsafe {
            if !self.render.is_null() {
                (self.render_free)(self.render);
                self.render = ptr::null_mut();
            }
            if let Some(f) = self.fbo.take() {
                gl.delete_framebuffer(f);
            }
            if let Some(t) = self.texture.take() {
                gl.delete_texture(t);
            }
        }
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            if !self.render.is_null() {
                (self.render_free)(self.render);
            }
            (self.destroy)(self.handle);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_frame_abi_and_nanosecond_deadlines_match_pinned_contract() {
        assert_eq!(std::mem::size_of::<RenderFrameInfo>(), 16);
        assert_eq!(std::mem::offset_of!(RenderFrameInfo, target_time), 8);
        let now = 50_000_000_000;
        assert_eq!(
            host_video_deadline(
                RenderFrameInfo {
                    flags: 1,
                    target_time: now + 16_000_000
                },
                now
            ),
            Some(std::time::Duration::from_millis(16))
        );
        assert_eq!(
            host_video_deadline(
                RenderFrameInfo {
                    flags: 3,
                    target_time: now + 16_000_000
                },
                now
            ),
            None
        );
        assert_eq!(
            host_video_deadline(
                RenderFrameInfo {
                    flags: 0,
                    target_time: now + 16_000_000
                },
                now
            ),
            None
        );
        assert_eq!(
            host_video_deadline(
                RenderFrameInfo {
                    flags: 1,
                    target_time: now - 1
                },
                now
            ),
            None
        );
        assert_eq!(
            host_video_deadline(
                RenderFrameInfo {
                    flags: 1,
                    target_time: now + 60_000_000_000
                },
                now
            ),
            None
        );
    }
    #[test]
    fn target_block_bypass_never_applies_to_future_unknown_or_special_frames() {
        let now = 50_000_000_000;
        assert!(video_frame_is_due(
            RenderFrameInfo {
                flags: 1,
                target_time: now
            },
            now
        ));
        assert!(video_frame_is_due(
            RenderFrameInfo {
                flags: 1,
                target_time: now - 1
            },
            now
        ));
        assert!(!video_frame_is_due(
            RenderFrameInfo {
                flags: 1,
                target_time: now + 1
            },
            now
        ));
        assert!(!video_frame_is_due(
            RenderFrameInfo {
                flags: 1,
                target_time: 0
            },
            now
        ));
        for flags in [0, 3, 5, 9, 17] {
            assert!(!video_frame_is_due(
                RenderFrameInfo {
                    flags,
                    target_time: now - 1
                },
                now
            ));
        }
    }
    #[test]
    fn pinned_mpv_clock_functions_use_context_and_consistent_units() {
        let player = Player::new(None, None).unwrap();
        unsafe {
            let micros = player
                ._lib
                .get::<unsafe extern "C" fn(*mut c_void) -> i64>(b"mpv_get_time_us\0")
                .unwrap();
            let before = (player.mpv_time_ns)(player.handle);
            let mid = micros(player.handle) * 1000;
            let after = (player.mpv_time_ns)(player.handle);
            assert!(after >= before && mid >= before - 1000 && mid <= after);
        }
    }

    #[test]
    fn native_release_does_not_finish_while_unload_hook_holds_demuxer() {
        // The locked mpv sets PT_STOP before running on_unload and freeing its
        // demuxer. A separate hook client makes that interval deterministic.
        struct HookClient {
            handle: *mut c_void,
            destroy: unsafe extern "C" fn(*mut c_void),
        }
        impl Drop for HookClient {
            fn drop(&mut self) {
                unsafe { (self.destroy)(self.handle) }
            }
        }
        #[repr(C)]
        struct HookEvent {
            name: *const c_char,
            id: u64,
        }
        let base = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = base.join("target/test-fixtures").join(format!(
            "release-hook-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let copy = root.join("dedicated.mp4");
        std::fs::copy(base.join("test-media/test-h264-aac.mp4"), &copy).unwrap();
        let original = std::fs::read(&copy).unwrap();
        let mut player = Player::new(None, None).unwrap();
        let create = unsafe {
            *player
                ._lib
                .get::<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void>(
                    b"mpv_create_client\0",
                )
                .unwrap()
        };
        let destroy = unsafe {
            *player
                ._lib
                .get::<unsafe extern "C" fn(*mut c_void)>(b"mpv_destroy\0")
                .unwrap()
        };
        let hook_add = unsafe {
            *player
                ._lib
                .get::<unsafe extern "C" fn(*mut c_void, u64, *const c_char, i32) -> i32>(
                    b"mpv_hook_add\0",
                )
                .unwrap()
        };
        let hook_continue = unsafe {
            *player
                ._lib
                .get::<unsafe extern "C" fn(*mut c_void, u64) -> i32>(b"mpv_hook_continue\0")
                .unwrap()
        };
        let hook = HookClient {
            handle: unsafe { create(player.handle, c"release-test".as_ptr()) },
            destroy,
        };
        assert!(!hook.handle.is_null());
        assert_eq!(
            unsafe { hook_add(hook.handle, 701, c"on_unload".as_ptr(), 0) },
            0
        );
        player.load(&copy).unwrap();
        let began = std::time::Instant::now();
        while !player.ready() {
            assert!(began.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        player.unload().unwrap();
        let began = std::time::Instant::now();
        let hook_id = loop {
            player.poll();
            let event = unsafe { &*(player.wait)(hook.handle, 0.) };
            if event.id == 25 && !event.data.is_null() {
                break unsafe { (*(event.data as *const HookEvent)).id };
            }
            assert!(began.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(2));
        };
        let began = std::time::Instant::now();
        while !player.unload_command_done.get() || !player.unload_idle.get() {
            player.poll();
            assert!(began.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let reported_released = player.release_complete().unwrap();
        #[cfg(windows)]
        let held_error = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(&copy)
                .err()
                .and_then(|e| e.raw_os_error())
        };
        #[cfg(not(windows))]
        let held_error: Option<i32> = None;
        std::fs::create_dir_all(base.join("artifacts")).unwrap();
        std::fs::write(base.join("artifacts/release-barrier-hook-state.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "stop_reply":player.unload_command_done.get(),"idle_active":player.unload_idle.get(),
            "on_unload_blocked":true,"reported_released":reported_released,"exclusive_open_error":held_error
        })).unwrap()).unwrap();
        assert!(
            !reported_released,
            "idle-active was published while on_unload still owns the file"
        );
        #[cfg(windows)]
        assert_eq!(held_error, Some(32));
        assert_eq!(unsafe { hook_continue(hook.handle, hook_id) }, 0);
        let began = std::time::Instant::now();
        while !player.release_complete().unwrap() {
            assert!(began.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            drop(
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(&copy)
                    .unwrap(),
            );
        }
        assert_eq!(std::fs::read(&copy).unwrap(), original);
    }
    #[test]
    fn native_async_unload_waits_for_idle_and_releases_dedicated_copies() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = base.join("target/test-fixtures").join(format!(
            "player-release-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut p = Player::new(None, None).unwrap();
        let mut rows = Vec::new();
        for round in 0..6 {
            let source = base.join("test-media").join(if round % 2 == 0 {
                "test-h264-aac.mp4"
            } else {
                "test-mpeg4-mp3.avi"
            });
            let copy = root.join(source.file_name().unwrap());
            std::fs::copy(&source, &copy).unwrap();
            let original = std::fs::read(&copy).unwrap();
            p.load(&copy).unwrap();
            let pending = round >= 3;
            let began = std::time::Instant::now();
            if !pending {
                while !p.ready() {
                    assert!(
                        began.elapsed() < std::time::Duration::from_secs(5),
                        "{:?}",
                        p.error()
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
            p.unload().unwrap();
            let began = std::time::Instant::now();
            while !p.release_complete().unwrap() {
                assert!(began.elapsed() < std::time::Duration::from_secs(5));
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                let exclusive = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(&copy)
                    .unwrap();
                drop(exclusive);
            }
            assert_eq!(std::fs::read(&copy).unwrap(), original);
            rows.push(serde_json::json!({"round":round,"pending_load_cancel":pending,"exclusive_open_after_release":true,"release_ms":began.elapsed().as_secs_f64()*1000.}));
        }
        std::fs::create_dir_all(base.join("artifacts")).unwrap();
        std::fs::write(base.join("artifacts/p4-player-release.json"),serde_json::to_vec_pretty(&serde_json::json!({"passed":true,"scope":"Headless native mpv release acknowledgment plus exclusive open on dedicated MP4/AVI copies; not GPU/UI proof","rows":rows})).unwrap()).unwrap();
    }
    #[test]
    fn command_reply_entry_id_matches_locked_c_node_layout_and_rejects_other_types() {
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(std::mem::size_of::<Node>(), 16);
            assert_eq!(std::mem::size_of::<NodeList>(), 24);
        }
        let keys = [c"playlist_entry_id".as_ptr()];
        let values = [Node {
            value: NodeValue { integer: 42 },
            format: 4,
        }];
        let mut list = NodeList {
            count: 1,
            values: values.as_ptr(),
            keys: keys.as_ptr(),
        };
        let mut root = Node {
            value: NodeValue { list: &list },
            format: 8,
        };
        unsafe {
            assert_eq!(entry_from_reply((&root as *const Node).cast()), Some(42));
            root.format = 0;
            assert_eq!(entry_from_reply((&root as *const Node).cast()), None);
            root.format = 8;
            list.count = -1;
            std::hint::black_box(&list);
            assert_eq!(entry_from_reply((&root as *const Node).cast()), None);
            assert_eq!(entry_from_reply(ptr::null()), None);
        }
    }
}
