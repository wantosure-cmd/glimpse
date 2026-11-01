//! Opt-in, read-only design regression. Events enter through the native input hook.
use super::*;
pub(super) struct DesignSmoke {
    pub menu_rect: Option<Rect>,
    pub toolbar_sidebar: Option<Rect>,
    pub toolbar_search: Option<Rect>,
    pub toolbar_refresh: Option<Rect>,
    pub toolbar_folder: Option<Rect>,
    pub toolbar_settings: Option<Rect>,
    pub toolbar_brand: Option<Rect>,
    pub performance_rect: Option<Rect>,
    pub toolbar_filter: Option<Rect>,
    pub window_close: Option<Rect>,
    pub window_maximize: Option<Rect>,
    pub window_minimize: Option<Rect>,
    toolbar_probe: u8,
    slider_release: Option<Pos2>,
    pub rotation_regions: HashMap<bool, Rect>,
    output: PathBuf,
    phase: usize,
    at: Instant,
    started: Instant,
    events: Vec<egui::Event>,
    rows: Vec<serde_json::Value>,
    initial_count: usize,
    captured: bool,
    rotation_video: Option<MediaId>,
    resize_baseline: Option<(usize, f32)>,
    resize_only: bool,
    navigation_baseline: Vec<Rect>,
    sidebar_list_width: f32,
}
impl DesignSmoke {
    pub fn new(output: PathBuf) -> Self {
        Self {
            menu_rect: None,
            toolbar_sidebar: None,
            toolbar_search: None,
            toolbar_refresh: None,
            toolbar_folder: None,
            toolbar_settings: None,
            toolbar_brand: None,
            performance_rect: None,
            toolbar_filter: None,
            window_close: None,
            window_maximize: None,
            window_minimize: None,
            toolbar_probe: 0,
            slider_release: None,
            rotation_regions: HashMap::new(),
            output,
            phase: 0,
            at: Instant::now(),
            started: Instant::now(),
            events: vec![],
            rows: vec![],
            initial_count: 0,
            captured: false,
            rotation_video: None,
            resize_baseline: None,
            resize_only: false,
            navigation_baseline: vec![],
            sidebar_list_width: 0.,
        }
    }
    pub fn new_resize(output: PathBuf) -> Self {
        let mut probe = Self::new(output);
        probe.resize_only = true;
        probe
    }
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        if self.resize_only {
            raw.events.clear();
            raw.events.push(egui::Event::PointerGone);
            raw.focused = false;
            return;
        }
        raw.focused = true;
        raw.events.append(&mut self.events);
        if let Some(pos) = self.slider_release.take() {
            self.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            });
        }
    }
    fn key(&mut self, key: egui::Key, ctrl: bool) {
        for pressed in [true, false] {
            self.events.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl,
                    command: ctrl,
                    ..Default::default()
                },
            });
        }
    }
    fn click(&mut self, pos: Pos2) {
        self.events.push(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            self.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
    }
    fn slider_press(&mut self, pos: Pos2) {
        self.events.push(egui::Event::PointerMoved(pos));
        self.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        self.slider_release = Some(pos);
    }
    fn advance(&mut self) {
        self.captured = false;
        self.phase += 1;
        self.at = Instant::now();
    }
    fn capture(&mut self, b: &Browser, ctx: &egui::Context, name: &str) {
        self.rows.push(serde_json::json!({"name":format!("layout-{name}"),
            "viewport_width":ctx.content_rect().width(),"list_width":b.last_width,
            "columns":b.prepared.as_ref().map(|p|p.layout.columns.len()),
            "layout_width":b.prepared.as_ref().and_then(|p|p.layout.headers.first()).map(|h|h.rect.w+12.),
            "detail_header":b.detail_layout.header.map(|r|[r.left(),r.top(),r.width(),r.height()]),
            "detail_canvas":b.detail_layout.canvas.map(|r|[r.left(),r.top(),r.width(),r.height()])}));
        let rect = |r: Rect| [r.left(), r.top(), r.width(), r.height()];
        let mut row = serde_json::json!({"name":name,"screen":rect(ctx.content_rect()),"pixels_per_point":ctx.pixels_per_point(),"sidebar_open":b.sidebar_open,"sidebar_pinned":b.pinned,"toolbar_sidebar":self.toolbar_sidebar.map(rect),"toolbar_search":self.toolbar_search.map(rect),"toolbar_refresh":self.toolbar_refresh.map(rect),"toolbar_folder":self.toolbar_folder.map(rect),"toolbar_settings":self.toolbar_settings.map(rect),"toolbar_brand":self.toolbar_brand.map(rect),"performance":self.performance_rect.map(rect),"toolbar_filter":self.toolbar_filter.map(rect),"settings":b.workspace.settings_open,"command":b.workspace.command_open,"category":format!("{:?}",b.workspace.category),"settings_rect":b.workspace.settings_rect.map(rect),"command_rect":b.workspace.command_rect.map(rect),"query":b.workspace.query,"selected":b.workspace.selected,"filter":format!("{:?}",b.settings.filter),"video_rotate":b.player.as_ref().and_then(|p|p.lock().unwrap().number("video-rotate")),"popup_open":egui::Popup::is_any_open(ctx),"menu_rect":self.menu_rect.map(rect),"rotation_regions":self.rotation_regions.iter().map(|(k,r)|(*k,rect(*r))).collect::<Vec<_>>(),"status":b.status,"transport":b.detail_layout.transport.map(rect),"seek":b.detail_layout.seek.map(rect),"volume_slider":b.detail_layout.volume.map(rect),"play_pause":b.detail_layout.play_pause.map(rect),"mute_button":b.detail_layout.mute.map(rect),"expand_button":b.detail_layout.expand.map(rect),"current_time":b.detail_layout.current_time.map(rect),"total_time":b.detail_layout.total_time.map(rect),"volume":b.volume,"muted":b.muted,"pause":b.player.as_ref().and_then(|p|p.lock().unwrap().text("pause")),"time":b.player.as_ref().and_then(|p|p.lock().unwrap().number("time-pos")),"duration":b.player.as_ref().and_then(|p|p.lock().unwrap().number("duration")),"video_region":b.video_region.map(|(_,r)|rect(r)),"display_size":b.player.as_ref().and_then(|p|p.lock().unwrap().display_size())});
        row["detail_open"] = serde_json::json!(b.detail.is_some());
        row["detail_close"] = serde_json::json!(b.detail_layout.close.map(rect));
        row["window_controls"] = serde_json::json!([
            self.window_minimize.map(rect),
            self.window_maximize.map(rect),
            self.window_close.map(rect)
        ]);
        row["window_close_hotspot"] = serde_json::json!(b.features.close_rect.map(rect));
        row["sidebar_overlay"] = serde_json::json!(b.sidebar_overlay_rect.map(rect));
        row["sidebar_pin"] = serde_json::json!(b.sidebar_pin_rect.map(rect));
        row["list_width"] = serde_json::json!(b.last_width);
        row["detail_arrows"] = serde_json::json!(b
            .detail_layout
            .arrows
            .iter()
            .copied()
            .map(rect)
            .collect::<Vec<_>>());
        row["detail_kind"] = serde_json::json!(b.detail.as_ref().and_then(|d| b
            .prepared
            .as_ref()
            .and_then(|p| p
                .lookup
                .get(&d.id)
                .map(|i| format!("{:?}", p.snapshot.files[*i].file.kind)))));
        self.rows.push(row);
        b.snapshot_image(ctx, &format!("design-{name}.png"));
    }
    pub fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        if self.resize_only {
            self.tick_resize(b, ctx);
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(20));
        if self.started.elapsed() > Duration::from_secs(90) {
            self.finish(ctx, Some("timeout"));
            return;
        }
        if b.prepared
            .as_ref()
            .is_none_or(|p| !p.snapshot.finished || p.view.sequence.is_empty())
            || b.prepare_dirty
        {
            return;
        }
        if self.at.elapsed() < Duration::from_millis(380) {
            return;
        }
        // Native click proof for the requested placement, before the existing
        // settings/commands/media/resize suite. No physical cursor movement.
        if self.phase == 0 && self.toolbar_probe < 13 && !b.textures.is_empty() {
            let (Some(sidebar), Some(settings), Some(brand), Some(filter)) = (
                self.toolbar_sidebar,
                self.toolbar_settings,
                self.toolbar_brand,
                self.toolbar_filter,
            ) else {
                return;
            };
            let (Some(search), Some(refresh), Some(folder)) = (
                self.toolbar_search,
                self.toolbar_refresh,
                self.toolbar_folder,
            ) else {
                return;
            };
            let actions = [search, refresh, folder, settings];
            // Timed hover/close and pin reflow can complete on the following
            // native frame, especially after a diagnostic screenshot write.
            let waiting = match self.toolbar_probe {
                1..=3 | 5 | 7 => !b.sidebar_open || b.sidebar_overlay_rect.is_none(),
                4 | 8 => b.sidebar_open || b.sidebar_overlay_rect.is_some(),
                6 => !b.pinned || b.last_width >= self.sidebar_list_width - 100.,
                _ => false,
            };
            if waiting {
                return;
            }
            if actions.iter().any(|r| {
                r.top() < brand.bottom()
                    || (r.center().y - settings.center().y).abs() > 2.
                    || r.right() > ctx.content_rect().right()
            }) || actions.windows(2).any(|r| r[0].right() > r[1].left())
            {
                self.finish(
                    ctx,
                    Some("folder action/settings group misplaced or overlapping"),
                );
                return;
            }
            let error = match self.toolbar_probe {
                0 if sidebar.top() < brand.bottom()
                    || sidebar.left() >= filter.left()
                    || (sidebar.center().y - settings.center().y).abs() > 2.
                    || settings.right() > ctx.content_rect().right() =>
                {
                    Some("toolbar placement outside requested second row")
                }
                1..=3 | 5 | 7
                    if b.pinned
                        || !b.sidebar_open
                        || b.sidebar_overlay_rect.is_none()
                        || (b.last_width - self.sidebar_list_width).abs() > 0.25 =>
                {
                    Some("hover directory overlay closed or reflowed list")
                }
                4 | 8 if b.pinned || b.sidebar_open || b.sidebar_overlay_rect.is_some() => {
                    Some("directory overlay failed to close after pointer left")
                }
                6 if !b.pinned
                    || !b.sidebar_open
                    || b.last_width >= self.sidebar_list_width - 100. =>
                {
                    Some("explicit directory pin failed")
                }
                9 if !b.workspace.settings_open => {
                    Some("second-row settings button failed to open")
                }
                10 if b.workspace.settings_open => {
                    Some("settings escape failed after button entry")
                }
                11 if !b.workspace.command_open => Some("second-row search button failed to open"),
                12 if b.workspace.command_open => {
                    Some("search escape failed after second-row entry")
                }
                _ => None,
            };
            if let Some(error) = error {
                self.finish(ctx, Some(error));
                return;
            }
            let name = [
                "toolbar-layout",
                "toolbar-sidebar-hover",
                "toolbar-sidebar-gap",
                "toolbar-sidebar-inside",
                "toolbar-sidebar-left",
                "toolbar-sidebar-click",
                "toolbar-sidebar-pinned",
                "toolbar-sidebar-unpinned",
                "toolbar-sidebar-closed",
                "toolbar-settings-open",
                "toolbar-settings-closed",
                "toolbar-search-open",
                "toolbar-search-closed",
            ][self.toolbar_probe as usize];
            self.capture(b, ctx, name);
            match self.toolbar_probe {
                0 => {
                    self.sidebar_list_width = b.last_width;
                    self.events
                        .push(egui::Event::PointerMoved(sidebar.center()));
                }
                1 => self.events.push(egui::Event::PointerMoved(egui::pos2(
                    sidebar.center().x,
                    sidebar.bottom() + 2.,
                ))),
                2 => self.events.push(egui::Event::PointerMoved(
                    b.sidebar_overlay_rect.unwrap().center(),
                )),
                3 | 7 => self.events.push(egui::Event::PointerMoved(egui::pos2(
                    ctx.content_rect().right() - 50.,
                    ctx.content_rect().center().y,
                ))),
                4 => self.click(sidebar.center()),
                5 | 6 => {
                    let Some(pin) = b.sidebar_pin_rect else {
                        self.finish(ctx, Some("Directory pin control missing"));
                        return;
                    };
                    self.click(pin.center());
                }
                8 => self.click(settings.center()),
                9 | 11 => self.key(egui::Key::Escape, false),
                10 => self.click(search.center()),
                _ => {}
            }
            self.toolbar_probe += 1;
            self.at = Instant::now();
            return;
        }
        if !self.captured {
            let name = match self.phase {
                0 => Some(String::from("library")),
                1..=6 => Some(format!("settings-{}", self.phase)),
                8 => Some(String::from("commands")),
                9 => Some(String::from("search")),
                17 => Some(String::from("scale-100")),
                18 => Some(String::from("scale-200")),
                20 => Some(String::from("detail")),
                21 => Some(String::from("detail-settings")),
                23 => Some(String::from("context-menu")),
                26 => Some(String::from("sidebar")),
                29 => Some(String::from("video-menu")),
                30 => Some(String::from("video-right-90")),
                33 => Some(String::from("video-restored")),
                36 => Some(String::from("video-left-270")),
                39 => Some(String::from("video-other-reset")),
                42 => Some(String::from("video-reopen-kept")),
                44 => Some(String::from("resize-narrow")),
                45 => Some(String::from("resize-wide")),
                47 => Some(String::from("zoomed-translucent-bars-favorite")),
                48 => Some(String::from("detail-video-wide")),
                49 => Some(String::from("detail-video-paused")),
                50 => Some(String::from("detail-video-playing")),
                51 => Some(String::from("detail-video-seek")),
                52 => Some(String::from("detail-video-volume")),
                54 => Some(String::from("detail-video-compact")),
                55 => Some(String::from("detail-video-narrow")),
                56 => Some(String::from("detail-video-wide-resized")),
                57 => Some(String::from("detail-close-returned")),
                77 => Some(String::from("window-close-hover")),
                78 => Some(String::from("window-maximize-hover")),
                79 => Some(String::from("window-minimize-hover")),
                80 => Some(String::from("window-controls-compact")),
                81 => Some(String::from("window-controls-scale200")),
                82 => Some(String::from("window-controls-scale100")),
                _ => None,
            };
            if let Some(name) = name {
                if self.phase == 0 && b.textures.is_empty() {
                    return;
                }
                if matches!(self.phase, 17 | 18)
                    && b.workspace
                        .settings_rect
                        .is_some_and(|r| !ctx.content_rect().expand(2.).contains_rect(r))
                {
                    self.finish(ctx, Some("scaled settings exceed viewport"));
                    return;
                }
                self.capture(b, ctx, &name);
                self.captured = true;
                self.at = Instant::now();
                return;
            }
        }
        let fail = match self.phase {
            0 => {
                self.initial_count = b.snapshot.as_ref().unwrap().files.len();
                self.key(egui::Key::Comma, true);
                None
            }
            1..=6 => {
                let category = workspace_ui::Category::ALL[self.phase - 1];
                if !b.workspace.settings_open || b.workspace.category != category {
                    Some("settings keyboard entry or category click failed")
                } else {
                    if self.phase < 6 {
                        let next = workspace_ui::Category::ALL[self.phase];
                        if let Some((_, r)) = b
                            .workspace
                            .category_regions
                            .iter()
                            .find(|(c, _)| *c == next)
                        {
                            self.click(r.center());
                        } else {
                            self.finish(ctx, Some("category hit region missing"));
                            return;
                        }
                    } else {
                        self.key(egui::Key::Escape, false);
                    }
                    None
                }
            }
            7 => {
                if b.workspace.is_open() {
                    Some("Escape did not close settings")
                } else {
                    self.key(egui::Key::K, true);
                    None
                }
            }
            8 => {
                if !b.workspace.command_open {
                    Some("Ctrl+K failed")
                } else {
                    self.events.push(egui::Event::Text("images".into()));
                    None
                }
            }
            9 => {
                if b.workspace.query != "images" {
                    Some("command search did not receive text")
                } else {
                    self.key(egui::Key::Enter, false);
                    None
                }
            }
            10 => {
                if b.workspace.command_open || b.settings.filter != MediaFilter::Images {
                    Some("Enter did not execute image filter")
                } else {
                    self.key(egui::Key::K, true);
                    None
                }
            }
            11 => {
                self.key(egui::Key::ArrowDown, false);
                None
            }
            12 => {
                if b.workspace.selected != 1 {
                    Some("ArrowDown did not select command")
                } else {
                    self.key(egui::Key::ArrowUp, false);
                    None
                }
            }
            13 => {
                if b.workspace.selected != 0 {
                    Some("ArrowUp did not select command")
                } else {
                    self.key(egui::Key::Escape, false);
                    None
                }
            }
            14 => {
                if b.workspace.is_open() {
                    Some("Escape did not close commands")
                } else {
                    self.key(egui::Key::Comma, true);
                    None
                }
            }
            15 => {
                // The selected list item must survive a Delete key behind a modal.
                let id = b.prepared.as_ref().unwrap().view.sequence[0];
                b.selection.insert(id);
                self.key(egui::Key::Delete, false);
                self.key(egui::Key::Space, false);
                None
            }
            16 => {
                if b.trash_operation.is_some()
                    || b.snapshot.as_ref().unwrap().files.len() != self.initial_count
                    || b.selection.is_empty()
                {
                    Some("modal input leaked into media operations")
                } else {
                    b.workspace.category = workspace_ui::Category::Appearance;
                    ctx.set_zoom_factor(1. / ctx.native_pixels_per_point().unwrap_or(1.));
                    None
                }
            }
            17 => {
                ctx.set_zoom_factor(2. / ctx.native_pixels_per_point().unwrap_or(1.));
                None
            }
            18 => {
                ctx.set_zoom_factor(1.);
                self.key(egui::Key::Escape, false);
                None
            }
            19 => {
                b.selection.clear();
                b.open_detail(b.prepared.as_ref().unwrap().view.sequence[0]);
                None
            }
            20 => {
                self.key(egui::Key::Comma, true);
                None
            }
            21 => {
                self.key(egui::Key::Escape, false);
                None
            }
            22 => {
                if b.detail.is_none() {
                    Some("closing settings also closed detail")
                } else {
                    let pos = b.detail_layout.canvas.unwrap().center();
                    self.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        self.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: Default::default(),
                        });
                    }
                    None
                }
            }
            23 => {
                if !egui::Popup::is_any_open(ctx) {
                    Some("detail context menu failed")
                } else {
                    self.key(egui::Key::Escape, false);
                    None
                }
            }
            24 => {
                b.close_detail();
                b.pinned = true;
                b.settings.filter = MediaFilter::All;
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                None
            }
            25 => None,
            26 => {
                let p = b.prepared.as_ref().unwrap();
                let id = p
                    .view
                    .sequence
                    .iter()
                    .find(|id| {
                        p.lookup
                            .get(id)
                            .is_some_and(|i| p.snapshot.files[*i].file.kind == MediaKind::Video)
                    })
                    .copied();
                if let Some(id) = id {
                    self.rotation_video = Some(id);
                    b.open_detail(id);
                    None
                } else {
                    Some("no video rotation fixture")
                }
            }
            27 => {
                if !b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready()) {
                    return;
                }
                let _ = b
                    .player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .set("pause", "yes");
                None
            }
            28 | 31 | 34 => {
                self.rotation_regions.clear();
                let pos = b.detail_layout.canvas.unwrap().center();
                self.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true, false] {
                    self.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
                None
            }
            29 | 32 | 35 => {
                let clockwise = self.phase == 29;
                if let Some(r) = self.rotation_regions.get(&clockwise) {
                    self.click(r.center());
                    None
                } else {
                    Some("video rotation menu missing")
                }
            }
            30 | 33 | 36 => {
                let expected = if self.phase == 30 {
                    90.
                } else if self.phase == 33 {
                    0.
                } else {
                    270.
                };
                if b.player
                    .as_ref()
                    .and_then(|p| p.lock().unwrap().number("video-rotate"))
                    != Some(expected)
                {
                    Some("native rotation property did not match menu action")
                } else if b.detail.as_ref().map(|v| v.id) != self.rotation_video {
                    Some("rotation unexpectedly navigated media")
                } else {
                    None
                }
            }
            37 => {
                let p = b.prepared.as_ref().unwrap();
                let other = p
                    .view
                    .sequence
                    .iter()
                    .find(|id| {
                        Some(**id) != self.rotation_video
                            && p.lookup
                                .get(id)
                                .is_some_and(|i| p.snapshot.files[*i].file.kind == MediaKind::Video)
                    })
                    .copied();
                if let Some(id) = other {
                    b.open_detail(id);
                    None
                } else {
                    Some("second video fixture missing")
                }
            }
            38 | 41 => {
                if !b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready()) {
                    return;
                }
                let _ = b
                    .player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .set("pause", "yes");
                None
            }
            39 => {
                if b.player
                    .as_ref()
                    .and_then(|p| p.lock().unwrap().number("video-rotate"))
                    != Some(0.)
                {
                    Some("rotation leaked to another video")
                } else {
                    None
                }
            }
            40 => {
                b.open_detail(self.rotation_video.unwrap());
                None
            }
            42 => {
                if b.player
                    .as_ref()
                    .and_then(|p| p.lock().unwrap().number("video-rotate"))
                    != Some(270.)
                {
                    Some("reopening did not restore video rotation")
                } else {
                    b.close_detail();
                    b.pinned = false;
                    b.target_width = 240.;
                    b.prepare_dirty = true;
                    b.prepare_after = Instant::now();
                    ctx.set_pixels_per_point(1.);
                    None
                }
            }
            43 => {
                // The scale change must reach winit before requesting a size;
                // otherwise a same-frame size command uses the old native scale.
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(820., 720.)));
                None
            }
            44 => {
                let p = b.prepared.as_ref().unwrap();
                self.resize_baseline = Some((p.layout.columns.len(), b.last_width));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1550., 900.)));
                None
            }
            45 => {
                let p = b.prepared.as_ref().unwrap();
                let (columns, width) = self.resize_baseline.unwrap();
                let layout_width = p.layout.headers.first().map_or(0., |h| h.rect.w + 12.);
                if b.last_width <= width + 150.
                    || p.layout.columns.len() <= columns
                    || (layout_width - b.last_width).abs() > 1.
                    || (ctx.content_rect().width() - b.last_width).abs() > 40.
                {
                    Some("window growth did not reflow the full available list width")
                } else {
                    None
                }
            }
            46 => {
                let image = b
                    .prepared
                    .as_ref()
                    .unwrap()
                    .snapshot
                    .files
                    .iter()
                    .find(|m| m.file.kind == MediaKind::Image)
                    .unwrap()
                    .clone();
                b.features.favorites.paths.insert(image.file.path);
                b.open_detail(image.id);
                b.detail.as_mut().unwrap().zoom = 3.;
                None
            }
            47 => {
                let video = b
                    .prepared
                    .as_ref()
                    .unwrap()
                    .snapshot
                    .files
                    .iter()
                    .find(|r| r.id == self.rotation_video.unwrap())
                    .unwrap()
                    .clone();
                b.rotate_video(&video, true);
                b.open_detail(video.id);
                None
            }
            48 => {
                if !b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready()) {
                    return;
                }
                self.click(b.detail_layout.play_pause.unwrap().center());
                None
            }
            49 => {
                if b.player
                    .as_ref()
                    .and_then(|p| p.lock().unwrap().text("pause"))
                    .as_deref()
                    != Some("yes")
                {
                    Some("detail play button did not pause")
                } else {
                    self.click(b.detail_layout.play_pause.unwrap().center());
                    None
                }
            }
            50 => {
                if b.player
                    .as_ref()
                    .and_then(|p| p.lock().unwrap().text("pause"))
                    .as_deref()
                    != Some("no")
                {
                    Some("detail play button did not resume")
                } else {
                    b.toggle_detail_pause();
                    self.slider_press(b.detail_layout.seek.unwrap().center());
                    None
                }
            }
            51 => {
                let p = b.player.as_ref().unwrap().lock().unwrap();
                if p.text("pause").as_deref() != Some("yes")
                    || (p.number("time-pos").unwrap_or(0.)
                        - p.number("duration").unwrap_or(0.) / 2.)
                        .abs()
                        > 2.
                {
                    Some("detail seek hit region or paused seek failed")
                } else {
                    drop(p);
                    let r = b.detail_layout.volume.unwrap();
                    self.slider_press(egui::pos2(r.left() + r.width() * 0.15, r.center().y));
                    None
                }
            }
            52 => {
                let p = b.player.as_ref().unwrap().lock().unwrap();
                if b.volume >= 95.
                    || (p.number("volume").unwrap_or(-100.) - f64::from(b.volume)).abs() > 1.
                {
                    Some("detail volume hit region failed")
                } else {
                    drop(p);
                    self.events.push(egui::Event::PointerGone);
                    ctx.set_zoom_factor(1.);
                    None
                }
            }
            53 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400., 660.)));
                None
            }
            54 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(820., 720.)));
                None
            }
            55 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1550., 900.)));
                None
            }
            56 => {
                if let Some(rect) = b.detail_layout.close {
                    self.click(rect.center());
                    None
                } else {
                    Some("Missing detail close button")
                }
            }
            57 => {
                if b.detail.is_some() {
                    self.finish(ctx, Some("Detail close did not return to library"));
                    return;
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400., 660.)));
                }
                None
            }
            58 | 62 | 66 => {
                let p = b.prepared.as_ref().unwrap();
                let image = p
                    .view
                    .sequence
                    .iter()
                    .find(|id| {
                        p.lookup
                            .get(id)
                            .is_some_and(|i| p.snapshot.files[*i].file.kind == MediaKind::Image)
                    })
                    .copied();
                let Some(id) = image else {
                    self.finish(ctx, Some("Navigation image fixture missing"));
                    return;
                };
                b.open_detail(id);
                let screen = ctx.content_rect();
                self.events.push(egui::Event::PointerMoved(egui::pos2(
                    screen.right() - 34.,
                    screen.center().y - 20.,
                )));
                None
            }
            59 | 63 | 67 => {
                if b.detail_layout.arrows.len() != 2 {
                    return;
                }
                if !self.captured {
                    self.navigation_baseline = b.detail_layout.arrows.clone();
                    self.capture(
                        b,
                        ctx,
                        &format!("navigation-image-{}", (self.phase - 59) / 4),
                    );
                    self.captured = true;
                    self.at = Instant::now();
                    return;
                }
                self.click(self.navigation_baseline[1].center());
                None
            }
            60 | 64 | 68 => {
                let p = b.prepared.as_ref().unwrap();
                let is_video = b
                    .detail
                    .as_ref()
                    .and_then(|d| p.lookup.get(&d.id))
                    .is_some_and(|i| p.snapshot.files[*i].file.kind == MediaKind::Video);
                if !is_video {
                    self.finish(ctx, Some("Same-position next click did not open video"));
                    return;
                }
                if !b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready()) {
                    return;
                }
                if b.detail_layout.arrows.len() != 2
                    || b.detail_layout
                        .arrows
                        .iter()
                        .zip(&self.navigation_baseline)
                        .any(|(a, b)| {
                            (a.min - b.min).length() > 0.25 || (a.size() - b.size()).length() > 0.25
                        })
                {
                    self.finish(ctx, Some("Image/video navigation targets moved"));
                    return;
                }
                if !self.captured {
                    self.capture(
                        b,
                        ctx,
                        &format!("navigation-video-{}", (self.phase - 60) / 4),
                    );
                    self.captured = true;
                    self.at = Instant::now();
                    return;
                }
                self.click(self.navigation_baseline[1].center());
                None
            }
            61 | 65 | 69 => {
                let p = b.prepared.as_ref().unwrap();
                let is_image = b
                    .detail
                    .as_ref()
                    .and_then(|d| p.lookup.get(&d.id))
                    .is_some_and(|i| p.snapshot.files[*i].file.kind == MediaKind::Image);
                if !is_image {
                    self.finish(
                        ctx,
                        Some("Same-position next click did not return to image"),
                    );
                    return;
                }
                if self.phase == 69 {
                    b.close_detail();
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400., 660.)));
                    self.advance();
                    return;
                }
                b.close_detail();
                let size = if self.phase == 61 {
                    egui::vec2(820., 720.)
                } else {
                    egui::vec2(1550., 900.)
                };
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                None
            }
            70 => {
                let Some(button) = b.sidebar_button_rect else {
                    return;
                };
                self.sidebar_list_width = b.last_width;
                self.events.push(egui::Event::PointerMoved(button.center()));
                None
            }
            71 | 72 => {
                let (Some(button), Some(overlay)) = (b.sidebar_button_rect, b.sidebar_overlay_rect)
                else {
                    return;
                };
                if !b.sidebar_open
                    || b.pinned
                    || (b.last_width - self.sidebar_list_width).abs() > 0.25
                    || overlay.top() < button.bottom()
                    || !ctx.content_rect().contains_rect(overlay)
                {
                    self.finish(
                        ctx,
                        Some("Narrow hover overlay reflowed list or exceeded viewport"),
                    );
                    return;
                }
                if !self.captured {
                    self.capture(
                        b,
                        ctx,
                        if self.phase == 71 {
                            "sidebar-hover-narrow"
                        } else {
                            "sidebar-inside-narrow"
                        },
                    );
                    self.captured = true;
                    self.at = Instant::now();
                    return;
                }
                self.events
                    .push(egui::Event::PointerMoved(if self.phase == 71 {
                        overlay.center()
                    } else {
                        egui::pos2(
                            ctx.content_rect().right() - 30.,
                            ctx.content_rect().center().y,
                        )
                    }));
                None
            }
            73 => {
                if b.sidebar_open || b.sidebar_overlay_rect.is_some() {
                    return;
                }
                self.events.push(egui::Event::PointerMoved(egui::pos2(
                    ctx.content_rect().left() + 2.,
                    ctx.content_rect().center().y,
                )));
                None
            }
            74 => {
                if !b.sidebar_open || b.sidebar_overlay_rect.is_none() {
                    if self.captured {
                        self.finish(ctx, Some("Legacy edge hover failed to stay open"));
                    }
                    return;
                }
                if b.outside_since.is_some() {
                    self.finish(ctx, Some("Legacy edge incorrectly counted as outside"));
                    return;
                }
                if !self.captured {
                    self.capture(b, ctx, "sidebar-hover-edge");
                    self.captured = true;
                    self.at = Instant::now();
                    return;
                }
                if self.at.elapsed() < Duration::from_millis(650) {
                    return;
                }
                self.events.push(egui::Event::PointerMoved(egui::pos2(
                    ctx.content_rect().right() - 30.,
                    ctx.content_rect().center().y,
                )));
                None
            }
            75 => {
                if b.sidebar_open || b.sidebar_overlay_rect.is_some() {
                    return;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(820., 720.)));
                None
            }
            76 => {
                let Some(r) = self.window_close else {
                    return;
                };
                self.events.push(egui::Event::PointerMoved(r.center()));
                None
            }
            77 => {
                let Some(r) = self.window_maximize else {
                    return;
                };
                self.events.push(egui::Event::PointerMoved(r.center()));
                None
            }
            78 => {
                let Some(r) = self.window_minimize else {
                    return;
                };
                self.events.push(egui::Event::PointerMoved(r.center()));
                None
            }
            79 => {
                self.events.push(egui::Event::PointerGone);
                b.workspace.appearance.compact = true;
                modern_ui::apply_preferences(ctx, &b.workspace.appearance);
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                None
            }
            80 => {
                ctx.set_zoom_factor(2. / ctx.native_pixels_per_point().unwrap_or(1.));
                None
            }
            81 => {
                ctx.set_zoom_factor(1. / ctx.native_pixels_per_point().unwrap_or(1.));
                None
            }
            82 => {
                self.finish(ctx, None);
                return;
            }

            _ => None,
        };
        if let Some(e) = fail {
            self.finish(ctx, Some(e));
        } else {
            self.advance();
        }
    }
    fn tick_resize(&mut self, b: &mut Browser, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(20));
        let width = ctx.content_rect().width();
        let layout_width = b
            .prepared
            .as_ref()
            .and_then(|p| p.layout.headers.first())
            .map_or(0., |h| h.rect.w + 12.);
        let columns = b.prepared.as_ref().map_or(0, |p| p.layout.columns.len());
        let elapsed = self.started.elapsed().as_secs_f64() * 1000.;
        if self
            .rows
            .last()
            .and_then(|r| r["elapsed_ms"].as_f64())
            .is_none_or(|last| elapsed - last >= 100.)
        {
            self.rows.push(serde_json::json!({"elapsed_ms":elapsed,"phase":self.phase,
                "viewport_width":width,"list_width":b.last_width,"layout_width":layout_width,
                "columns":columns,"revision":b.revision,"prepared_revision":b.prepared.as_ref().map(|p|p.revision),
                "prepare_dirty":b.prepare_dirty,"textures":b.textures.len(),"visible_cards":b.list_regions.len(),
                "media":b.snapshot.as_ref().map_or(0,|s|s.files.len()),"scan_busy":b.scan_busy,
                "maximized":ctx.input(|i|i.viewport().maximized),"pixels_per_point":ctx.pixels_per_point()}));
        }
        if self.started.elapsed() > Duration::from_secs(60) {
            self.finish(ctx, Some("resize probe timed out before ready geometry"));
            return;
        }
        match self.phase {
            0 => {
                ctx.set_pixels_per_point(1.);
                b.target_width = 240.;
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                self.advance();
            }
            1 if self.at.elapsed() > Duration::from_millis(150) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(820., 720.)));
                self.advance();
            }
            2 if (width - 820.).abs() < 40.
                && (layout_width - b.last_width).abs() < 1.
                && columns > 0
                && !b.textures.is_empty()
                && !b.list_regions.is_empty() =>
            {
                self.resize_baseline = Some((columns, b.last_width));
                self.advance();
            }
            3 if self.at.elapsed() > Duration::from_secs(2) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
                self.advance();
            }
            4 if self.at.elapsed() > Duration::from_secs(3) => {
                let (old_columns, old_width) = self.resize_baseline.unwrap();
                let maximized = ctx.input(|i| i.viewport().maximized) == Some(true);
                let reflowed = maximized
                    && width > old_width + 150.
                    && columns > old_columns
                    && (layout_width - b.last_width).abs() < 1.
                    && (width - b.last_width).abs() < 40.;
                self.finish(
                    ctx,
                    (!reflowed).then_some("OS maximize did not reflow full list width within3s"),
                );
            }
            _ => {}
        }
    }
    fn finish(&mut self, ctx: &egui::Context, error: Option<&str>) {
        let report = serde_json::json!({"checks_pass":error.is_none(),"error":error,"phase":self.phase,"rows":self.rows,"scope":"Native egui controlled keyboard/pointer events and actual rendered FBO; not physical input or OS DPI verification"});
        let _ = std::fs::write(&self.output, serde_json::to_vec_pretty(&report).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
