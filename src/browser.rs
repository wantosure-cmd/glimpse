//! Native browser UI. Expensive enumeration, decode, sort, and layout live in workers.
#[path = "browser_animation_smoke.rs"]
mod animation_smoke;
#[path = "browser_components.rs"]
mod components;
#[path = "browser_controls.rs"]
mod controls;
#[path = "browser_daily_smoke.rs"]
mod daily_smoke;
#[path = "browser_video_switch_smoke.rs"]
mod video_switch_smoke;
#[path = "browser_design_smoke.rs"]
mod design_smoke;
#[path = "browser_detail_smoke.rs"]
mod detail_smoke;
#[path = "browser_detail.rs"]
mod detail_ui;
#[path = "browser_edit_smoke.rs"]
mod edit_smoke;
#[path = "browser_empty_fold_smoke.rs"]
mod empty_fold_smoke;
#[path = "browser_features.rs"]
mod features;
#[path = "browser_features_smoke.rs"]
mod features_smoke;
#[path = "browser_folders.rs"]
mod folders;
#[path = "browser_fonts.rs"]
mod fonts;
#[path = "browser_format_smoke.rs"]
mod format_smoke;
#[path = "browser_gif_smoke.rs"]
mod gif_smoke;
#[path = "browser_input_smoke.rs"]
mod input_smoke;
#[cfg(windows)]
#[path = "browser_instance.rs"]
mod instance_ui;
#[path = "browser_loading_smoke.rs"]
mod loading_smoke;
#[path = "browser_loading.rs"]
mod loading_ui;
#[path = "browser_media_jump_smoke.rs"]
mod media_jump_smoke;
#[path = "browser_ui.rs"]
mod modern_ui;
#[path = "browser_motion.rs"]
mod motion;
#[path = "browser_navigation_smoke.rs"]
mod navigation_smoke;
#[path = "browser_playback_smoke.rs"]
mod playback_smoke;
#[path = "browser_presentation_smoke.rs"]
mod presentation_smoke;
#[path = "browser_presentation.rs"]
mod presentation_ui;
#[path = "browser_preview_failures.rs"]
mod preview_failures;
#[path = "browser_rotate.rs"]
mod rotate_ui;
#[path = "browser_seek_benchmark.rs"]
mod seek_benchmark;
#[path = "browser_subtitles.rs"]
mod subtitle_ui;
#[path = "browser_trash_smoke.rs"]
mod trash_smoke;
#[path = "browser_trash.rs"]
mod trash_ui;
#[path = "browser_watch_smoke.rs"]
mod watch_smoke;
use crate::{
    playback::Player,
    thumbnails::{ThumbJob, ThumbnailService},
};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use media_browser::{
    domain::*,
    input::{Action, Context as InputContext, Hit, Input, InputRouter, KeyLatch},
    library::{LibraryEvent, LibraryService},
    playback_control::{SeekQueue, SeekRequest},
    prepare::{PrepareRequest, PrepareService, Prepared},
    view::{
        self, MediaFilter, NavigationHistory, NavigationState, ScrollAnchor, SortField,
        ViewSettings,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[path = "browser_icons.rs"]
mod icons;
#[path = "browser_workspace.rs"]
mod workspace_ui;

type Proc = Arc<dyn Fn(&std::ffi::CStr) -> *const std::ffi::c_void + Send + Sync>;
struct TextureEntry {
    texture: egui::TextureHandle,
    bytes: usize,
    last_frame: u64,
    source_size: (u32, u32),
}

#[derive(Default)]
struct DimensionHints {
    generation: u64,
    versions: HashMap<MediaId, Fingerprint>,
    order: VecDeque<MediaId>,
}
impl DimensionHints {
    fn remember(&mut self, generation: u64, id: MediaId, fingerprint: Fingerprint) {
        if self.generation != generation {
            self.generation = generation;
            self.versions.clear();
            self.order.clear();
        }
        if self.versions.get(&id) == Some(&fingerprint) {
            return;
        }
        self.order.retain(|old| *old != id);
        if self.versions.len() == 256 && !self.versions.contains_key(&id) {
            if let Some(old) = self.order.pop_front() {
                self.versions.remove(&old);
            }
        }
        self.versions.insert(id, fingerprint);
        self.order.push_back(id);
    }
    fn contains(&self, generation: u64, id: MediaId, fingerprint: &Fingerprint) -> bool {
        self.generation == generation && self.versions.get(&id) == Some(fingerprint)
    }
}

fn viewport_geometry_requests(
    layout: &media_browser::layout::MasonryLayout,
    top: f32,
    bottom: f32,
    limit: usize,
    label_only: bool,
    already_probed: impl Fn(MediaId) -> bool,
) -> Vec<(MediaId, u8)> {
    let probe_top = |row: &media_browser::layout::SizingRow| {
        if label_only {
            row.rect.y
        } else {
            row.probe_top
        }
    };
    let visible = |row: &&media_browser::layout::SizingRow| {
        row.rect.bottom() >= top && probe_top(row) <= bottom
    };
    let near = layout.sizing_rows.iter().filter(|row| {
        !visible(row)
            && row.rect.bottom() >= top - (bottom - top)
            && probe_top(row) <= bottom + (bottom - top)
    });
    layout
        .sizing_rows
        .iter()
        .filter(visible)
        .map(|row| (row, 2))
        .chain(near.map(|row| (row, 4)))
        .flat_map(|(row, priority)| row.items.iter().map(move |id| (*id, priority)))
        .filter(|(id, _)| !already_probed(*id))
        .take(limit)
        .collect()
}

#[cfg(test)]
fn trim_inactive_list_textures(
    textures: &mut HashMap<String, TextureEntry>,
    bytes: usize,
    frame: u64,
) -> usize {
    trim_inactive_list_textures_with_policy(textures, bytes, frame, false)
}

fn trim_inactive_list_textures_with_policy(
    textures: &mut HashMap<String, TextureEntry>,
    mut bytes: usize,
    frame: u64,
    compact_retention: bool,
) -> usize {
    // Keep current/previous-frame handles, including newly uploaded prefetch.
    // A complete UI frame also lets the request deduplicator observe cache hits
    // before an inactive entry can be retired and requested again.
    let recent_frame = frame.saturating_sub(1);
    let hot_bytes: usize = textures
        .values()
        .filter(|entry| entry.last_frame >= recent_frame)
        .map(|entry| entry.bytes)
        .sum();
    let list_budget = if compact_retention {
        hot_bytes
            .saturating_add(16 * 1024 * 1024)
            .clamp(32 * 1024 * 1024, 128 * 1024 * 1024)
    } else {
        hot_bytes
            .saturating_mul(2)
            .clamp(32 * 1024 * 1024, 256 * 1024 * 1024)
    };
    while bytes > list_budget {
        let Some(key) = textures
            .iter()
            .filter(|(_, entry)| entry.last_frame < recent_frame)
            .min_by_key(|(_, entry)| entry.last_frame)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        if let Some(entry) = textures.remove(&key) {
            bytes = bytes.saturating_sub(entry.bytes);
        }
    }
    bytes
}

#[derive(Clone)]
struct Viewer {
    id: MediaId,
    zoom: f32,
    pan: Vec2,
    native_pending: bool,
    center_thumbnail: bool,
    end_handled: bool,
}
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
enum VideoEndAction {
    #[default]
    Next,
    Replay,
    Pause,
}
impl VideoEndAction {
    fn label(self) -> &'static str {
        match self {
            Self::Next => media_browser::ui_text!(msg_65ed9776572d),
            Self::Replay => media_browser::ui_text!(msg_4026b79d595e),
            Self::Pause => media_browser::ui_text!(msg_7d7a771f65a6),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    auto_fold_empty: bool,
    config_version: u32,
    language: String,
    places: media_browser::places::Places,
    presentation: media_browser::presentation::Preferences,
    appearance: workspace_ui::Appearance,
    subtitles: media_browser::subtitles::SubtitleStyle,
    video_end_action: VideoEndAction,
    settings: ViewSettings,
    shortcuts: media_browser::shortcuts::DetailShortcuts,
    target_width: f32,
    pinned: bool,
    last_root: Option<PathBuf>,
    volume: f32,
    muted: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: 0,
            auto_fold_empty: true,
            language: "zh-CN".into(),
            places: Default::default(),
            presentation: Default::default(),
            appearance: Default::default(),
            subtitles: Default::default(),
            video_end_action: VideoEndAction::default(),
            shortcuts: Default::default(),
            settings: ViewSettings::default(),
            target_width: 260.,
            pinned: false,
            last_root: None,
            volume: 70.,
            muted: false,
        }
    }
}

pub struct Browser {
    daily_smoke: Option<daily_smoke::DailySmoke>,
    video_switch_smoke: Option<video_switch_smoke::VideoSwitchSmoke>,
    loading: loading_ui::LoadingState,
    loading_smoke: Option<loading_smoke::LoadingSmoke>,
    auto_fold_empty: bool,
    automatic_collapsed: Arc<HashSet<PathBuf>>,
    auto_expanded: HashSet<PathBuf>,
    tree_auto_expanded: HashSet<PathBuf>,
    list_scrollbar: Option<Rect>,
    empty_fold_smoke: Option<empty_fold_smoke::EmptyFoldSmoke>,
    folder_toggle_regions: HashMap<PathBuf, Rect>,
    features: features::Features,
    features_smoke: Option<features_smoke::FeaturesSmoke>,
    language: String,
    visit_pending: bool,
    load_metrics: Option<(PathBuf, Vec<serde_json::Value>, Duration)>,
    load_transition_metrics: Option<Vec<serde_json::Value>>,
    places: media_browser::places::Places,
    workspace: workspace_ui::Workspace,
    permanent_prompt: Option<trash_ui::PermanentPrompt>,
    permanent_controls: Option<(Rect, Rect, bool)>,
    sticky_paths: Vec<PathBuf>,
    sticky_jump_up: bool,
    media_jump_smoke: Option<media_jump_smoke::MediaJumpSmoke>,
    sticky_jump_button: Option<Rect>,
    sticky_jump_target: Option<PathBuf>,
    list_viewport: Option<Rect>,
    pending_folder_location: Option<PathBuf>,
    folder_location_anchor: Option<PathBuf>,
    folder_title_regions: HashMap<PathBuf, Rect>,
    sticky_toggle_regions: HashMap<PathBuf, Rect>,
    navigation_smoke: Option<navigation_smoke::NavigationSmoke>,
    design_smoke: Option<design_smoke::DesignSmoke>,
    presentation_smoke: Option<presentation_smoke::PresentationSmoke>,
    motion_rect: Option<Rect>,
    list_regions: HashMap<MediaId, Rect>,
    media_transition: Option<motion::MediaTransition>,
    progressive_preview_paints: u64,
    folder_smoke: Option<folders::FolderSmoke>,
    selected_folders: HashSet<PathBuf>,
    pending_folders: Option<folders::PendingFolders>,
    folder_prepare: Option<folders::FolderPreparation>,
    shell_state: Option<bool>,
    shell_result: Option<std::sync::mpsc::Receiver<Result<bool, String>>>,
    video_prefetch: media_browser::video_prefetch::VideoPrefetch,
    subtitles: media_browser::subtitles::SubtitleStyle,
    subtitle_picker: Option<std::sync::mpsc::Receiver<(MediaId, Option<PathBuf>)>>,
    subtitle_notice: Option<String>,
    detail_layout: detail_ui::DetailLayout,
    detail_smoke: Option<detail_smoke::DetailSmoke>,
    video_end_action: VideoEndAction,
    presentation: presentation_ui::Presentation,
    edit_smoke: Option<edit_smoke::EditSmoke>,
    shortcuts: media_browser::shortcuts::DetailShortcuts,
    rotation: Option<rotate_ui::RotationOperation>,
    rotation_ids: HashSet<MediaId>,
    rotation_message: Option<String>,
    format_smoke: Option<format_smoke::FormatSmoke>,
    seek_benchmark: Option<seek_benchmark::SeekBenchmark>,
    window_recovery: media_browser::window_recovery::WindowRecovery,
    window_recovery_enabled: bool,
    #[cfg(windows)]
    instance: Option<instance_ui::Bridge>,
    trash_smoke: Option<trash_smoke::TrashSmoke>,
    trash_service: media_browser::trash::TrashService,
    trash_operation: Option<trash_ui::TrashOperation>,
    pending_trash: Option<trash_ui::PendingTrash>,
    trash_undo_panel: Option<Rect>,
    trash_undo_button: Option<Rect>,
    trash_counter: u64,
    trash_message: Option<String>,
    trash_failures: Vec<(PathBuf, String)>,
    last_trash: Option<serde_json::Value>,
    watch_smoke: Option<watch_smoke::WatchSmoke>,
    input_smoke: Option<input_smoke::InputSmoke>,
    visible_videos: HashMap<MediaId, (Rect, Rect)>,
    last_hover_delay_ms: Option<f64>,
    player_version: Option<Fingerprint>,
    load_started: Option<Instant>,
    playback_errors: HashMap<MediaId, (Fingerprint, String)>,
    gif_smoke: Option<gif_smoke::GifSmoke>,
    animation_smoke: Option<animation_smoke::AnimationSmoke>,
    animation: crate::gif_viewer::GifViewer,
    list_animations: crate::list_animation_viewer::ListAnimationViewer,
    list_animation_requests: Vec<media_browser::gif_previews::Request>,
    animated_images: HashMap<MediaId, media_browser::domain::Fingerprint>,
    playback_smoke: Option<playback_smoke::PlaybackSmoke>,
    input_router: InputRouter,
    seeks: SeekQueue,
    seek_flight: Option<(SeekRequest, Instant)>,
    player_media: Option<MediaId>,
    positions: HashMap<MediaId, (Fingerprint, f64)>,
    resume_pending: Option<f64>,
    resume_starting: bool,
    resume_pause_override: Option<bool>,
    restore_flight: bool,
    volume: f32,
    muted: bool,
    hover_candidate: Option<(MediaId, Instant)>,
    preview_active: Option<MediaId>,
    preview_idle: Option<Instant>,
    hovered_video: Option<MediaId>,
    video_region: Option<(MediaId, Rect)>,
    video_retry_button: Option<(MediaId, Rect)>,
    image_region: Option<Rect>,
    video_exclusion: Option<Rect>,
    routed_zoom: f32,
    control_focused: bool,
    control_regions: Vec<Rect>,
    key_latch: KeyLatch,
    seek_notice: Option<(MediaId, String, Instant)>,
    benchmark: Option<crate::benchmark::ScrollBenchmark>,
    ctx: egui::Context,
    library: LibraryService,
    watcher: media_browser::watch::WatchService,
    watch_epoch: u64,
    watch_root: Option<PathBuf>,
    watch_source_root: Option<PathBuf>,
    watch_warning: Option<String>,
    index_notice: Option<String>,
    refresh_pending: bool,
    scan_busy: bool,
    was_focused: bool,
    preparer: PrepareService,
    thumbnails: ThumbnailService,
    dimension_hints: DimensionHints,
    proc: Proc,
    player: Option<Arc<Mutex<Player>>>,
    video_turns: HashMap<MediaId, (media_browser::domain::Fingerprint, u16)>,
    shell_message: Option<String>,
    snapshot: Option<Arc<LibrarySnapshot>>,
    prepared: Option<Arc<Prepared>>,
    sidebar: Option<Arc<LibrarySnapshot>>,
    tree_collapsed: HashSet<PathBuf>,
    generation: u64,
    revision: u64,
    prepare_dirty: bool,
    prepare_after: Instant,
    last_width: f32,
    target_width: f32,
    settings: ViewSettings,
    collapsed: HashSet<PathBuf>,
    folder_resume_anchors: HashMap<PathBuf, ScrollAnchor>,
    history: NavigationHistory,
    selection: HashSet<MediaId>,
    detail: Option<Viewer>,
    deferred_detail: Option<MediaId>,
    hidden_pause: Option<(MediaId, bool)>,
    root_text: String,
    path_editing: bool,
    detail_open_frame: u64,
    selection_anchor: Option<MediaId>,
    preview_controls_rect: Option<Rect>,
    preview_seek_rect: Option<Rect>,
    preview_size_rect: Option<Rect>,
    status: String,
    scroll_y: f32,
    viewport_height: f32,
    scroll_override: Option<f32>,
    pending_anchor: Option<ScrollAnchor>,
    textures: HashMap<String, TextureEntry>,
    texture_bytes: usize,
    frame: u64,
    frame_textures: Vec<egui::TextureHandle>,
    failures: preview_failures::PreviewFailures,
    cache_hits: u64,
    decoded: u64,
    adaptive_photo_uploads: bool,
    compact_list_retention: bool,
    adaptive_video_posters: bool,
    extra_video_posters: u64,
    upload_extra_total: u64,
    upload_max_per_frame: usize,
    upload_extra_bytes_max: usize,
    upload_prepare_max_ms: f64,
    pinned: bool,
    sidebar_open: bool,
    sidebar_button_rect: Option<Rect>,
    sidebar_overlay_rect: Option<Rect>,
    sidebar_pin_rect: Option<Rect>,
    edge_since: Option<Instant>,
    outside_since: Option<Instant>,
    data_dir: PathBuf,
    initial_root: Option<PathBuf>,
    initial_open: Option<(PathBuf, u64)>,
    first_frame: bool,
    first_ui_report: Option<PathBuf>,
    first_ui_ms: Option<u128>,
    started: Instant,
    smoke: bool,
    smoke_step: u8,
    smoke_at: Instant,
    smoke_rows: Vec<serde_json::Value>,
    quit_after: Option<Duration>,
    fullscreen: bool,
    video_fullscreen: Option<bool>,
    video_fullscreen_activity: Instant,
    font_receiver: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
    full_font_loaded: bool,
}
/// Reuse the existing index worker while the native window/GL context is created.
/// Results stay in its bounded queue until navigation adopts this generation.
pub(crate) struct StartupLibrary {
    data_dir: PathBuf,
    config: Config,
    root: Option<PathBuf>,
    library: LibraryService,
    generation: Option<u64>,
    repaint: std::sync::Arc<std::sync::Mutex<Option<egui::Context>>>,
}
impl StartupLibrary {
    pub(crate) fn start(args: &[String]) -> Self {
        let (data_dir, config, root) = startup_config(args);
        let repaint = std::sync::Arc::new(std::sync::Mutex::new(None::<egui::Context>));
        let wake = repaint.clone();
        let library = LibraryService::new_recovering_excluding(
            data_dir.join("index.sqlite"),
            vec![data_dir.clone()],
            move || {
                if let Some(ctx) = wake.lock().unwrap().as_ref() {
                    ctx.request_repaint();
                }
            },
        );
        let generation = root.as_ref().map(|path| library.open(path));
        crate::startup_trace::mark("startup_library_open_queued");
        Self {
            data_dir,
            config,
            root,
            library,
            generation,
            repaint,
        }
    }
}
fn startup_config(args: &[String]) -> (PathBuf, Config, Option<PathBuf>) {
    let argument = |key: &str| {
        args.iter()
            .position(|s| s == key)
            .and_then(|i| args.get(i + 1))
    };
    let data_dir = argument("--data-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_data_dir);
    let mut config = std::fs::read(data_dir.join("settings.json"))
        .ok()
        .filter(|b| b.len() < 256 * 1024)
        .and_then(|b| serde_json::from_slice::<Config>(&b).ok())
        .unwrap_or_default();
    if config.config_version < 2 {
        config.appearance.compact = false;
    }
    let root = argument("--open")
        .map(PathBuf::from)
        .or_else(|| config.last_root.clone());
    (data_dir, config, root)
}
impl Browser {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        args: Vec<String>,
        started: Instant,
        startup: Option<StartupLibrary>,
    ) -> Self {
        crate::startup_trace::mark("browser_new_entry");
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        modern_ui::apply_theme(&cc.egui_ctx);
        cc.egui_ctx.set_fonts(fonts::definitions(None));
        crate::startup_trace::mark("browser_fonts_queued");
        let argument = |key: &str| {
            args.iter()
                .position(|s| s == key)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let (data_dir, config, initial_root, library, initial_open) = if let Some(startup) = startup
        {
            *startup.repaint.lock().unwrap() = Some(cc.egui_ctx.clone());
            let initial_open = startup.root.clone().zip(startup.generation);
            (
                startup.data_dir,
                startup.config,
                startup.root,
                startup.library,
                initial_open,
            )
        } else {
            let (data_dir, config, root) = startup_config(&args);
            let ctx = cc.egui_ctx.clone();
            let library = LibraryService::new_recovering_excluding(
                data_dir.join("index.sqlite"),
                vec![data_dir.clone()],
                move || ctx.request_repaint(),
            );
            (data_dir, config, root, library, None)
        };
        media_browser::ui_text::set_english(config.language == "en");
        crate::startup_trace::mark("browser_config_loaded");
        crate::startup_trace::mark("browser_library_started");
        let ctx = cc.egui_ctx.clone();
        let watcher = media_browser::watch::WatchService::new(move || ctx.request_repaint());
        let ctx = cc.egui_ctx.clone();
        let preparer = PrepareService::new_with_group_index(
            move || ctx.request_repaint(),
            !args.iter().any(|s| s == "--prepare-legacy-completed-cache"),
            args.iter().any(|s| s == "--prepare-cached-sort-keys"),
            !args.iter().any(|s| s == "--prepare-legacy-group-index"),
        );
        let ctx = cc.egui_ctx.clone();
        let thumbnails =
            ThumbnailService::new(data_dir.join("thumbnails"), move || ctx.request_repaint());
        let trash_ctx = cc.egui_ctx.clone();
        crate::startup_trace::mark("browser_workers_started");
        Self {
            features: features::Features::new(&data_dir),
            daily_smoke: argument("--daily-smoke").map(|p| daily_smoke::DailySmoke::new(PathBuf::from(p), args.iter().any(|s|s=="--daily-restart"), args.iter().any(|s|s=="--daily-scan-exit"))),
            video_switch_smoke: argument("--video-switch-smoke").map(|p| video_switch_smoke::VideoSwitchSmoke::new(PathBuf::from(p), argument("--switch-sources").map(PathBuf::from))),
            features_smoke: argument("--features-smoke")
                .map(|p| features_smoke::FeaturesSmoke::new(PathBuf::from(p))),
            language: config.language,
            detail_layout: Default::default(),
            detail_smoke: argument("--detail-smoke")
                .map(|p| detail_smoke::DetailSmoke::new(PathBuf::from(p))),
            edit_smoke: argument("--edit-smoke")
                .map(|p| {
                    edit_smoke::EditSmoke::new(
                        PathBuf::from(p),
                        false,
                        args.iter().any(|a| a == "--edit-smoke-controlled-focus"),
                    )
                })
                .or_else(|| {
                    argument("--cloud-smoke").map(|p| {
                        edit_smoke::EditSmoke::new(PathBuf::from(p), true, false)
                            .with_source(argument("--cloud-smoke-source").map(PathBuf::from))
                    })
                }),
            shortcuts: config.shortcuts,
            rotation: None,
            rotation_ids: HashSet::new(),
            rotation_message: None,
            #[cfg(windows)]
            instance: None,
            trash_smoke: args
                .iter()
                .any(|s| s == "--trash-smoke")
                .then(trash_smoke::TrashSmoke::new),
            trash_service: media_browser::trash::TrashService::new(move || {
                trash_ctx.request_repaint()
            }),
            trash_operation: None,
            pending_trash: None,
            trash_undo_panel: None,
            trash_undo_button: None,
            trash_counter: 0,
            format_smoke: argument("--format-smoke").map(|path| {
                format_smoke::FormatSmoke::new(PathBuf::from(path))
                    .with_motion_probe(args.iter().any(|arg| arg == "--format-motion-probe"))
            }),
            seek_benchmark: argument("--seek-benchmark")
                .map(PathBuf::from)
                .map(seek_benchmark::SeekBenchmark::new),
            window_recovery: Default::default(),
            window_recovery_enabled: !args
                .iter()
                .any(|a| matches!(a.as_str(), "--benchmark-scroll" | "--seek-benchmark")),
            trash_message: None,
            trash_failures: Vec::new(),
            last_trash: None,
            watch_smoke: args
                .iter()
                .any(|s| s == "--watch-smoke")
                .then(watch_smoke::WatchSmoke::new),
            input_smoke: args
                .iter()
                .any(|s| s == "--input-smoke")
                .then(input_smoke::InputSmoke::new),
            visible_videos: HashMap::new(),
            last_hover_delay_ms: None,
            player_version: None,
            load_started: None,
            playback_errors: HashMap::new(),
            gif_smoke: args
                .iter()
                .any(|s| s == "--gif-smoke")
                .then(gif_smoke::GifSmoke::new),
            animation: crate::gif_viewer::GifViewer::new(cc.egui_ctx.clone()),
            animation_smoke: argument("--animation-smoke")
                .map(PathBuf::from)
                .map(animation_smoke::AnimationSmoke::new),
            list_animations: crate::list_animation_viewer::ListAnimationViewer::new(
                cc.egui_ctx.clone(),
            ),
            list_animation_requests: Vec::new(),
            animated_images: HashMap::new(),
            playback_smoke: args
                .iter()
                .any(|s| s == "--playback-smoke")
                .then(playback_smoke::PlaybackSmoke::new),
            loading: loading_ui::LoadingState::default(),
            loading_smoke: argument("--loading-smoke")
                .map(PathBuf::from)
                .map(loading_smoke::LoadingSmoke::new),
            input_router: InputRouter::default(),
            seeks: SeekQueue::default(),
            seek_flight: None,
            player_media: None,
            positions: HashMap::new(),
            resume_pending: None,
            resume_starting: false,
            resume_pause_override: None,
            restore_flight: false,
            volume: if config.volume.is_finite() {
                config.volume.clamp(0., 400.)
            } else {
                70.
            },
            muted: config.muted,
            video_prefetch: Default::default(),
            subtitles: {
                let mut s = config.subtitles;
                s.normalize();
                s
            },
            subtitle_picker: None,
            subtitle_notice: None,
            hover_candidate: None,
            preview_active: None,
            preview_idle: None,
            hovered_video: None,
            video_region: None,
            video_retry_button: None,
            image_region: None,
            video_exclusion: None,
            routed_zoom: 0.,
            control_focused: false,
            control_regions: Vec::new(),
            key_latch: KeyLatch::default(),
            seek_notice: None,
            benchmark: crate::benchmark::ScrollBenchmark::from_args(&args),
            ctx: cc.egui_ctx.clone(),
            library,
            watcher,
            watch_epoch: 0,
            watch_root: None,
            watch_source_root: None,
            watch_warning: None,
            index_notice: None,
            refresh_pending: false,
            scan_busy: false,
            was_focused: false,
            preparer,
            thumbnails,
            dimension_hints: DimensionHints::default(),
            proc: cc
                .get_proc_address
                .clone()
                .expect(media_browser::ui_text!(msg_eecb8317e320)),
            player: None,
            video_turns: HashMap::new(),
            shell_message: None,
            snapshot: None,
            prepared: None,
            sidebar: None,
            tree_collapsed: HashSet::new(),
            auto_fold_empty: config.auto_fold_empty,
            automatic_collapsed: Arc::new(HashSet::new()),
            auto_expanded: HashSet::new(),
            tree_auto_expanded: HashSet::new(),
            list_scrollbar: None,
            empty_fold_smoke: argument("--empty-fold-smoke").map(PathBuf::from).map(|p| {
                empty_fold_smoke::EmptyFoldSmoke::new(
                    p,
                    args.iter().any(|a| a == "--verify-fold-disabled"),
                )
            }),
            folder_toggle_regions: HashMap::new(),
            generation: 0,
            revision: 0,
            prepare_dirty: false,
            prepare_after: Instant::now(),
            last_width: 0.,
            target_width: config
                .target_width
                .clamp(160., media_browser::layout::MAX_PREVIEW_WIDTH),
            places: config.places,
            visit_pending: true,
            load_transition_metrics: args
                .iter()
                .any(|s| s == "--load-metrics-transitions")
                .then(Vec::new),
            load_metrics: argument("--load-metrics")
                .map(|p| (PathBuf::from(p), Vec::new(), Duration::ZERO)),
            workspace: workspace_ui::Workspace {
                appearance: config.appearance.clone(),
                ..Default::default()
            },
            settings: config.settings,
            video_end_action: config.video_end_action,
            presentation: presentation_ui::Presentation::new(config.presentation),
            collapsed: HashSet::new(),
            folder_resume_anchors: HashMap::new(),
            history: NavigationHistory::default(),
            selection: HashSet::new(),
            detail: None,
            deferred_detail: None,
            hidden_pause: None,
            path_editing: false,
            detail_open_frame: 0,
            selection_anchor: None,
            preview_controls_rect: None,
            preview_seek_rect: None,
            preview_size_rect: None,
            root_text: initial_root
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            status: media_browser::ui_text!(msg_9ff32fab7ec0).into(),
            scroll_y: 0.,
            viewport_height: 600.,
            scroll_override: None,
            pending_anchor: None,
            permanent_prompt: None,
            permanent_controls: None,
            sticky_paths: Vec::new(),
            sticky_jump_up: false,
            media_jump_smoke: argument("--media-jump-smoke")
                .map(PathBuf::from)
                .map(media_jump_smoke::MediaJumpSmoke::new),
            sticky_jump_button: None,
            sticky_jump_target: None,
            list_viewport: None,
            pending_folder_location: None,
            folder_location_anchor: None,
            folder_title_regions: HashMap::new(),
            sticky_toggle_regions: HashMap::new(),
            navigation_smoke: args
                .iter()
                .position(|s| s == "--navigation-smoke")
                .and_then(|i| args.get(i + 1))
                .map(|s| navigation_smoke::NavigationSmoke::new(PathBuf::from(s))),
            presentation_smoke: argument("--presentation-smoke")
                .map(|p| presentation_smoke::PresentationSmoke::new(PathBuf::from(p))),
            design_smoke: argument("--design-smoke")
                .map(|p| design_smoke::DesignSmoke::new(PathBuf::from(p)))
                .or_else(|| argument("--detail-ui-smoke")
                    .map(|p| design_smoke::DesignSmoke::new_detail_ui(PathBuf::from(p))))
                .or_else(|| argument("--folder-sort-smoke")
                    .map(|p| design_smoke::DesignSmoke::new_folder_sort(PathBuf::from(p))))
                .or_else(|| {
                    argument("--resize-only-smoke")
                        .map(|p| design_smoke::DesignSmoke::new_resize(PathBuf::from(p)))
                }),
            motion_rect: None,
            list_regions: HashMap::new(),
            media_transition: None,
            progressive_preview_paints: 0,
            folder_smoke: argument("--folder-smoke").map(|p| {
                folders::FolderSmoke::new(PathBuf::from(p))
                    .with_partial(args.iter().any(|a| a == "--confirm-permanent-failure"))
                    .with_preview_only(args.iter().any(|a| a == "--progressive-preview-only"))
            }),
            selected_folders: HashSet::new(),
            pending_folders: None,
            folder_prepare: None,
            shell_state: None,
            shell_result: None,
            textures: HashMap::new(),
            texture_bytes: 0,
            frame: 0,
            frame_textures: Vec::new(),
            failures: preview_failures::PreviewFailures::default(),
            cache_hits: 0,
            decoded: 0,
            adaptive_photo_uploads: std::env::var_os("GLIMPSE_LEGACY_TWO_UPLOADS").as_deref()
                != Some(std::ffi::OsStr::new("1")),
            compact_list_retention: std::env::var_os("GLIMPSE_KEEP_LEGACY_LIST_RETENTION")
                .as_deref()
                != Some(std::ffi::OsStr::new("1")),
            adaptive_video_posters: std::env::var_os("GLIMPSE_KEEP_LEGACY_POSTER_UPLOADS")
                .as_deref()
                != Some(std::ffi::OsStr::new("1")),
            extra_video_posters: 0,
            upload_extra_total: 0,
            upload_max_per_frame: 0,
            upload_extra_bytes_max: 0,
            upload_prepare_max_ms: 0.,
            pinned: config.pinned,
            sidebar_open: false,
            sidebar_button_rect: None,
            sidebar_overlay_rect: None,
            sidebar_pin_rect: None,
            edge_since: None,
            outside_since: None,
            data_dir,
            initial_root,
            initial_open,
            first_frame: true,
            first_ui_report: argument("--first-ui-report").map(PathBuf::from),
            first_ui_ms: None,
            started,
            smoke: args.iter().any(|s| s == "--browser-smoke"),
            smoke_step: 0,
            smoke_at: Instant::now(),
            smoke_rows: Vec::new(),
            quit_after: argument("--quit-after")
                .and_then(|s| s.parse().ok())
                .map(Duration::from_millis),
            fullscreen: args.iter().any(|s| s == "--fullscreen"),
            video_fullscreen: None,
            video_fullscreen_activity: Instant::now(),
            font_receiver: None,
            full_font_loaded: false,
        }
    }
    fn current_state(&self) -> Option<NavigationState> {
        let p = self.prepared.as_ref()?;
        Some(NavigationState {
            root: p.snapshot.root.clone(),
            settings: self.settings.clone(),
            collapsed: self.collapsed.clone(),
            auto_expanded: self.auto_expanded.clone(),
            anchor: p.layout.anchor(self.scroll_y),
        })
    }
    fn navigate(&mut self, root: PathBuf, preserve_tree: bool) {
        self.folder_location_anchor = None;
        if let Some(state) = self.current_state() {
            self.history.replace_current(state);
        }
        let state = NavigationState {
            root: root.clone(),
            settings: self.settings.clone(),
            collapsed: HashSet::new(),
            auto_expanded: HashSet::new(),
            anchor: None,
        };
        self.history.visit(state.clone());
        self.navigate_state(state, preserve_tree);
    }
    fn navigate_state(&mut self, state: NavigationState, preserve_tree: bool) {
        self.loading.scan_error = None;
        self.list_animations.clear();
        self.animated_images.clear();
        self.visit_pending = true;
        self.close_detail();
        self.remember_position();
        if let Some(p) = &self.player {
            let _ = p.lock().unwrap().unload();
        }
        self.player_media = None;
        self.preview_active = None;
        self.hover_candidate = None;
        self.seeks.bind(None);
        self.seek_flight = None;
        self.resume_pending = None;
        self.load_started = None;
        self.resume_starting = false;
        self.restore_flight = false;
        self.video_region = None;
        self.image_region = None;
        self.input_router.reset();
        self.selected_folders.clear();
        self.settings = state.settings;
        self.collapsed = state.collapsed;
        self.auto_expanded = state.auto_expanded;
        self.automatic_collapsed = Arc::new(HashSet::new());
        self.pending_anchor = state.anchor;
        self.selection.clear();
        self.failures.clear();
        self.snapshot = None;
        self.prepared = None;
        let tree_root = if preserve_tree {
            self.sidebar.as_ref().map(|s| s.root.clone())
        } else {
            None
        };
        self.generation = self
            .initial_open
            .take()
            .filter(|(root, _)| root == &state.root && tree_root.is_none())
            .map(|(_, generation)| generation)
            .unwrap_or_else(|| self.library.open_view(&state.root, tree_root.as_deref()));
        let watched = tree_root.unwrap_or_else(|| state.root.clone());
        self.watch_epoch = self.watcher.open(&watched, &self.data_dir);
        self.watch_source_root = Some(watched);
        self.watch_root = Some(state.root.clone());
        self.watch_warning = None;
        self.refresh_pending = false;
        self.scan_busy = true;
        self.root_text = display_path(&state.root);
        self.scroll_y = 0.;
        self.scroll_override = Some(0.);
        self.status = media_browser::ui_text!(msg_06defd29f465).into();
        if !preserve_tree {
            self.sidebar = None;
            self.tree_collapsed.clear();
            self.tree_auto_expanded.clear();
            self.folder_resume_anchors.clear();
        }
    }
    fn history_move(&mut self, forward: bool) {
        if let Some(state) = self.current_state() {
            self.history.replace_current(state);
        }
        let state = if forward {
            self.history.forward()
        } else {
            self.history.back()
        };
        if let Some(state) = state {
            self.navigate_state(state, true);
        }
    }
    fn refresh_in_place(&mut self) {
        if self.scan_busy {
            self.refresh_pending = true;
            return;
        }
        if let Some(root) = &self.watch_root {
            // Retrying unchanged files must not be blocked by the old per-key
            // failure cache. Keep textures, selection, geometry and anchors.
            self.failures.clear();
            self.playback_errors.clear();
            self.loading.scan_error = None;
            self.generation = self
                .library
                .open_view(root, self.watch_source_root.as_deref());
            self.scan_busy = true;
            self.refresh_pending = false;
            self.status = media_browser::ui_text!(msg_d854355c8e4d).into();
        }
    }
    fn request_prepare(&mut self, width: f32) {
        if let Some(snapshot) = &self.snapshot {
            self.revision += 1;
            self.preparer.submit(PrepareRequest {
                revision: self.revision,
                snapshot: {
                    if self.features.only_favorites {
                        let mut filtered = (**snapshot).clone();
                        filtered
                            .files
                            .retain(|m| self.features.favorites.paths.contains(&m.file.path));
                        Arc::new(filtered)
                    } else {
                        snapshot.clone()
                    }
                },
                settings: self.settings.clone(),
                collapsed: self.collapsed.clone(),
                auto_fold_empty: self.auto_fold_empty,
                auto_expanded: self.auto_expanded.clone(),
                previous: self.prepared.clone(),
                reuse_view: !self
                    .benchmark
                    .as_ref()
                    .is_some_and(|b| b.legacy_preparation),
                width,
                target_width: self.target_width,
                gap: if self.workspace.appearance.compact {
                    4.
                } else {
                    8.
                },
            });
            self.prepare_dirty = false;
            self.last_width = width;
        }
    }
    fn apply_thumbnail_metadata(&mut self, job: &ThumbJob, meta: &crate::thumbnails::ThumbMeta) {
        if job.generation != self.generation {
            return;
        }
        let Some(record) = self.snapshot.as_ref().and_then(|snapshot| {
            snapshot.files.iter().find(|record| {
                record.id == job.id && record.file.fingerprint == job.file.fingerprint
            })
        }) else {
            return;
        };
        let duration = meta.duration_ms.or(record.duration_ms);
        if meta.animated {
            self.animated_images
                .insert(job.id, job.file.fingerprint.clone());
        } else {
            self.animated_images.remove(&job.id);
        }
        if record.width.zip(record.height).is_none() && meta.width > 0 && meta.height > 0 {
            // Scheduling acknowledgement only: drawing/layout still wait for
            // the authoritative index snapshot and its stable geometry.
            self.dimension_hints
                .remember(self.generation, job.id, job.file.fingerprint.clone());
        }
        self.library.update_metadata(
            self.generation,
            job.id,
            job.file.fingerprint.clone(),
            Some(meta.width),
            Some(meta.height),
            duration,
            None,
        );
    }
    fn load_readiness_counts(&self, ctx: &egui::Context) -> (usize, usize, usize) {
        let ready = self.prepared.as_ref().map_or(0, |p| {
            let bucket = thumbnail_bucket(p.layout.column_width * ctx.pixels_per_point());
            self.list_regions
                .keys()
                .filter(|id| {
                    p.lookup.get(id).is_some_and(|i| {
                        let record = &p.snapshot.files[*i];
                        record.width.zip(record.height).is_some_and(|size| {
                            [bucket, 64].iter().any(|bucket| {
                                self.textures
                                    .get(&self.job(record, *bucket, 1).key())
                                    .is_some_and(|texture| texture.source_size == size)
                            })
                        })
                    })
                })
                .count()
        });
        let clear_ready = self.prepared.as_ref().map_or(0, |p| {
            let bucket = thumbnail_bucket(p.layout.column_width * ctx.pixels_per_point());
            self.list_regions
                .keys()
                .filter(|id| {
                    p.lookup.get(id).is_some_and(|i| {
                        let record = &p.snapshot.files[*i];
                        record.width.zip(record.height).is_some_and(|size| {
                            self.textures
                                .get(&self.job(record, bucket, 1).key())
                                .is_some_and(|t| t.source_size == size)
                        })
                    })
                })
                .count()
        });
        let pending_geometry = self.prepared.as_ref().map_or(0, |p| {
            p.layout
                .sizing_rows
                .iter()
                .filter(|r| {
                    r.rect.bottom() >= self.scroll_y
                        && r.probe_top <= self.scroll_y + self.viewport_height
                })
                .map(|r| r.items.len())
                .sum::<usize>()
        });
        (ready, clear_ready, pending_geometry)
    }
    fn poll(&mut self, ctx: &egui::Context) {
        self.poll_rotation();
        if let Some(event) = self.watcher.poll() {
            if event.epoch == self.watch_epoch {
                self.watch_warning = event.warning;
                self.refresh_pending |= event.changed;
            }
        }
        if let Some(bytes) = self.font_receiver.as_ref().and_then(|r| r.try_recv().ok()) {
            ctx.set_fonts(fonts::definitions(Some(bytes)));
            self.full_font_loaded = true;
            self.font_receiver = None;
        }
        while let Some(event) = self.library.try_recv() {
            match event {
                LibraryEvent::Tree(tree) if tree.generation == self.generation => {
                    if self.watch_source_root.as_ref() != Some(&tree.root) {
                        self.watch_epoch = self.watcher.open(&tree.root, &self.data_dir);
                        self.watch_source_root = Some(tree.root.clone());
                    }
                    self.sidebar = Some(tree);
                }
                LibraryEvent::Snapshot(snapshot) if snapshot.generation == self.generation => {
                    if self.visit_pending {
                        self.places.visit(&snapshot.root);
                        self.visit_pending = false;
                    }
                    self.scan_busy = !snapshot.finished;
                    if snapshot.finished && snapshot.errors.is_empty() {
                        self.loading.scan_error = None;
                    }
                    if !self.path_editing {
                        self.root_text = display_path(&snapshot.root);
                    }
                    self.watch_root = Some(snapshot.root.clone());
                    self.status = if !snapshot.errors.is_empty() {
                        media_browser::ui_format!(msg_0c77305892a3, snapshot.errors.len())
                    } else if snapshot.finished {
                        media_browser::ui_format!(
                            msg_a2d7df3e4edd,
                            snapshot.files.len(),
                            snapshot.elapsed_ms
                        )
                    } else {
                        media_browser::ui_format!(msg_2095eb0c19dd, snapshot.files.len())
                    };
                    self.snapshot = Some(snapshot);
                    self.prepare_dirty = true;
                    self.prepare_after = Instant::now();
                }
                LibraryEvent::IndexRecovered { backup } => {
                    self.index_notice = Some(media_browser::ui_format!(
                        msg_412b88228038,
                        display_path(&backup)
                    ));
                }
                LibraryEvent::Error {
                    generation,
                    message,
                } if generation == self.generation => {
                    self.loading.scan_error = Some(message.clone());
                    self.status = message;
                    self.scan_busy = false;
                }
                _ => {}
            }
        }
        if let Some(prepared) = self.preparer.poll() {
            if prepared.revision == self.revision && prepared.snapshot.generation == self.generation
            {
                self.automatic_collapsed = prepared.automatic_collapsed.clone();
                if prepared.needs_full_font
                    && !self.full_font_loaded
                    && self.font_receiver.is_none()
                {
                    let (tx, rx) = std::sync::mpsc::channel();
                    self.font_receiver = Some(rx);
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        if let Ok(bytes) =
                            std::fs::read(crate::native::runtime_dir().join("Chinese-full.ttf"))
                        {
                            let _ = tx.send(bytes);
                            ctx.request_repaint();
                        }
                    });
                }
                let anchor = self.pending_anchor.take().or_else(|| {
                    self.prepared
                        .as_ref()
                        .and_then(|p| p.layout.anchor(self.scroll_y))
                });
                let restored = anchor
                    .as_ref()
                    .and_then(|anchor| prepared.layout.restore_anchor(anchor));
                if let Some(smoke) = &mut self.watch_smoke {
                    smoke.trace.push(serde_json::json!({"kind":"prepare","scroll":self.scroll_y,
                        "old_height":self.prepared.as_ref().map(|p|p.layout.height),"new_height":prepared.layout.height,
                        "old_cards":self.prepared.as_ref().map(|p|p.layout.rectangles.len()),"new_cards":prepared.layout.rectangles.len(),
                        "anchor":anchor.as_ref().map(|a|[a.media_id as f64,a.offset as f64]),"restored":restored,"generation":self.generation}));
                }
                self.scroll_override = restored.or_else(|| {
                    let old = self.prepared.as_ref()?;
                    let header = old
                        .layout
                        .visible_headers(self.scroll_y, self.scroll_y + self.viewport_height)
                        .first()?;
                    let new = prepared
                        .layout
                        .headers
                        .iter()
                        .find(|h| h.directory == header.directory && h.kind == header.kind)
                        .or_else(|| {
                            prepared.layout.headers.iter().rev().find(|h| {
                                h.kind.is_none() && header.directory.starts_with(&h.directory)
                            })
                        })?;
                    Some((new.rect.y - (header.rect.y - self.scroll_y)).max(0.))
                });
                self.selection.retain(|id| prepared.lookup.contains_key(id));
                let changed_detail = self.detail.as_ref().and_then(|viewer| {
                    let new = prepared
                        .lookup
                        .get(&viewer.id)
                        .map(|i| &prepared.snapshot.files[*i]);
                    let old = self
                        .prepared
                        .as_ref()
                        .and_then(|p| p.lookup.get(&viewer.id).map(|i| &p.snapshot.files[*i]));
                    match (old, new) {
                        (_, None) => Some((viewer.id, false)),
                        (Some(old), Some(new)) if old.file.fingerprint != new.file.fingerprint => {
                            Some((viewer.id, true))
                        }
                        _ => None,
                    }
                });
                let stale_player = self.player_media.is_some_and(|id| {
                    prepared.lookup.get(&id).is_none_or(|i| {
                        self.player_version.as_ref()
                            != Some(&prepared.snapshot.files[*i].file.fingerprint)
                    })
                });
                self.animated_images.retain(|id, fp| {
                    prepared
                        .lookup
                        .get(id)
                        .is_some_and(|i| prepared.snapshot.files[*i].file.fingerprint == *fp)
                });
                self.prepared = Some(prepared);
                if let Some(path) = self
                    .pending_folder_location
                    .take()
                    .or_else(|| self.folder_location_anchor.clone())
                {
                    self.locate_folder(&path);
                }
                if stale_player {
                    if let Some(p) = &self.player {
                        let _ = p.lock().unwrap().unload();
                    }
                    self.player_media = None;
                    self.player_version = None;
                    self.preview_active = None;
                    self.hover_candidate = None;
                    self.seeks.bind(None);
                    self.seek_flight = None;
                    self.resume_pending = None;
                    self.resume_starting = false;
                    self.restore_flight = false;
                    self.load_started = None;
                }
                if let Some((id, present)) = changed_detail {
                    if present {
                        self.open_detail(id);
                    } else {
                        self.close_detail();
                    }
                }
            }
        }
        if self.refresh_pending && !self.scan_busy {
            self.refresh_in_place();
        }
        for _ in 0..64 {
            let Some(event) = self.thumbnails.poll_metadata() else {
                break;
            };
            self.apply_thumbnail_metadata(&event.job, &event.meta);
        }
        // Metadata fallbacks carry no pixels. Give them a separate bounded
        // drain, as with the existing64-slot early-metadata channel, rather
        // than consuming one of the two texture uploads.
        for _ in 0..64 {
            let Some(event) = self.thumbnails.poll_metadata_result() else {
                break;
            };
            if event.job.generation != self.generation
                || !self.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.files.iter().any(|record| {
                        record.id == event.job.id
                            && record.file.fingerprint == event.job.file.fingerprint
                    })
                })
            {
                continue;
            }
            match event.result {
                Ok(pixels) => self.apply_thumbnail_metadata(&event.job, &pixels.meta),
                Err(message) if event.job.priority < 4 => {
                    self.failures
                        .insert(event.key, event.job.id, message.clone());
                    self.library.update_metadata(
                        self.generation,
                        event.job.id,
                        event.job.file.fingerprint,
                        None,
                        None,
                        None,
                        Some(message),
                    );
                }
                Err(_) => {}
            }
        }
        let mut thumbnail_results_processed = 0;
        let upload_began = Instant::now();
        let mut uploaded_this_frame = 0;
        let mut extra_upload_bytes = 0usize;
        for slot in 0..4 {
            let event = if slot < 2 {
                self.thumbnails.poll()
            } else {
                let headroom = (256usize * 1024 * 1024).saturating_sub(
                    self.texture_bytes
                        + self
                            .animation
                            .texture_bytes()
                            .saturating_add(self.list_animations.texture_bytes()),
                );
                let remaining = (2usize * 1024 * 1024)
                    .saturating_sub(extra_upload_bytes)
                    .min(headroom);
                if !self.adaptive_photo_uploads
                    || self.detail.is_some()
                    || (self.preview_active.is_some() && !self.adaptive_video_posters)
                    || upload_began.elapsed() >= Duration::from_millis(2)
                    || remaining == 0
                {
                    break;
                }
                self.thumbnails.poll_visible_list_pixels(
                    remaining,
                    self.adaptive_video_posters,
                    |id| self.list_regions.contains_key(&id),
                )
            };
            let Some(event) = event else {
                break;
            };
            thumbnail_results_processed += 1;
            if event.job.generation != self.generation {
                continue;
            }
            let valid = self.snapshot.as_ref().is_some_and(|s| {
                s.files.iter().any(|m| {
                    m.id == event.job.id && m.file.fingerprint == event.job.file.fingerprint
                })
            });
            if !valid {
                continue;
            }
            match event.result {
                Ok(pixels) => {
                    if event.job.bucket == 0 {
                        self.apply_thumbnail_metadata(&event.job, &pixels.meta);
                        continue;
                    }
                    let size = [
                        pixels.image.width() as usize,
                        pixels.image.height() as usize,
                    ];
                    let bytes = pixels.image.as_raw().len();
                    uploaded_this_frame += 1;
                    if slot >= 2 {
                        extra_upload_bytes += bytes;
                        self.upload_extra_total += 1;
                        if event.job.file.kind == MediaKind::Video {
                            self.extra_video_posters += 1;
                        }
                    }
                    let image =
                        egui::ColorImage::from_rgba_unmultiplied(size, pixels.image.as_raw());
                    let texture = ctx.load_texture(&event.key, image, egui::TextureOptions::LINEAR);
                    if let Some(old) = self.textures.insert(
                        event.key,
                        TextureEntry {
                            texture,
                            bytes,
                            last_frame: self.frame,
                            source_size: (pixels.meta.width, pixels.meta.height),
                        },
                    ) {
                        self.texture_bytes = self.texture_bytes.saturating_sub(old.bytes);
                    }
                    self.texture_bytes += bytes;
                    self.apply_thumbnail_metadata(&event.job, &pixels.meta);
                    self.cache_hits += u64::from(pixels.cache_hit);
                    self.decoded += 1;
                }
                Err(message) => {
                    if event.job.bucket == 0 && event.job.priority >= 4 {
                        continue;
                    }
                    self.failures
                        .insert(event.key, event.job.id, message.clone());
                    self.library.update_metadata(
                        self.generation,
                        event.job.id,
                        event.job.file.fingerprint,
                        None,
                        None,
                        None,
                        Some(message),
                    );
                }
            }
        }
        self.upload_max_per_frame = self.upload_max_per_frame.max(uploaded_this_frame);
        self.upload_extra_bytes_max = self.upload_extra_bytes_max.max(extra_upload_bytes);
        if uploaded_this_frame > 0 {
            self.upload_prepare_max_ms = self
                .upload_prepare_max_ms
                .max(upload_began.elapsed().as_secs_f64() * 1000.);
        }
        // Continue bounded draining if worker wakeups coalesced. The measured
        // CPU preparation time excludes deferred GPU work; byte/count limits
        // remain necessary and native frame intervals must still be checked.
        if thumbnail_results_processed >= 2 {
            ctx.request_repaint();
        }
        if self.detail.is_none() {
            self.texture_bytes = trim_inactive_list_textures_with_policy(
                &mut self.textures,
                self.texture_bytes,
                self.frame,
                self.adaptive_photo_uploads && self.compact_list_retention,
            );
        }
        while self.texture_bytes
            + self
                .animation
                .texture_bytes()
                .saturating_add(self.list_animations.texture_bytes())
            > 256 * 1024 * 1024
        {
            let Some(key) = self
                .textures
                .iter()
                .min_by_key(|(_, e)| e.last_frame)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(entry) = self.textures.remove(&key) {
                self.texture_bytes = self.texture_bytes.saturating_sub(entry.bytes);
            }
        }
    }
    fn job(&self, record: &MediaRecord, bucket: u32, priority: u8) -> ThumbJob {
        ThumbJob {
            generation: self.generation,
            id: record.id,
            file: record.file.clone(),
            bucket,
            decode_bucket: bucket,
            region: None,
            priority,
        }
    }
    fn texture(&mut self, key: &str) -> Option<egui::TextureHandle> {
        let entry = self.textures.get_mut(key)?;
        entry.last_frame = self.frame;
        let texture = entry.texture.clone();
        self.frame_textures.push(texture.clone());
        Some(texture)
    }
    fn layout_texture(&mut self, key: &str, record: &MediaRecord) -> Option<egui::TextureHandle> {
        let expected = record.width.zip(record.height)?;
        if self.textures.get(key)?.source_size != expected {
            return None;
        }
        self.texture(key)
    }
    fn is_animated(&self, record: &MediaRecord) -> bool {
        record
            .file
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gif"))
            || self.animated_images.get(&record.id) == Some(&record.file.fingerprint)
    }
    fn open_detail(&mut self, id: MediaId) {
        self.list_animations.clear();
        self.presentation.reset_clock();
        self.folder_location_anchor = None;
        if self.trash_releasing() || self.trash_blocks_media(id) || self.rotation.is_some() {
            return;
        }
        if !self.prepared.as_ref().is_some_and(|p| p.lookup.get(&id)
            .is_some_and(|i| p.snapshot.files[*i].file.kind == MediaKind::Video)) {
            self.exit_video_fullscreen();
        }
        if self.detail.is_none() {
            self.begin_media_motion(id, false);
        } else {
            self.media_transition = None;
        }
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        self.video_prefetch.cancel();
        if self
            .ctx
            .input(|i| i.viewport().minimized == Some(true) || i.viewport().occluded == Some(true))
        {
            self.deferred_detail = Some(id);
            self.detail = Some(Viewer {
                id,
                zoom: 1.,
                pan: Vec2::ZERO,
                native_pending: false,
                center_thumbnail: true,
                end_handled: false,
            });
            self.animation.clear();
            if let Some(p) = &self.player {
                let p = p.lock().unwrap();
                let _ = p.set("pause", "yes");
                let _ = p.set("mute", "yes");
            }
            return;
        }
        self.deferred_detail = None;
        self.detail_open_frame = self.frame;
        self.animation.clear();
        self.preview_active = None;
        self.hover_candidate = None;
        self.video_region = None;
        self.image_region = None;
        self.detail = Some(Viewer {
            id,
            zoom: 1.,
            pan: Vec2::ZERO,
            native_pending: false,
            center_thumbnail: true,
            end_handled: false,
        });
        let record = self
            .prepared
            .as_ref()
            .and_then(|p| p.lookup.get(&id).map(|i| p.snapshot.files[*i].clone()));
        if let Some(record) = record {
            if self.presentation.mode != presentation_ui::Mode::Group && self.is_animated(&record) {
                if self.list_animations.released() {
                    self.animation.open(&record.file);
                }
            }
            if record.file.kind == MediaKind::Video {
                let result = self.activate_video(&record, true);
                if let Err(error) = result {
                    self.status = error;
                }
            } else if let Some(p) = &self.player {
                let p = p.lock().unwrap();
                let _ = p.set("pause", "yes");
                let _ = p.set("mute", "yes");
                self.resume_starting = false;
            }
        }
    }
    fn close_detail(&mut self) {
        self.exit_video_fullscreen();
        self.presentation.mode = presentation_ui::Mode::Manual;
        self.presentation.playing = false;
        self.presentation.reset_clock();
        if let Some(viewer) = &self.detail {
            self.begin_media_motion(viewer.id, true);
        }
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        self.video_prefetch.cancel();
        self.deferred_detail = None;
        self.hidden_pause = None;
        if let Some(viewer) = self.detail.take() {
            self.resume_starting = false;
            self.animation.clear();
            if let Some(prepared) = &self.prepared {
                if let Some(record) = prepared
                    .lookup
                    .get(&viewer.id)
                    .map(|i| &prepared.snapshot.files[*i])
                {
                    if self.collapsed.remove(&record.file.parent) {
                        self.prepare_dirty = true;
                        self.prepare_after = Instant::now();
                        self.pending_anchor = Some(ScrollAnchor {
                            media_id: viewer.id,
                            offset: 12.,
                        });
                    }
                }
                if let Some(rect) = prepared.layout.rectangles.get(&viewer.id) {
                    if rect.bottom() < self.scroll_y
                        || rect.y > self.scroll_y + self.viewport_height
                    {
                        self.scroll_override = Some(rect.y.max(0.));
                    }
                }
            }
            if let Some(p) = &self.player {
                let p = p.lock().unwrap();
                let _ = p.set("mute", "yes");
                let _ = p.set("pause", "yes");
            }
            self.remember_position();
            self.preview_active = None;
            self.hover_candidate = None;
            self.preview_idle = Some(Instant::now());
        }
    }
    fn paint_video(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let displayed_id = self.video_region.map(|(id, _)| id).or(self.player_media);
        let error = displayed_id.and_then(|id| self.playback_errors.get(&id)
            .map(|(_, error)| (id, error.clone())));
        if let Some((id, error)) = error {
            ui.painter().rect_filled(rect, 0., Color32::BLACK);
            let width = (rect.width() - 32.).clamp(40., 560.);
            let panel = Rect::from_min_size(egui::pos2(rect.center().x - width * 0.5,
                (rect.center().y - 66.).max(rect.top())), egui::vec2(width, 260.));
            let mut error_ui = ui.new_child(egui::UiBuilder::new().id_salt(("video-error", id))
                .max_rect(panel).layout(egui::Layout::top_down(egui::Align::Center)));
            error_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            error_ui.spacing_mut().item_spacing.y = 10.;
            error_ui.label(egui::RichText::new(media_browser::ui_text!(msg_981ed5de50ac))
                .size(20.).color(modern_ui::DANGER));
            let hint = if self.player.is_none() { media_browser::ui_text!(video_player_start_hint) }
                else { media_browser::ui_text!(video_play_failed_hint) };
            error_ui.add(egui::Label::new(egui::RichText::new(hint).color(modern_ui::MUTED)).wrap());
            let retry = error_ui.add_sized(egui::vec2(112., 28.),
                egui::Button::new(media_browser::ui_text!(msg_b8784c8dd563)));
            error_ui.collapsing(media_browser::ui_text!(video_error_details), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(error).small().color(modern_ui::MUTED)).wrap());
            });
            self.control_regions.push(error_ui.min_rect().intersect(rect));
            self.control_regions.push(retry.rect);
            self.control_focused |= retry.has_focus();
            self.video_retry_button = Some((id, retry.rect));
            if retry.clicked() {
                let record = self.prepared.as_ref().and_then(|p|p.lookup.get(&id)
                    .map(|index|p.snapshot.files[*index].clone()));
                if let Some(record) = record {
                    let detail = self.detail.as_ref().is_some_and(|v|v.id == id);
                    if let Err(error) = self.activate_video(&record, detail) { self.status = error; }
                }
            }
            return;
        }
        if let Some(player) = self.player.clone() {
            let loading = !player.lock().unwrap().has_presented_frame()
                || self.resume_pending.is_some()
                || self.restore_flight;
            // Draw only this source's versioned poster below the GPU callback.
            // The callback replaces it as soon as an actual new-source frame
            // exists; file-loaded/empty redraws must not flash a black FBO.
            if loading {
                let record = self.prepared.as_ref().and_then(|p| {
                    self.player_media.and_then(|id| p.lookup.get(&id))
                        .map(|index| p.snapshot.files[*index].clone())
                });
                if let Some(record) = record {
                    let poster = [1024, 512, 256, 128, 64, 2048].into_iter().find_map(|bucket| {
                        let key = self.job(&record, bucket, 0).key();
                        self.layout_texture(&key, &record)
                    });
                    if let Some(poster) = poster {
                        ui.painter().image(poster.id(), contain(rect, poster.size_vec2()),
                            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)), Color32::WHITE);
                    }
                }
            }
            ui.painter().add(egui::PaintCallback {
                rect,
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                    let v = info.viewport_in_pixels();
                    let c = info.clip_rect_in_pixels();
                    if let Err(e) = player.lock().unwrap().paint_clipped(
                        painter.gl(),
                        v.width_px,
                        v.height_px,
                        [v.left_px, v.from_bottom_px, v.width_px, v.height_px],
                        painter.intermediate_fbo(),
                        [c.left_px, c.from_bottom_px, c.width_px, c.height_px],
                    ) {
                        eprintln!("Video render: {e}");
                    }
                })),
            });
            if loading {
                let badge = Rect::from_center_size(rect.center(), egui::vec2(128., 30.));
                ui.painter().rect_filled(badge, 5., Color32::from_black_alpha(160));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    media_browser::ui_text!(msg_13e5048c59b9),
                    egui::FontId::proportional(15.),
                    Color32::GRAY,
                );
            }
            if let Some((id, text, since)) = &self.seek_notice {
                if Some(*id) == self.player_media && since.elapsed() < Duration::from_millis(1200) {
                    let visible = rect.intersect(ui.clip_rect());
                    if visible.width() < 40. || visible.height() < 46. {
                        return;
                    }
                    let width = (visible.width() - 16.).clamp(16., 324.);
                    let toast = Rect::from_min_size(
                        visible.right_top() + egui::vec2(-width - 8., 8.),
                        egui::vec2(width, 30.),
                    );
                    let painter = ui.painter().with_clip_rect(visible);
                    painter.rect_filled(toast, 5., Color32::from_black_alpha(210));
                    painter.text(
                        toast.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        egui::FontId::proportional(13.),
                        Color32::WHITE,
                    );
                    ui.ctx().request_repaint_after(
                        Duration::from_millis(1200).saturating_sub(since.elapsed()),
                    );
                }
            }
        }
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        self.modern_toolbar(ui);
    }
    fn folder_collapsed(&self, path: &Path) -> bool {
        self.collapsed.contains(path)
            || (self.automatic_collapsed.contains(path) && !self.auto_expanded.contains(path))
    }
    fn tree_folder_collapsed(&self, path: &Path) -> bool {
        self.tree_collapsed.contains(path)
            || (self.automatic_collapsed.contains(path) && !self.tree_auto_expanded.contains(path))
    }
    fn sidebar_contents(&mut self, ui: &mut egui::Ui) {
        self.places_sidebar(ui);
        ui.add_space(4.);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(media_browser::ui_text!(msg_1bd55f256f49))
                    .size(10.)
                    .color(modern_ui::MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pin = icons::icon_button(
                    ui,
                    if self.pinned {
                        icons::Glyph::PinOff
                    } else {
                        icons::Glyph::Pin
                    },
                    if self.pinned {
                        media_browser::ui_text!(msg_759e7613d9ba)
                    } else {
                        media_browser::ui_text!(msg_630366fde3cd)
                    },
                );
                self.sidebar_pin_rect = Some(pin.rect);
                if pin.clicked() {
                    self.pinned = !self.pinned;
                    self.sidebar_open = true;
                    self.outside_since = None;
                    self.ctx.request_repaint();
                }
            });
        });
        ui.add_space(4.);
        let Some(snapshot) = self.sidebar.clone() else {
            ui.weak(media_browser::ui_text!(msg_e2882bdbe7bc));
            return;
        };
        let parents: HashSet<_> = snapshot
            .directories
            .iter()
            .filter_map(|d| d.parent.clone())
            .collect();
        let mut direct: HashMap<PathBuf, u64> = HashMap::new();
        for d in &snapshot.directories {
            if let Some(parent) = &d.parent {
                *direct.entry(parent.clone()).or_default() += 1;
            }
        }
        let stale = snapshot.generation != self.generation;
        egui::ScrollArea::vertical()
            .id_salt("glimpse-directory-tree")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.;
                for dir in &snapshot.directories {
                    if dir
                        .path
                        .ancestors()
                        .skip(1)
                        .any(|p| self.tree_folder_collapsed(p))
                    {
                        continue;
                    }
                    let depth = dir
                        .path
                        .strip_prefix(&snapshot.root)
                        .map(|p| p.components().count())
                        .unwrap_or(0);
                    let indent = media_browser::layout::tree_indent(depth, ui.available_width());
                    let (row, hover) = ui.allocate_exact_size(
                        egui::vec2(
                            ui.available_width(),
                            if self.workspace.appearance.compact {
                                35.
                            } else {
                                40.
                            },
                        ),
                        egui::Sense::hover(),
                    );
                    let chosen = self
                        .folder_location_anchor
                        .as_ref()
                        .or(self.watch_root.as_ref())
                        == Some(&dir.path);
                    if chosen || hover.hovered() {
                        ui.painter().rect_filled(
                            row,
                            6.,
                            if chosen {
                                Color32::from_rgb(44, 40, 54)
                            } else {
                                modern_ui::SURFACE
                            },
                        );
                    }
                    for level in 0..depth {
                        let x = row.left() + 6. + level as f32 * 8.;
                        ui.painter().line_segment(
                            [egui::pos2(x, row.top()), egui::pos2(x, row.bottom())],
                            egui::Stroke::new(1., Color32::from_gray(48)),
                        );
                    }
                    let toggle = Rect::from_min_size(
                        row.min + egui::vec2(indent, 0.),
                        egui::vec2(20., row.height()),
                    );
                    if parents.contains(&dir.path) {
                        icons::paint(
                            ui.painter(),
                            Rect::from_center_size(
                                toggle.center() - egui::vec2(0., 8.),
                                egui::vec2(16., 16.),
                            ),
                            if self.tree_folder_collapsed(&dir.path) {
                                icons::Glyph::Right
                            } else {
                                icons::Glyph::Down
                            },
                            modern_ui::MUTED,
                        );
                        if ui
                            .interact(
                                toggle,
                                ui.scope_id().with(("tree-toggle", &dir.path)),
                                egui::Sense::click(),
                            )
                            .clicked()
                        {
                            if self.tree_folder_collapsed(&dir.path) {
                                self.tree_collapsed.remove(&dir.path);
                                self.tree_auto_expanded.insert(dir.path.clone());
                            } else {
                                self.tree_collapsed.insert(dir.path.clone());
                                self.tree_auto_expanded.remove(&dir.path);
                            }
                        }
                    }
                    let hit = Rect::from_min_max(toggle.right_top(), row.right_bottom());
                    let response = ui.interact(
                        hit,
                        ui.scope_id().with(("tree-row", &dir.path)),
                        egui::Sense::click(),
                    );
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            row,
                            6.,
                            ui.visuals().selection.stroke,
                            egui::StrokeKind::Inside,
                        );
                    }
                    let name = dir
                        .path
                        .file_name()
                        .unwrap_or(dir.path.as_os_str())
                        .to_string_lossy();
                    let galley =
                        egui::WidgetText::from(egui::RichText::new(name.as_ref()).size(12.).color(
                            if chosen {
                                Color32::from_gray(239)
                            } else {
                                Color32::from_gray(208)
                            },
                        ))
                        .into_galley(
                            ui,
                            Some(egui::TextWrapMode::Truncate),
                            hit.width() - 8.,
                            egui::TextStyle::Body,
                        );
                    ui.painter()
                        .galley(hit.min + egui::vec2(2., 4.), galley, Color32::WHITE);
                    let counts = media_browser::ui_format!(
                        msg_1b29a059b428,
                        dir.recursive.images,
                        dir.recursive.videos,
                        direct.get(&dir.path).copied().unwrap_or(0),
                        if stale || !dir.subtree_complete {
                            " …"
                        } else {
                            ""
                        }
                    );
                    let counts = egui::WidgetText::from(
                        egui::RichText::new(counts)
                            .size(10.)
                            .color(modern_ui::MUTED),
                    )
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Truncate),
                        hit.width() - 8.,
                        egui::TextStyle::Small,
                    );
                    ui.painter()
                        .galley(hit.min + egui::vec2(2., 25.), counts, modern_ui::MUTED);
                    if response.clicked() {
                        self.locate_folder(&dir.path);
                    }
                    if response.secondary_clicked() && !self.selected_folders.contains(&dir.path) {
                        self.selected_folders.clear();
                        self.selected_folders.insert(dir.path.clone());
                    }
                    response.clone().on_hover_text(media_browser::ui_format!(
                        msg_267ef9791aaf,
                        display_path(&dir.path),
                        dir.direct.images,
                        dir.direct.videos,
                        direct.get(&dir.path).copied().unwrap_or(0),
                        dir.recursive.images,
                        dir.recursive.videos
                    ));
                    response.context_menu(|ui| self.folder_menu(ui, &dir.path));
                }
            });
    }
    fn list(&mut self, ui: &mut egui::Ui, jobs: &mut Vec<ThumbJob>) {
        self.viewport_height = ui.available_height();
        let width = (ui.available_width() - 18.).max(40.);
        if self.snapshot.is_none() {
            self.last_width = width;
            self.prepare_dirty = false;
        } else if (width - self.last_width).abs() > 1. {
            self.prepare_dirty = true;
        }
        if self.prepare_dirty && Instant::now() >= self.prepare_after {
            self.request_prepare(width);
        }
        let Some(prepared) = self.prepared.clone() else {
            ui.vertical_centered(|ui| {
                ui.add_space(100.);
                if self.snapshot.is_none() {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(64., 64.), egui::Sense::hover());
                    media_browser::branding::paint(ui.painter(), r);
                    ui.add_space(12.);
                    ui.heading(media_browser::branding::NAME);
                    ui.add_space(6.);
                }
                ui.heading(if self.snapshot.is_some() {
                    media_browser::ui_text!(msg_5f9bb617ff06)
                } else {
                    media_browser::ui_text!(msg_1d3a0176f221)
                });
                ui.label(media_browser::ui_text!(msg_b7622205cdd2));
            });
            return;
        };
        let offset = self.scroll_override.take();
        self.loading.begin_frame();
        let mut retry_loading = false;
        self.list_regions.clear();
        self.folder_title_regions.clear();
        self.folder_toggle_regions.clear();
        self.sticky_toggle_regions.clear();
        self.sticky_jump_button = None;
        self.sticky_jump_target = None;
        let output = show_media_scroll(ui, self.watch_epoch, offset, |ui, viewport| {
            ui.set_min_height(prepared.layout.height);
            ui.set_min_width(width);
            let origin = ui.min_rect().min;
            for guide in &prepared.layout.tree_guides {
                if guide.bottom() >= viewport.min.y && guide.y <= viewport.max.y {
                    let line = to_rect(*guide, origin);
                    ui.painter().line_segment(
                        [line.left_top(), line.left_bottom()],
                        egui::Stroke::new(1., Color32::from_gray(56)),
                    );
                }
            }
            for header in prepared
                .layout
                .visible_headers(viewport.min.y, viewport.max.y)
            {
                self.folder_header(ui, header, origin, &prepared.snapshot.root);
            }
            if let Some(state) = loading_ui::empty_list_state(
                prepared.snapshot.finished && !self.scan_busy,
                !prepared.snapshot.errors.is_empty() || self.loading.scan_error.is_some(),
                self.snapshot.as_ref().is_some_and(|snapshot| snapshot.files.is_empty()),
                prepared.view.sequence.is_empty(),
            ) {
                let free = Rect::from_min_max(
                    egui::pos2(origin.x, origin.y + prepared.layout.height.max(viewport.min.y)),
                    egui::pos2(origin.x + width, origin.y + viewport.max.y),
                );
                loading_ui::paint_empty_list(ui, free, state);
            }
            // Visible geometry shares priority1 with visible pixels and is
            // inserted first. Detail pixels keep priority0. Nearby geometry
            // shares priority3 with pixel prefetch, inserted first; all
            // per-frame/job caps stay fixed.
            for (id, priority) in viewport_geometry_requests(
                &prepared.layout,
                viewport.min.y,
                viewport.max.y,
                (prepared.layout.columns.len() * 2).clamp(4, 16),
                self.benchmark
                    .as_ref()
                    .is_some_and(|b| b.legacy_sizing_priority),
                |id| {
                    prepared.lookup.get(&id).is_some_and(|index| {
                        self.dimension_hints.contains(
                            self.generation,
                            id,
                            &prepared.snapshot.files[*index].file.fingerprint,
                        )
                    })
                },
            ) {
                if let Some(index) = prepared.lookup.get(&id) {
                    let pixels_first = self
                        .benchmark
                        .as_ref()
                        .is_some_and(|b| b.legacy_geometry_priority);
                    let prefetch_first = self
                        .benchmark
                        .as_ref()
                        .is_some_and(|b| b.legacy_near_geometry_priority);
                    let effective = match priority {
                        2 if !pixels_first => 1,
                        4 if !prefetch_first => 3,
                        _ => priority,
                    };
                    jobs.push(self.job(&prepared.snapshot.files[*index], 0, effective));
                }
            }
            for pending in &prepared.layout.sizing_rows {
                if pending.rect.bottom() < viewport.min.y - viewport.height()
                    || pending.rect.y > viewport.max.y + viewport.height()
                {
                    continue;
                }
                if pending.rect.bottom() >= viewport.min.y && pending.rect.y <= viewport.max.y {
                    let rect = to_rect(pending.rect, origin);
                    let slow = pending
                        .items
                        .first()
                        .is_some_and(|id| self.loading.slow(self.generation, *id));
                    let text_clip = Rect::from_min_max(
                        rect.min,
                        egui::pos2(
                            if slow {
                                (rect.right() - 124.).max(rect.left())
                            } else {
                                rect.right()
                            },
                            rect.bottom(),
                        ),
                    );
                    ui.painter()
                        .with_clip_rect(text_clip.intersect(ui.clip_rect()))
                        .text(
                            rect.left_center() + egui::vec2(6., 0.),
                            egui::Align2::LEFT_CENTER,
                            if slow {
                                media_browser::ui_text!(loading_geometry_slow)
                            } else {
                                media_browser::ui_text!(msg_a4ebb6277770)
                            },
                            egui::FontId::proportional(13.),
                            Color32::GRAY,
                        );
                    if slow {
                        let button = Rect::from_min_size(
                            egui::pos2((rect.right() - 120.).max(rect.left()), rect.top()),
                            egui::vec2(116_f32.min(rect.width()), 28.),
                        );
                        if ui
                            .put(
                                button,
                                egui::Button::new(media_browser::ui_text!(loading_retry)),
                            )
                            .clicked()
                        {
                            retry_loading = true;
                        }
                    }
                }
            }
            let bucket =
                thumbnail_bucket(prepared.layout.column_width * ui.ctx().pixels_per_point());
            let prefetch = if media_browser::media_path::virtual_hint(&prepared.snapshot.root) {
                0.
            } else {
                viewport.height()
            };
            for card in prepared
                .layout
                .visible_cards(viewport.min.y - prefetch, viewport.max.y + prefetch)
            {
                let Some(index) = prepared.lookup.get(&card.id) else {
                    continue;
                };
                let record = &prepared.snapshot.files[*index];
                let priority =
                    if card.rect.bottom() >= viewport.min.y && card.rect.y <= viewport.max.y {
                        1
                    } else {
                        3
                    };
                let key = self.progressive_job(record, bucket, priority, jobs);
                if card.rect.bottom() < viewport.min.y || card.rect.y > viewport.max.y {
                    continue;
                }
                let rect = to_rect(card.rect, origin);
                self.list_regions.insert(card.id, rect);
                let response = ui.interact(
                    rect,
                    ui.scope_id().with(("media", card.id)),
                    egui::Sense::click_and_drag(),
                );
                ui.painter()
                    .rect_filled(rect, modern_ui::CARD_RADIUS, modern_ui::SURFACE);
                if !self.textures.contains_key(&key)
                    && self.textures.contains_key(&self.job(record, 64, 0).key())
                {
                    if self.progressive_preview_paints == 0 && self.folder_smoke.is_some() {
                        self.snapshot_image(ui.ctx(), "folder-progressive-soft.png");
                    }
                    self.progressive_preview_paints += 1;
                }
                let gif = record
                    .file
                    .path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
                if gif
                    && self.detail.is_none()
                    && !self.trash_releasing()
                    && !self.trash_blocks_media(record.id)
                    && self.rotation.is_none()
                {
                    if let Some((w, h)) = record.width.zip(record.height) {
                        let fit = contain(rect, egui::vec2(w as f32, h as f32));
                        let scale = (2. * fit.width() * ui.ctx().pixels_per_point() / w as f32)
                            .min(2. * fit.height() * ui.ctx().pixels_per_point() / h as f32)
                            .min(1.);
                        self.list_animation_requests
                            .push(media_browser::gif_previews::Request {
                                id: record.id,
                                generation: self.generation,
                                file: record.file.clone(),
                                width: (w as f32 * scale).round().max(1.) as u32,
                                height: (h as f32 * scale).round().max(1.) as u32,
                            });
                    }
                }
                let animated = if self.detail.is_none() {
                    self.list_animations.texture(record.id)
                } else {
                    None
                };
                if let Some(texture) = animated.or_else(|| {
                    record.width.zip(record.height).and_then(|_| {
                        self.layout_texture(&key, record)
                            .or_else(|| self.layout_texture(&self.job(record, 64, 0).key(), record))
                    })
                }) {
                    self.frame_textures.push(texture.clone());
                    let fit = contain(rect, texture.size_vec2());
                    if let Some(smoke) = &mut self.format_smoke {
                        smoke.list_region = Some(fit);
                    }
                    ui.painter().image(
                        texture.id(),
                        fit,
                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                        Color32::WHITE,
                    );
                } else if let Some(error) = self.failures.get(&key).or(record.error.as_ref()) {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        media_browser::ui_text!(msg_1fe9e120a450),
                        egui::FontId::proportional(15.),
                        modern_ui::DANGER,
                    );
                    response.clone().on_hover_text(error);
                } else {
                    modern_ui::loading(
                        ui.painter(),
                        rect,
                        media_browser::ui_text!(msg_4927a53bcc88),
                    );
                }
                if record.file.kind == MediaKind::Video {
                    let content = contain(rect, self.video_size(record));
                    let check =
                        Rect::from_min_size(rect.min + egui::vec2(4., 4.), egui::vec2(28., 28.));
                    self.visible_videos
                        .insert(record.id, (content.intersect(ui.clip_rect()), check));
                    if ui.input(|i| {
                        i.pointer.hover_pos().is_some_and(|p| {
                            content.intersect(ui.clip_rect()).contains(p) && !check.contains(p)
                        })
                    }) {
                        self.hovered_video = Some(record.id);
                    }
                    if self.preview_active == Some(record.id) && self.detail.is_none() {
                        self.paint_video(ui, content);
                        self.preview_controls(ui, record.id, content);
                        self.video_region = Some((record.id, content.intersect(ui.clip_rect())));
                        self.video_exclusion = Some(check);
                    }
                    if self
                        .playback_errors
                        .get(&record.id)
                        .is_some_and(|(version, _)| version == &record.file.fingerprint)
                    {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            media_browser::ui_text!(msg_981ed5de50ac),
                            egui::FontId::proportional(16.),
                            modern_ui::DANGER,
                        );
                    }
                    if self.preview_active != Some(record.id) {
                        let label = record
                            .duration_ms
                            .map(format_time)
                            .unwrap_or_else(|| media_browser::ui_text!(msg_c20f7618d330).into());
                        modern_ui::media_badge(ui, rect, &label);
                    }
                }
                let check =
                    Rect::from_min_size(rect.min + egui::vec2(4., 4.), egui::vec2(28., 28.));
                self.favorite_card(ui, record, rect);
                let selected = self.selection.contains(&card.id);
                let checkbox = ui.interact(
                    check,
                    ui.scope_id().with(("check", card.id)),
                    egui::Sense::click_and_drag(),
                );
                self.drag_select_card(ui, card.id, rect, &checkbox, &response);
                if selected || response.hovered() || checkbox.hovered() || checkbox.has_focus() {
                    modern_ui::selection_mark(
                        ui.painter(),
                        check,
                        selected,
                        self.workspace.appearance.accent(),
                    );
                }
                let hover = ui.ctx().animate_bool_with_time(
                    response.id.with("card-hover"),
                    response.hovered() || response.has_focus(),
                    ui.style().animation_time,
                );
                if selected || hover > 0. {
                    ui.painter().rect_stroke(
                        rect,
                        8.,
                        egui::Stroke::new(
                            if selected { 3. } else { modern_ui::STROKE },
                            if selected {
                                self.workspace.appearance.accent()
                            } else {
                                modern_ui::BORDER.gamma_multiply(hover)
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                }
                if checkbox.clicked() {
                    self.toggle_selection(card.id, ui.input(|i| i.modifiers.shift));
                }
                let over_control = ui.input(|i| {
                    i.pointer.interact_pos().is_some_and(|p| {
                        check.contains(p)
                            || features::favorite_rect(rect).contains(p)
                            || self.features.dragging
                            || self.control_regions.iter().any(|r| r.contains(p))
                    })
                });
                if response.clicked() && !over_control {
                    let modifiers = ui.input(|i| i.modifiers);
                    if modifiers.command || modifiers.ctrl || modifiers.shift {
                        self.toggle_selection(card.id, modifiers.shift);
                    } else {
                        self.open_detail(card.id);
                    }
                }
                if response.secondary_clicked() && !self.selection.contains(&card.id) {
                    self.selection.clear();
                    self.selection.insert(card.id);
                    self.selection_anchor = Some(card.id);
                }
                response.context_menu(|ui| self.media_menu(ui, record));
                response.on_hover_text(record.file.path.display().to_string());
            }
        });
        self.scroll_y = output.state.clamped_offset().y;
        self.list_viewport = Some(output.inner_rect);
        self.list_scrollbar = Some(Rect::from_min_size(
            egui::pos2(output.inner_rect.right() + 4., output.inner_rect.top()),
            egui::vec2(14., output.inner_rect.height()),
        ));
        if self.features.dragging {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if output.inner_rect.x_range().contains(pos.x) {
                    let delta = if pos.y > output.inner_rect.bottom() - 32. {
                        24.
                    } else if pos.y < output.inner_rect.top() + 32. {
                        -24.
                    } else {
                        0.
                    };
                    if delta != 0. {
                        self.scroll_override = Some((self.scroll_y + delta).max(0.));
                        ui.ctx().request_repaint();
                    }
                }
            }
        }
        if self.detail.is_none() {
            self.folder_sticky(ui, &prepared, output.inner_rect);
        }
        self.loading.end_frame();
        if retry_loading {
            self.refresh_in_place();
        }
    }
    fn snapshot_image(&self, ctx: &egui::Context, name: &str) {
        let dir = PathBuf::from("artifacts");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(name);
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
            egui::ScreenshotCallback::new(move |pixels| {
                let rgba: Vec<u8> = pixels.pixels.iter().flat_map(|p| p.to_array()).collect();
                let _ = image::save_buffer(
                    path,
                    &rgba,
                    pixels.size[0] as u32,
                    pixels.size[1] as u32,
                    image::ColorType::Rgba8,
                );
            }),
        ))
    }
    fn smoke(&mut self, ctx: &egui::Context) {
        if !self.smoke {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        if self.smoke_at.elapsed() < Duration::from_secs(3) {
            return;
        }
        match self.smoke_step {
            0 => self.snapshot_image(ctx, "p2-list.png"),
            1 => {
                if let Some(p) = &self.prepared {
                    if let Some(record) = p
                        .snapshot
                        .files
                        .iter()
                        .find(|m| m.file.kind == MediaKind::Image)
                    {
                        self.open_detail(record.id);
                    }
                }
            }
            2 => self.snapshot_image(ctx, "p2-image-detail.png"),
            3 => {
                self.close_detail();
                self.settings.type_groups = true;
                self.settings.filter = MediaFilter::Videos;
                self.prepare_dirty = true;
                self.prepare_after = Instant::now();
            }
            4 => self.snapshot_image(ctx, "p2-videos.png"),
            5 => {
                if let Some(root) = self.snapshot.as_ref().map(|s| s.root.clone()) {
                    self.settings.filter = MediaFilter::All;
                    self.navigate(root.join("图集 2"), true);
                    self.pinned = true;
                }
            }
            6 => self.snapshot_image(ctx, "p2-error-and-sidebar.png"),
            7 => {
                let data = serde_json::json!({"scope":"programmatic browser actions, not physical mouse proof","steps":self.smoke_rows,"decoded":self.decoded,"cache_hits":self.cache_hits,"texture_bytes":self.texture_bytes,"texture_count":self.textures.len(),"full_font_loaded":self.full_font_loaded,"preview_failures":self.failures.len(),"preview_failure_variants":self.failures.variant_len()});
                let _ = std::fs::write(
                    "artifacts/p2-browser-smoke.json",
                    serde_json::to_vec_pretty(&data).unwrap(),
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
        self.smoke_rows.push(serde_json::json!({"step":self.smoke_step,"media":self.prepared.as_ref().map(|p|p.snapshot.files.len()),"groups":self.prepared.as_ref().map(|p|p.view.groups.len()),"shown_images":self.prepared.as_ref().map(|p|p.view.shown.images),"shown_videos":self.prepared.as_ref().map(|p|p.view.shown.videos),"detail":self.detail.as_ref().map(|d|d.id),"status":self.status}));
        self.smoke_step += 1;
        self.smoke_at = Instant::now();
    }
}

impl eframe::App for Browser {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.first_frame {
            crate::startup_trace::mark("first_logic");
        }
        #[cfg(windows)]
        self.poll_instance(ctx);
        if self.window_recovery_enabled {
            // egui viewport points include UI zoom; preserve native logical size
            // so increasing text/UI zoom never enlarges an otherwise valid window.
            let zoom = ctx.zoom_factor();
            let scale_changed = self
                .window_recovery
                .update_scale(ctx.native_pixels_per_point().unwrap_or(1.), zoom);
            let (size, monitor, minimized, maximized, fullscreen) = ctx.input(|i| {
                let v = i.viewport();
                (
                    v.inner_rect.map(|r| [r.width() * zoom, r.height() * zoom]),
                    v.monitor_size.map(|s| [s.x * zoom, s.y * zoom]),
                    v.minimized == Some(true),
                    v.maximized == Some(true),
                    v.fullscreen == Some(true),
                )
            });
            if scale_changed {
                ctx.request_repaint();
            } else {
                let decision = self.window_recovery.observe(
                    size,
                    monitor,
                    minimized,
                    maximized,
                    fullscreen,
                    self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                );
                if let Some(minimum) = decision.minimum {
                    ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                        minimum[0] / zoom,
                        minimum[1] / zoom,
                    )));
                }
                if let Some(size) = decision.restore {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        size[0] / zoom,
                        size[1] / zoom,
                    )));
                    ctx.request_repaint();
                }
                if decision.retry {
                    ctx.request_repaint_after(Duration::from_millis(250));
                }
            }
        }
        self.folder_tick(ctx);
        self.trash_tick(ctx);
        let hidden = ctx
            .input(|i| i.viewport().minimized == Some(true) || i.viewport().occluded == Some(true));
        if hidden {
            if let Some(smoke) = &mut self.detail_smoke {
                smoke.hidden_tick(ctx);
            }
            if self.detail.is_none() {
                self.hovered_video = None;
                self.hover_tick(ctx);
            } else if let (Some(id), Some(player)) = (self.player_media, &self.player) {
                let p = player.lock().unwrap();
                if self.hidden_pause.is_none() {
                    self.hidden_pause = Some((id, p.paused_intent()));
                    let _ = p.set("pause", "yes");
                }
            }
        } else {
            if let Some(id) = self.deferred_detail.take() {
                self.open_detail(id);
            }
            if let Some((id, paused)) = self.hidden_pause.take() {
                if self.player_media == Some(id) && self.detail.as_ref().is_some_and(|v| v.id == id)
                {
                    if let Some(p) = &self.player {
                        let _ = p
                            .lock()
                            .unwrap()
                            .set("pause", if paused { "yes" } else { "no" });
                    }
                }
            }
            if self.detail.is_none() && !ctx.input(|i| i.focused) {
                self.hovered_video = None;
                self.hover_tick(ctx);
            }
        }
        #[cfg(windows)]
        self.record_instance_visibility(ctx);
        if let Some(after) = self.quit_after {
            if self.started.elapsed() >= after {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(after.saturating_sub(self.started.elapsed()));
            }
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(smoke) = &mut self.loading_smoke {
            smoke.inject(raw);
        }
        if let Some(benchmark) = &mut self.seek_benchmark {
            benchmark.inject(raw);
        }
        if self.first_frame {
            crate::startup_trace::mark("first_raw_input");
        }
        if let Some(mut smoke) = self.input_smoke.take() {
            smoke.inject(self, raw);
            self.input_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.detail_smoke.take() {
            smoke.inject(raw);
            self.detail_smoke = Some(smoke);
        }
        if let Some(smoke) = &self.format_smoke {
            smoke.isolate_pointer(raw);
        }
        if let Some(smoke) = &self.animation_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.media_jump_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.empty_fold_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.trash_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.edit_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.folder_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.navigation_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.features_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.daily_smoke {smoke.inject(raw);}
        if let Some(smoke) = &mut self.video_switch_smoke {smoke.inject(raw);}
        if let Some(smoke) = &mut self.design_smoke {
            smoke.inject(raw);
        }
        if let Some(smoke) = &mut self.presentation_smoke {
            smoke.inject(raw);
        }
        if self.benchmark.is_some() {
            crate::benchmark::isolate_scroll_benchmark_input(raw);
            if self.benchmark.as_ref().is_some_and(|b| b.hover_video) {
                // Explicit mixed-load protocol: aim at the visible video
                // nearest the viewport centre. No OS pointer/focus mutation.
                let area = ctx.content_rect().shrink2(egui::vec2(0., 80.));
                let point = self
                    .list_regions
                    .iter()
                    .filter_map(|(id, rect)| {
                        let prepared = self.prepared.as_ref()?;
                        let record = &prepared.snapshot.files[*prepared.lookup.get(id)?];
                        if record.file.kind != MediaKind::Video {
                            return None;
                        }
                        let clipped = rect.intersect(area);
                        clipped.is_positive().then_some((*id, clipped.center()))
                    })
                    .min_by(|(aid, a), (bid, b)| {
                        a.distance_sq(area.center())
                            .total_cmp(&b.distance_sq(area.center()))
                            .then_with(|| aid.cmp(bid))
                    })
                    .map(|(_, point)| point);
                raw.focused = true;
                raw.events.push(egui::Event::WindowFocused(true));
                if let Some(point) = point {
                    raw.events.push(egui::Event::PointerMoved(point));
                }
            }
        }
        self.raw_controls(ctx, raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let trace_first = self.first_frame && crate::startup_trace::enabled();
        if trace_first {
            crate::startup_trace::mark("first_ui_entry");
        }
        let ctx = ui.ctx().clone();
        self.frame += 1;
        self.features_tick(&ctx);
        self.motion_rect = None;
        self.frame_textures.clear();
        self.visible_videos.clear();
        self.preview_controls_rect = None;
        self.preview_seek_rect = None;
        self.list_animation_requests.clear();
        let list_visible = self.detail.is_none()
            && !self.trash_releasing()
            && self.rotation.is_none()
            && !self.workspace.is_open()
            && ctx.input(|i| {
                i.viewport().minimized != Some(true) && i.viewport().occluded != Some(true)
            });
        if list_visible && self.animation.released() {
            self.list_animations.tick(&ctx);
        } else {
            self.list_animations.clear();
        }
        self.animation.tick(&ctx);
        self.control_focused = false;
        self.control_regions.clear();
        self.video_retry_button = None;
        self.hovered_video = None;
        self.video_region = None;
        self.video_exclusion = None;
        self.image_region = None;
        if self.first_frame {
            modern_ui::apply_preferences(&ctx, &self.workspace.appearance);
            crate::startup_trace::mark("first_ui_preferences_ready");
            self.first_frame = false;
            if self.fullscreen {
                // Apply after window creation as well: initial inner-size restoration can
                // otherwise leave a fullscreen request at the normal window dimensions.
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            }
            if self.first_ui_report.is_some() {
                self.first_ui_ms = Some(self.started.elapsed().as_millis());
            }
            ctx.request_repaint();
        } else if let Some(root) = self.initial_root.take() {
            self.navigate(root, false);
        }
        // Do not consume early snapshots with generation zero before initial navigation.
        if self.initial_open.is_none() || self.initial_root.is_none() {
            self.poll(&ctx);
        }

        if let Some(benchmark) = &mut self.benchmark {
            let (ready, media, height) = self
                .prepared
                .as_ref()
                .map(|p| {
                    (
                        p.snapshot.finished && !self.prepare_dirty,
                        p.snapshot.files.len(),
                        p.layout.height,
                    )
                })
                .unwrap_or((false, 0, 0.));
            if let Some(offset) = benchmark.tick(
                &ctx,
                ready,
                media,
                height,
                self.viewport_height,
                self.scroll_y,
                self.texture_bytes,
                self.textures.len(),
                self.decoded,
                self.cache_hits,
                self.failures.len(),
                self.started.elapsed().as_secs_f64() * 1000.,
            ) {
                self.scroll_override = Some(offset);
            }
        }
        let mut jobs = Vec::new();
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_owned()));
        if let Some(path) = dropped {
            self.navigate(path, false);
        }
        self.sidebar_button_rect = None;
        self.sidebar_pin_rect = None;
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.bottom_status_bar(ui));
        let sidebar_was_pinned = self.pinned;
        if sidebar_was_pinned {
            egui::Panel::left("directory")
                .exact_size(220.)
                .show(ui, |ui| self.sidebar_contents(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| self.list(ui, &mut jobs));
        // Unpinning during panel paint must not paint the same controls twice
        // and deliver one click to both copies. Float from the next frame.
        if !self.pinned && !sidebar_was_pinned && self.detail.is_none() && !self.workspace.is_open()
        {
            let pointer = ctx.input(|i| i.pointer.hover_pos());
            let button = self.sidebar_button_rect;
            let screen = ctx.content_rect();
            let anchor = egui::pos2(
                button
                    .map_or(screen.left(), |r| r.left())
                    .max(screen.left()),
                button.map_or(screen.top() + 76., |r| r.bottom()) + 4.,
            );
            let over_button = pointer.is_some_and(|p| button.is_some_and(|r| r.contains(p)));
            let over_edge = pointer.is_some_and(|p| {
                p.x < screen.left() + 8. && p.y >= anchor.y && p.y < screen.bottom() - 28.
            });
            let over_sidebar = pointer.is_some_and(|p| {
                button.is_some_and(|r| r.expand(4.).contains(p))
                    || self
                        .sidebar_overlay_rect
                        .is_some_and(|r| r.expand(4.).contains(p))
            });
            let popup_open = egui::Popup::is_any_open(&ctx);
            if (over_button || over_edge) && !popup_open {
                let began = self.edge_since.get_or_insert_with(Instant::now);
                if began.elapsed() > Duration::from_millis(150) {
                    self.sidebar_open = true;
                } else {
                    ctx.request_repaint_after(Duration::from_millis(150));
                }
            } else {
                self.edge_since = None;
            }
            if self.sidebar_open {
                if over_sidebar || over_edge || popup_open {
                    self.outside_since = None;
                } else {
                    let began = self.outside_since.get_or_insert_with(Instant::now);
                    if began.elapsed() > Duration::from_millis(400) {
                        self.sidebar_open = false;
                        ctx.request_repaint();
                    } else {
                        ctx.request_repaint_after(Duration::from_millis(400));
                    }
                }
                let available_height = (screen.bottom() - anchor.y - 28.).max(40.);
                let width = 220_f32.min((screen.width() - 24.).max(40.));
                let area = egui::Area::new(egui::Id::unique("directory-overlay"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(anchor)
                    .constrain_to(screen)
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(Color32::from_rgb(28, 30, 34))
                            .corner_radius(8.)
                            .stroke(egui::Stroke::new(1., modern_ui::BORDER))
                            .inner_margin(10.)
                            .show(ui, |ui| {
                                ui.set_width(width);
                                ui.set_max_height((available_height - 20.).max(20.));
                                ui.set_min_height((available_height - 20.).max(20.));
                                self.sidebar_contents(ui);
                            });
                    });
                self.sidebar_overlay_rect = Some(area.response.rect);
            } else {
                self.sidebar_overlay_rect = None;
            }
        } else {
            self.sidebar_overlay_rect = None;
            self.edge_since = None;
            self.outside_since = None;
            if self.detail.is_some() || self.workspace.is_open() {
                self.sidebar_open = false;
            }
        }
        if self.pending_trash.is_some() {
            if let Some(rect)=self.trash_undo_panel {self.control_regions.push(rect);}
        }
        self.detail(&ctx, &mut jobs);
        self.paint_closing_motion(&ctx);
        self.folder_undo_ui(&ctx);
        self.trash_ui(&ctx);
        self.rotation_dialog(&ctx);
        self.workspace_ui(&ctx);
        let list_visible = self.detail.is_none()
            && !self.trash_releasing()
            && self.rotation.is_none()
            && !self.workspace.is_open()
            && self.animation.released()
            && ctx.input(|i| {
                i.viewport().minimized != Some(true) && i.viewport().occluded != Some(true)
            });
        let requests = if list_visible {
            std::mem::take(&mut self.list_animation_requests)
        } else {
            Vec::new()
        };
        self.thumbnails.set_animation_pressure(!requests.is_empty());
        self.list_animations.submit(requests);
        if let Some(mut smoke) = self.design_smoke.take() {
            smoke.tick(self, &ctx);
            self.design_smoke = Some(smoke);
        }
        self.hover_tick(&ctx);
        self.playback_tick(&ctx);
        self.subtitle_tick();
        // Opt-in observation only: record state changes on existing draw callbacks.
        // Do not request extra frames or replace the original100ms sampling stream.
        if self.load_metrics.is_some()
            && self.load_transition_metrics.is_some()
            && self.started.elapsed() <= Duration::from_secs(10)
        {
            let (ready, clear, pending) = self.load_readiness_counts(&ctx);
            let mut ids: Vec<_> = self.list_regions.keys().copied().collect();
            ids.sort_unstable();
            let mut row = serde_json::json!({"visible_ready":ready,"visible_clear_ready":clear,
                "visible_pending_geometry":pending,"visible_cards":ids.len(),"visible_ids":ids,
                "generation":self.generation,"scan_complete":!self.scan_busy,
                "media":self.snapshot.as_ref().map_or(0, |s|s.files.len()),
                "viewport_points":[ctx.content_rect().width(),ctx.content_rect().height()],
                "pixels_per_point":ctx.pixels_per_point()});
            let rows = self.load_transition_metrics.as_mut().unwrap();
            let changed = rows.last().is_none_or(|last| {
                row.as_object()
                    .unwrap()
                    .iter()
                    .any(|(key, value)| last.get(key) != Some(value))
            });
            if changed && rows.len() < 3600 {
                row["elapsed_ms"] = serde_json::json!(self.started.elapsed().as_secs_f64() * 1000.);
                rows.push(row);
            }
        }
        if let Some((path, mut rows, mut last)) = self.load_metrics.take() {
            let elapsed = self.started.elapsed();
            if elapsed.saturating_sub(last) >= Duration::from_millis(100) && rows.len() < 3600 {
                let (ready, clear_ready, pending_geometry) = self.load_readiness_counts(&ctx);
                if ready > 0
                    && !rows
                        .iter()
                        .any(|r| r["visible_ready"].as_u64().unwrap_or(0) > 0)
                {
                    self.snapshot_image(&ctx, &path.with_extension("png").to_string_lossy());
                }
                rows.push(serde_json::json!({"elapsed_ms":elapsed.as_secs_f64()*1000.,"visible_ready":ready,"visible_clear_ready":clear_ready,"visible_pending_geometry":pending_geometry,"visible_cards":self.list_regions.len(),"scan_complete":!self.scan_busy,"metadata_ready":self.snapshot.as_ref().map_or(0, |s| s.files.iter().filter(|m|m.width.is_some()).count()),"media":self.snapshot.as_ref().map_or(0,|s|s.files.len()),"layout":self.prepared.as_ref().map(|p|serde_json::json!({"geometry":format!("{:016x}",layout_geometry_digest(&p.layout)),"cards":p.layout.rectangles.len(),"metadata_ready":p.snapshot.files.iter().filter(|m|m.width.is_some()).count(),"scroll_y":self.scroll_y})),"decoded":self.decoded,"cache_hits":self.cache_hits,"failures":self.failures.len(),"thumbnail_pipeline":self.thumbnails.stats(),"visible_gif_stats":self.list_animations.stats(),"visible_gif_texture_bytes":self.list_animations.texture_bytes(),"visible_gif_frames":self.list_animations.entries.iter().map(|(id,e)|serde_json::json!({"id":id,"sequence":e.sequence,"cycle":e.cycle,"ended":e.ended,"error":e.error})).collect::<Vec<_>>(),"texture_retention_policy":if self.adaptive_photo_uploads && self.compact_list_retention {"hot-plus16MiB-soft128MiB"} else {"legacy-double-hot-soft256MiB"},"texture_extra_video_posters":self.extra_video_posters,"texture_poster_policy":if self.adaptive_video_posters {"visible-list-posters-with-cpu-byte-guards"} else {"legacy-photo-only-no-active-video"},"texture_submission_policy":if self.adaptive_photo_uploads {"visible-photo-max4-cpu2ms-extra2MiB"} else {"fixed2"},"texture_extra_submissions":self.upload_extra_total,"texture_max_submissions_per_frame":self.upload_max_per_frame,"texture_extra_bytes_max":self.upload_extra_bytes_max,"texture_prepare_max_ms":self.upload_prepare_max_ms,"metadata_index_batch_counts":self.library.metadata_batch_counts(),"video_first_ready_render_ms":self.player.as_ref().and_then(|p|p.lock().ok().and_then(|p|p.first_ready_render_ms))}));
                if let Some(prepared) = &self.prepared {
                    let row = rows.last_mut().unwrap();
                    row["layout"]["build_ms"] = serde_json::json!(prepared.build_ms);
                    row["layout"]["view_reused"] = serde_json::json!(prepared.view_reused);
                }
                if self.benchmark.as_ref().is_some_and(|b| b.hover_video) {
                    rows.last_mut().unwrap()["video_render_timing"] = self
                        .player
                        .as_ref()
                        .map(|p| p.lock().unwrap().render_timing_diagnostic())
                        .unwrap_or(serde_json::Value::Null);
                }
                if let Some(row) = rows.last_mut() {
                    let mut visible_ids: Vec<_> = self.list_regions.keys().copied().collect();
                    visible_ids.sort_unstable();
                    row["visible_ids"] = serde_json::json!(visible_ids);
                    row["viewport_points"] = serde_json::json!([
                        ctx.content_rect().width(),
                        ctx.content_rect().height()
                    ]);
                    row["pixels_per_point"] = serde_json::json!(ctx.pixels_per_point());
                    row["native_pixels_per_point"] =
                        serde_json::json!(ctx.native_pixels_per_point());
                }
                last = elapsed;
            }
            self.load_metrics = Some((path, rows, last));
        }
        self.thumbnails.set_requests(
            self.generation,
            jobs.into_iter().filter(|job| !self.trash_blocks_media(job.id)).collect(),
        );
        if self.prepare_dirty {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        #[cfg(windows)]
        self.record_instance(&ctx);
        if let Some(mut smoke) = self.presentation_smoke.take() {
            smoke.tick(self, &ctx);
            self.presentation_smoke = Some(smoke);
        }
        if self.features.dragging {
            self.features.previous = ctx.input(|i| i.pointer.hover_pos());
        }
        if let Some(mut smoke) = self.features_smoke.take() {
            smoke.tick(self, &ctx);
            self.features_smoke = Some(smoke);
        }
        self.smoke(&ctx);
        if let Some(mut smoke) = self.daily_smoke.take() {
            smoke.tick(self, &ctx);
            self.daily_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.video_switch_smoke.take() {
            smoke.tick(self, &ctx);
            self.video_switch_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.folder_smoke.take() {
            smoke.tick(self, &ctx);
            self.folder_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.detail_smoke.take() {
            smoke.tick(self, &ctx);
            self.detail_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.edit_smoke.take() {
            smoke.tick(self, &ctx);
            self.edit_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.format_smoke.take() {
            smoke.tick(self, &ctx);
            self.format_smoke = Some(smoke);
        }
        if let Some(mut benchmark) = self.seek_benchmark.take() {
            benchmark.tick(self, &ctx);
            self.seek_benchmark = Some(benchmark);
        }
        if let Some(mut smoke) = self.trash_smoke.take() {
            smoke.tick(self, &ctx);
            self.trash_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.watch_smoke.take() {
            smoke.tick(self, &ctx);
            self.watch_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.input_smoke.take() {
            smoke.tick(self, &ctx);
            self.input_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.gif_smoke.take() {
            smoke.tick(self, &ctx);
            self.gif_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.animation_smoke.take() {
            smoke.tick(self, &ctx);
            self.animation_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.media_jump_smoke.take() {
            smoke.tick(self, &ctx);
            self.media_jump_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.empty_fold_smoke.take() {
            smoke.tick(self, &ctx);
            self.empty_fold_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.loading_smoke.take() {
            smoke.tick(self, &ctx);
            self.loading_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.navigation_smoke.take() {
            smoke.tick(self, &ctx);
            self.navigation_smoke = Some(smoke);
        }
        if let Some(mut smoke) = self.playback_smoke.take() {
            smoke.tick(self, &ctx);
            self.playback_smoke = Some(smoke);
        }
        if trace_first {
            crate::startup_trace::mark("first_ui_complete");
            ui.painter().add(egui::PaintCallback {
                rect: ui.max_rect(),
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(|_, _| {
                    crate::startup_trace::mark("first_gl_paint_callback");
                })),
            });
        }
        if let Some(after) = self.quit_after {
            if self.started.elapsed() >= after {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(after.saturating_sub(self.started.elapsed()));
            }
        }
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let (Some(path), Some(elapsed)) = (&self.first_ui_report, self.first_ui_ms) {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                let _ = std::fs::create_dir_all(parent);
            }
            let report = serde_json::json!({
                "internal_first_ui_ms": elapsed,
                "scope": "First UI callback after preferences; opt-in report written at orderly exit. Not physical presentation or interactivity."
            });
            let _ = std::fs::write(path, report.to_string());
        }
        if let (Some(p), Some(gl)) = (&self.player, gl) {
            p.lock().unwrap().release_gl(gl);
        }
        if let Some((path, rows, _)) = self.load_metrics.take() {
            let _ = std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"pending_geometry_scope":"possible-next-card-column-span","scope":"Native draw callbacks with ready visible textures, not physical presentation timestamps","rows":rows,"transitions":self.load_transition_metrics,"transition_scope":"Opt-in existing draw callbacks during first10s; state changes only; no extra repaint; periodic rows unchanged"})).unwrap());
        }
        let config = Config {
            auto_fold_empty: self.auto_fold_empty,
            config_version: 2,
            language: self.language.clone(),
            places: self.places.clone(),
            presentation: self.presentation.prefs.clone(),
            appearance: self.workspace.appearance.clone(),
            subtitles: self.subtitles.clone(),
            video_end_action: self.video_end_action,
            shortcuts: self.shortcuts.clone(),
            settings: self.settings.clone(),
            target_width: self.target_width,
            pinned: self.pinned,
            // Navigation clears the old snapshot before the new scan completes.
            // Persist the requested view as well when quitting in that interval.
            last_root: self.watch_root.clone().or_else(|| self.snapshot.as_ref().map(|s| s.root.clone())),
            volume: self.volume,
            muted: self.muted,
        };
        if let Ok(data) = serde_json::to_vec_pretty(&config) {
            let _ = std::fs::create_dir_all(&self.data_dir);
            let temporary = self.data_dir.join("settings.tmp");
            if std::fs::write(&temporary, data).is_ok() {
                let _ = std::fs::rename(temporary, self.data_dir.join("settings.json"));
            }
        }
    }
}
fn layout_geometry_digest(layout: &media_browser::layout::MasonryLayout) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    layout.height.to_bits().hash(&mut hash);
    for column in &layout.columns {
        for card in column {
            card.id.hash(&mut hash);
            for value in [card.rect.x, card.rect.y, card.rect.w, card.rect.h] {
                value.to_bits().hash(&mut hash);
            }
        }
    }
    hash.finish()
}

fn media_scroll_area(epoch: u64) -> egui::ScrollArea {
    // Only the list canvas is faster: routed video seeking and filmstrip wheels
    // have already consumed their input, and other scroll areas keep their speed.
    egui::ScrollArea::vertical()
        .id_salt(("media", epoch))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .wheel_scroll_multiplier(egui::vec2(1., 1.5))
        .auto_shrink([false, false])
}

fn show_media_scroll<R>(
    ui: &mut egui::Ui,
    epoch: u64,
    offset: Option<f32>,
    body: impl FnOnce(&mut egui::Ui, Rect) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    let old_scroll = ui.spacing().scroll;
    ui.spacing_mut().scroll.floating = false;
    ui.spacing_mut().scroll.bar_width = 14.;
    ui.spacing_mut().scroll.bar_inner_margin = 4.;
    ui.spacing_mut().scroll.bar_outer_margin = 0.;
    ui.spacing_mut().scroll.handle_min_length = 44.;
    ui.spacing_mut().scroll.foreground_color = true;
    let mut area = media_scroll_area(epoch);
    if let Some(offset) = offset {
        // egui integrates against the previous frame's bounds before measuring
        // this frame's content. A restored position in newly grown content must
        // not create a rubber-band velocity against those obsolete bounds.
        ui.spacing_mut().scroll.kinetic.rubber_band = false;
        area = area.vertical_scroll_offset(offset);
    }
    let result = area.show_viewport(ui, body);
    ui.spacing_mut().scroll = old_scroll;
    result
}

#[cfg(test)]
mod scrolling_tests {
    use super::*;

    #[test]
    fn visible_empty_column_does_not_wait_for_a_below_screen_sizing_label() {
        use media_browser::layout::{MasonryLayout, Rect as LayoutRect, SizingRow};
        let mut layout = MasonryLayout::default();
        layout.sizing_rows.push(SizingRow {
            rect: LayoutRect {
                x: 0.,
                y: 1600.,
                w: 1500.,
                h: 34.,
            },
            probe_top: 40.,
            items: vec![5, 6, 7],
        });
        assert_eq!(
            viewport_geometry_requests(&layout, 0., 600., 2, false, |_| false),
            vec![(5, 2), (6, 2)]
        );
        assert!(viewport_geometry_requests(&layout, 0., 600., 2, true, |_| false).is_empty());
        // Once the viewport is below this whole unresolved span it is not
        // allowed to displace dimensions belonging to the watched position.
        assert!(viewport_geometry_requests(&layout, 3000., 3600., 2, false, |_| false).is_empty());
    }

    #[test]
    fn viewport_dimensions_do_not_wait_behind_above_viewport_prefetch() {
        use media_browser::layout::{MasonryLayout, Rect as LayoutRect, SizingRow};
        let row = |y, items| SizingRow {
            probe_top: y,
            rect: LayoutRect {
                x: 0.,
                y,
                w: 300.,
                h: 34.,
            },
            items,
        };
        let mut layout = MasonryLayout::default();
        layout.sizing_rows = vec![
            row(100., (1..=24).collect()),
            row(550., vec![31, 32, 33]),
            row(900., vec![41, 42]),
            row(1300., vec![51]),
            row(3000., vec![99]),
        ];
        // A three-job budget used to be exhausted entirely by the first row
        // above the viewport, leaving the watched position blank.
        assert_eq!(
            viewport_geometry_requests(&layout, 500., 1000., 3, false, |_| false),
            vec![(31, 2), (32, 2), (33, 2)]
        );
        let requests = viewport_geometry_requests(&layout, 500., 1000., 32, false, |_| false);
        assert_eq!(
            &requests[..5],
            &[(31, 2), (32, 2), (33, 2), (41, 2), (42, 2)]
        );
        assert!(requests[5..].iter().all(|(_, priority)| *priority == 4));
        assert!(!requests.iter().any(|(id, _)| *id == 99));
        // Reversing the scroll changes which dimensions are urgent; ordinary
        // top-to-bottom order remains deterministic within the new viewport.
        assert_eq!(
            viewport_geometry_requests(&layout, 0., 400., 2, false, |_| false),
            vec![(1, 2), (2, 2)]
        );
    }

    #[test]
    fn completed_dimension_probes_free_quota_before_snapshot_without_crossing_versions() {
        use media_browser::layout::{MasonryLayout, Rect as LayoutRect, SizingRow};
        let mut layout = MasonryLayout::default();
        layout.sizing_rows.push(SizingRow {
            probe_top: 50.,
            rect: LayoutRect {
                x: 0.,
                y: 50.,
                w: 300.,
                h: 34.,
            },
            items: (1..=64).collect(),
        });
        let version = Fingerprint {
            bytes: 1,
            modified_ns: 1,
        };
        let changed = Fingerprint {
            bytes: 2,
            modified_ns: 1,
        };
        let mut hints = DimensionHints::default();
        for id in 1..=16 {
            hints.remember(1, id, version.clone());
        }
        let requests = viewport_geometry_requests(&layout, 0., 400., 16, false, |id| {
            hints.contains(1, id, &version)
        });
        assert_eq!(requests, (17..=32).map(|id| (id, 2)).collect::<Vec<_>>());
        assert_eq!(
            viewport_geometry_requests(&layout, 0., 400., 1, false, |id| hints
                .contains(2, id, &version)),
            vec![(1, 2)]
        );
        assert_eq!(
            viewport_geometry_requests(&layout, 0., 400., 1, false, |id| hints
                .contains(1, id, &changed)),
            vec![(1, 2)]
        );
        for id in 17..=64 {
            hints.remember(1, id, version.clone());
        }
        assert!(
            viewport_geometry_requests(&layout, 0., 400., 16, false, |id| hints
                .contains(1, id, &version))
            .is_empty()
        );
        for id in 65..=257 {
            hints.remember(1, id, version.clone());
        }
        assert_eq!(hints.versions.len(), 256);
        assert_eq!(hints.order.len(), 256);
        assert!(!hints.contains(1, 1, &version));
        hints.remember(2, 1, changed.clone());
        assert_eq!(hints.versions.len(), 1);
        assert!(!hints.contains(2, 257, &version));
        assert!(hints.contains(2, 1, &changed));
    }

    #[test]
    fn list_cache_retires_old_tiles_but_keeps_recent_texture_handles() {
        let ctx = egui::Context::default();
        let mut textures = HashMap::new();
        let mut insert = |name: &str, last_frame| {
            textures.insert(
                name.to_owned(),
                TextureEntry {
                    texture: ctx.load_texture(
                        name,
                        egui::ColorImage::new([1, 1], vec![egui::Color32::LIGHT_BLUE]),
                        egui::TextureOptions::LINEAR,
                    ),
                    // Test accounting/retention without allocating GPU MiBs.
                    bytes: 8 * 1024 * 1024,
                    last_frame,
                    source_size: (1, 1),
                },
            );
        };
        for n in 1..=20 {
            insert(&format!("old-{n}"), n);
        }
        for n in 0..3 {
            insert(&format!("hot-{n}"), 99);
        }
        insert("fresh", 100);
        let protected: Vec<_> = ["hot-0", "hot-1", "hot-2", "fresh"]
            .into_iter()
            .map(|name| (name, textures[name].texture.id()))
            .collect();
        let bytes = textures.values().map(|entry| entry.bytes).sum::<usize>();
        let bytes = trim_inactive_list_textures(&mut textures, bytes, 100);
        assert_eq!(bytes, 64 * 1024 * 1024);
        assert_eq!(
            bytes,
            textures.values().map(|entry| entry.bytes).sum::<usize>()
        );
        for (name, id) in protected {
            assert_eq!(textures[name].texture.id(), id);
        }
        for n in 1..=16 {
            assert!(!textures.contains_key(&format!("old-{n}")));
        }
        for n in 17..=20 {
            assert!(textures.contains_key(&format!("old-{n}")));
        }
        for entry in textures.values_mut() {
            entry.last_frame = 101;
        }
        let count = textures.len();
        assert_eq!(
            trim_inactive_list_textures(&mut textures, bytes, 102),
            bytes
        );
        assert_eq!(textures.len(), count);
    }

    #[test]
    fn compact_retention_keeps_visible_handles_even_above_soft_limit() {
        let ctx = egui::Context::default();
        let mut textures = HashMap::new();
        for id in 0..36 {
            textures.insert(
                format!("item-{id}"),
                TextureEntry {
                    texture: ctx.load_texture(
                        format!("item-{id}"),
                        egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
                        egui::TextureOptions::LINEAR,
                    ),
                    bytes: 8 * 1024 * 1024,
                    last_frame: if id < 20 { id as u64 } else { 99 },
                    source_size: (1, 1),
                },
            );
        }
        let protected: Vec<_> = textures
            .iter()
            .filter(|(_, entry)| entry.last_frame >= 99)
            .map(|(name, entry)| (name.clone(), entry.texture.id()))
            .collect();
        let bytes = textures.values().map(|entry| entry.bytes).sum();
        let bytes = trim_inactive_list_textures_with_policy(&mut textures, bytes, 100, true);
        assert_eq!(bytes, 128 * 1024 * 1024);
        for (name, id) in protected {
            assert_eq!(textures[&name].texture.id(), id);
        }
        // Recent content is protected rather than degraded to satisfy a soft cap.
        let more = ctx.load_texture(
            "more",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            egui::TextureOptions::LINEAR,
        );
        textures.insert(
            "more".into(),
            TextureEntry {
                texture: more,
                bytes: 8 * 1024 * 1024,
                last_frame: 100,
                source_size: (1, 1),
            },
        );
        assert_eq!(
            trim_inactive_list_textures_with_policy(
                &mut textures,
                bytes + 8 * 1024 * 1024,
                100,
                true
            ),
            136 * 1024 * 1024
        );
        assert_eq!(textures.len(), 17);
    }

    fn wheel_distance(faster: bool, unit: egui::MouseWheelUnit) -> f32 {
        let ctx = egui::Context::default();
        let mut offset = 0.;
        for frame in 0..90 {
            let mut raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800., 600.))),
                time: Some(frame as f64 / 60.),
                ..Default::default()
            };
            raw.events
                .push(egui::Event::PointerMoved(egui::pos2(300., 250.)));
            if frame == 3 {
                raw.events.push(egui::Event::MouseWheel {
                    unit,
                    delta: egui::vec2(0., -3.),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Default::default(),
                });
            }
            let mut output = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut area = if faster {
                        media_scroll_area(1)
                    } else {
                        egui::ScrollArea::vertical().auto_shrink([false, false])
                    };
                    if frame == 0 {
                        area = area.vertical_scroll_offset(500.);
                    }
                    offset = area
                        .show_viewport(ui, |ui, _| ui.set_min_height(10000.))
                        .state
                        .clamped_offset()
                        .y;
                });
            });
            // Headless geometry test: no GPU consumes texture uploads here.
            output.textures_delta.clear();
        }
        offset - 500.
    }

    #[test]
    fn list_wheel_travels_one_and_half_times_without_changing_other_areas() {
        for unit in [egui::MouseWheelUnit::Line, egui::MouseWheelUnit::Point] {
            let normal = wheel_distance(false, unit);
            let faster = wheel_distance(true, unit);
            assert!(normal > 1., "No scroll: {normal}");
            assert!(
                (faster / normal - 1.5).abs() < 0.02,
                "{unit:?}: normal={normal}, faster={faster}"
            );
        }
    }

    #[test]
    fn restored_scroll_in_grown_content_does_not_create_spring_velocity() {
        let run = |guarded: bool| {
            let ctx = egui::Context::default();
            let mut offset = 0.;
            for frame in 0..90 {
                let raw = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800., 600.))),
                    time: Some(frame as f64 / 60.),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(raw, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        let requested = (frame == 2).then_some(800.);
                        let height = if frame < 2 { 1000. } else { 1800. };
                        let original = ui.spacing().scroll.kinetic.rubber_band;
                        let state = if guarded {
                            show_media_scroll(ui, 1, requested, |ui, _| ui.set_min_height(height))
                                .state
                        } else {
                            let mut area = media_scroll_area(1);
                            if let Some(value) = requested {
                                area = area.vertical_scroll_offset(value);
                            }
                            area.show_viewport(ui, |ui, _| ui.set_min_height(height))
                                .state
                        };
                        assert_eq!(ui.spacing().scroll.kinetic.rubber_band, original);
                        offset = state.clamped_offset().y;
                    });
                });
                output.textures_delta.clear();
            }
            offset
        };
        let control = run(false);
        let guarded = run(true);
        assert!(
            (control - 800.).abs() > 1.,
            "Control did not reproduce drift: {control}"
        );
        assert!(
            (guarded - 800.).abs() < 0.1,
            "Restored position drifted: {guarded}"
        );
    }
}

fn to_rect(rect: media_browser::layout::Rect, origin: Pos2) -> Rect {
    Rect::from_min_size(
        origin + egui::vec2(rect.x, rect.y),
        egui::vec2(rect.w, rect.h),
    )
}
fn contain(rect: Rect, size: Vec2) -> Rect {
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    Rect::from_center_size(rect.center(), size * scale)
}
fn thumbnail_bucket(width: f32) -> u32 {
    if width <= 256. {
        256
    } else if width <= 512. {
        512
    } else {
        1024
    }
}
fn format_time(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        text.trim_start_matches(r"\\?\").to_owned()
    }
}
fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("LargeScreenMediaBrowser")
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Library/Application Support/LargeScreenMediaBrowser")
    }
}
