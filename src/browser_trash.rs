//! UI-side recycle coordinator. Source release gates precede every Shell submission.
use super::*;
use media_browser::trash::{TrashEvent, TrashRequest};
pub(super) struct PendingTrash {
    prepared: Arc<Prepared>,
    requests: Vec<TrashRequest>,
    ids: HashSet<MediaId>,
    created: Instant,
    deadline: Instant,
}
pub(super) struct PermanentPrompt {
    prepared: Arc<Prepared>,
    requests: Vec<TrashRequest>,
    ids: HashSet<MediaId>,
    acknowledged: bool,
}
pub(super) struct TrashOperation {
    permanent: bool,
    fallback: Vec<TrashRequest>,
    prepared: Arc<Prepared>,
    batch: u64,
    requests: Vec<TrashRequest>,
    ids: HashSet<MediaId>,
    directories: Vec<PathBuf>,
    started: Instant,
    pub(super) submitted: bool,
    sequence: Vec<MediaId>,
    detail: Option<MediaId>,
    root: PathBuf,
    success: HashSet<MediaId>,
    results: Vec<serde_json::Value>,
    release_ms: Option<f64>,
    undo_wait_ms: Option<f64>,
    resume_detail: Option<(MediaId,bool)>,
}
impl Browser {
    // Keep the release barrier until submission, then isolate only the batch's sources.
    pub(super) fn trash_releasing(&self) -> bool {
        self.trash_operation.as_ref().is_some_and(|op| !op.submitted)
    }
    pub(super) fn trash_blocks_media(&self, id: MediaId) -> bool {
        let Some(op) = &self.trash_operation else { return false; };
        if op.ids.contains(&id) { return true; }
        let path = self.prepared.as_ref().and_then(|p| {
            p.lookup.get(&id).map(|i| &p.snapshot.files[*i].file.path)
        });
        path.is_some_and(|path| op.directories.iter().any(|root| path.starts_with(root)))
    }
    pub(super) fn trash_diagnostic_for_smoke(&self) -> serde_json::Value {
        serde_json::json!({
            "pending":self.pending_trash.as_ref().map(|p|serde_json::json!({"requests":p.requests.len(),"remaining_ms":p.deadline.saturating_duration_since(Instant::now()).as_millis(),"age_ms":p.created.elapsed().as_millis()})),
            "operation":self.trash_operation.as_ref().map(|op|serde_json::json!({
                "batch":op.batch,"submitted":op.submitted,"requests":op.requests.len(),
                "results":op.results.len(),"age_ms":op.started.elapsed().as_millis(),
                "release_ms":op.release_ms,"thumbnails_released":self.thumbnails.sources_released(&op.ids),
                "prefetch_idle":self.video_prefetch.idle()
            })),
            "permanent_prompt":self.permanent_prompt.is_some(),"failures":self.trash_failures.len(),
            "message":self.trash_message,"last":self.last_trash
        })
    }
    pub(super) fn begin_trash(&mut self) {
        if self.trash_operation.is_some()
            || self.pending_trash.is_some()
            || self.pending_folders.is_some()
            || self.folder_prepare.is_some()
            || self.features.cleanup.is_some()
            || self.rotation.is_some()
            || self.permanent_prompt.is_some()
        {
            return;
        }
        let Some(prepared) = self.prepared.clone() else {
            return;
        };
        let ids: HashSet<_> = if let Some(v) = &self.detail {
            HashSet::from([v.id])
        } else {
            self.selection.clone()
        };
        let requests: Vec<_> = prepared
            .view
            .sequence
            .iter()
            .filter(|id| ids.contains(id))
            .filter_map(|id| {
                prepared.lookup.get(id).map(|index| {
                    let record = &prepared.snapshot.files[*index];
                    TrashRequest {
                        empty_only: false,
                        directory: false,
                        id: *id,
                        root: prepared.snapshot.root.clone(),
                        path: record.file.path.clone(),
                        fingerprint: record.file.fingerprint.clone(),
                    }
                })
            })
            .collect();
        if requests.is_empty() {
            return;
        }
        let created=Instant::now();
        self.pending_trash=Some(PendingTrash {prepared,requests,ids,created,deadline:created+Duration::from_secs(10)});
        self.ctx.request_repaint();
    }
    pub(super) fn start_trash_requests(
        &mut self,
        prepared: Arc<Prepared>,
        requests: Vec<TrashRequest>,
        mut ids: HashSet<MediaId>,
    ) {
        ids.extend(requests.iter().map(|r| r.id));
        let current=self.detail.as_ref().map(|v|v.id);
        let detail=current.filter(|id|ids.contains(id));
        let resume_detail=current.filter(|_|detail.is_none()).map(|id| (id,
            self.player.as_ref().is_some_and(|p|p.lock().unwrap().paused_intent())));
        let directories = requests.iter().filter(|r| r.directory).map(|r| r.path.clone()).collect();
        self.trash_counter = self.trash_counter.wrapping_add(1).max(1);
        self.thumbnails.block_sources(&ids);
        self.video_prefetch.cancel();
        self.animation.clear();
        self.list_animations.clear();
        self.remember_position();
        self.preview_active = None;
        self.hover_candidate = None;
        self.preview_idle = None;
        self.seeks.bind(None);
        self.seek_flight = None;
        self.resume_pending = None;
        self.resume_starting = false;
        self.resume_pause_override = None;
        self.restore_flight = false;
        self.load_started = None;
        self.player_media = None;
        self.player_version = None;
        self.video_region = None;
        self.image_region = None;
        self.trash_failures.clear();
        if let Some(player) = &self.player {
            let mut p = player.lock().unwrap();
            let _ = p.set("mute", "yes");
            let _ = p.set("pause", "yes");
            if let Err(error) = p.unload() {
                self.thumbnails.unblock_sources(&ids);
                self.trash_message = Some(error);
                return;
            }
        }
        self.trash_operation = Some(TrashOperation {
            permanent: false,
            fallback: Vec::new(),
            prepared: prepared.clone(),
            batch: self.trash_counter,
            requests,
            ids,
            directories,
            started: Instant::now(),
            submitted: false,
            sequence: prepared.view.sequence.clone(),
            detail,
            root: prepared.snapshot.root.clone(),
            success: HashSet::new(),
            results: Vec::new(),
            release_ms: None,
            undo_wait_ms: None,
            resume_detail,
        });
        self.ctx.request_repaint();
    }
    fn abort_trash_release(&mut self, message: String) {
        if let Some(op) = self.trash_operation.take() {
            self.thumbnails.unblock_sources(&op.ids);
            self.trash_message = Some(media_browser::ui_format!(
                msg_5da40cfb0e14,
                message = message
            ));
            self.last_trash = Some(
                serde_json::json!({"batch":op.batch,"submitted":false,"error":message,"source_count":op.ids.len()}),
            );
            if let Some((id,paused))=op.resume_detail {self.restore_trash_viewer(id,paused);}
            else if let Some(id) = op.detail.filter(|id| self.detail.as_ref().is_some_and(|v| v.id == *id)) {
                self.open_detail(id);
            }
        }
    }
    fn restore_trash_viewer(&mut self,id:MediaId,paused:bool) {
        if let Some(viewer)=self.detail.clone().filter(|v|v.id==id) {
            self.open_detail(id);
            if self.detail.as_ref().is_some_and(|v|v.id==id) {
                self.detail=Some(viewer);
                if self.player_media==Some(id) {
                    if self.resume_starting {self.resume_pause_override=Some(paused);}
                    else if let Some(player)=&self.player {let _=player.lock().unwrap().set("pause",if paused {"yes"} else {"no"});}
                }
            }
        }
    }
    pub(super) fn trash_tick(&mut self, ctx: &egui::Context) {
        if self.pending_trash.as_ref().is_some_and(|p|Instant::now()>=p.deadline)
            && self.trash_operation.is_none() && self.rotation.is_none() && self.permanent_prompt.is_none() {
            let pending=self.pending_trash.take().unwrap();
            let wait=pending.created.elapsed().as_secs_f64()*1000.;
            self.start_trash_requests(pending.prepared,pending.requests,pending.ids);
            if let Some(op)=self.trash_operation.as_mut(){op.undo_wait_ms=Some(wait);}
        }
        if self.pending_trash.is_some(){ctx.request_repaint_after(Duration::from_millis(100));}
        let Some(op) = self.trash_operation.as_ref() else {
            return;
        };
        if !op.submitted {
            let player = if let Some(player) = &self.player {
                player.lock().unwrap().release_complete()
            } else {
                Ok(true)
            };
            match player {
                Err(error) => {
                    self.abort_trash_release(error);
                    return;
                }
                Ok(player_released) => {
                    let thumbs_released =
                        self.thumbnails.sources_released(&op.ids) && self.video_prefetch.idle();
                    let gif_released = self.animation.released() && self.list_animations.released();
                    if player_released && thumbs_released && gif_released {
                        let op = self.trash_operation.as_mut().unwrap();
                        op.release_ms = Some(op.started.elapsed().as_secs_f64() * 1000.);
                        let submission = if op.permanent {
                            self.trash_service
                                .submit_permanent_confirmed(op.batch, op.requests.clone())
                        } else {
                            self.trash_service.submit(op.batch, op.requests.clone())
                        };
                        match submission {
                            Ok(()) => op.submitted = true,
                            Err(error) => {
                                self.abort_trash_release(error);
                                return;
                            }
                        }
                    } else if op.started.elapsed() > Duration::from_secs(20) {
                        self.abort_trash_release(media_browser::ui_format!(
                            msg_dc144acad285,
                            gif_released = gif_released,
                            player_released = player_released,
                            thumbs_released = thumbs_released
                        ));
                        return;
                    }
                }
            }
        }
        let resume=self.trash_operation.as_mut().filter(|op|op.submitted).and_then(|op|op.resume_detail.take());
        if let Some((id,paused))=resume {self.restore_trash_viewer(id,paused);}
        while let Some(event) = self.trash_service.poll() {
            match event {
                TrashEvent::Item {
                    batch,
                    request,
                    result,
                } => {
                    let Some(op) = self.trash_operation.as_mut().filter(|op| op.batch == batch)
                    else {
                        continue;
                    };
                    match result {
                        Ok(receipt) => {
                            op.success.insert(request.id);
                            self.selection.remove(&request.id);
                            self.positions.remove(&request.id);
                            self.playback_errors.remove(&request.id);
                            op.results.push(serde_json::json!({"id":request.id,"path":request.path,"success":true,"recycled_path":receipt.recycled_path,"pre_delete_flags":receipt.pre_delete_flags}));
                            if !request.directory {
                                self.library.recycled(request);
                            }
                        }
                        Err(error) => {
                            if !op.permanent && media_browser::trash::can_offer_permanent(&error) {
                                op.fallback.push(request.clone());
                            }
                            self.trash_failures
                                .push((request.path.clone(), error.clone()));
                            op.results.push(serde_json::json!({"id":request.id,"path":request.path,"success":false,"error":error}));
                        }
                    }
                }
                TrashEvent::PermanentItem {
                    batch,
                    request,
                    result,
                } => {
                    let Some(op) = self
                        .trash_operation
                        .as_mut()
                        .filter(|op| op.batch == batch && op.permanent)
                    else {
                        continue;
                    };
                    match result {
                        Ok(()) => {
                            op.success.insert(request.id);
                            self.selection.remove(&request.id);
                            self.positions.remove(&request.id);
                            self.playback_errors.remove(&request.id);
                            op.results.push(serde_json::json!({"id":request.id,"path":request.path,"success":true,"permanent":true}));
                            if !request.directory {
                                self.library.recycled(request);
                            }
                        }
                        Err(error) => {
                            self.trash_failures
                                .push((request.path.clone(), error.message.clone()));
                            op.results.push(serde_json::json!({"id":request.id,"path":request.path,"success":false,"permanent":true,"error":error.to_string(),"may_have_deleted_contents":error.may_have_deleted_contents}));
                        }
                    }
                }
                TrashEvent::Finished { batch } => {
                    if !self
                        .trash_operation
                        .as_ref()
                        .is_some_and(|op| op.batch == batch)
                    {
                        continue;
                    }
                    let op = self.trash_operation.take().unwrap();
                    self.thumbnails.unblock_sources(&op.ids);
                    if !op.fallback.is_empty() {
                        self.permanent_prompt = Some(PermanentPrompt {
                            prepared: op.prepared.clone(),
                            requests: op.fallback.clone(),
                            ids: op.ids.clone(),
                            acknowledged: false,
                        });
                    }
                    self.trash_message = Some(if op.permanent {
                        media_browser::ui_format!(
                            msg_d87ad132b6cc,
                            op.success.len(),
                            op.results.len() - op.success.len()
                        )
                    } else {
                        media_browser::ui_format!(
                            msg_cf69915ef404,
                            op.success.len(),
                            op.results.len() - op.success.len()
                        )
                    });
                    self.last_trash = Some(
                        serde_json::json!({"batch":batch,"permanent":op.permanent,"submitted":true,"release_confirmed":true,"release_ms":op.release_ms,"undo_wait_ms":op.undo_wait_ms,"success_count":op.success.len(),"failed_count":op.results.len()-op.success.len(),"items":op.results}),
                    );
                    let next = op.detail.and_then(|id| {
                        if !op.success.contains(&id) {
                            return Some(id);
                        }
                        let index = op.sequence.iter().position(|i| *i == id)?;
                        op.sequence[index + 1..]
                            .iter()
                            .chain(op.sequence[..index].iter().rev())
                            .find(|id| !op.success.contains(id))
                            .copied()
                    });
                    // A new generation cancels previously enumerated batches and rejects
                    // queued snapshots. Counts come from one repository, never decremented twice.
                    self.selected_folders.retain(|path| {
                        !op.requests.iter().any(|r| {
                            r.directory && op.success.contains(&r.id) && path.starts_with(&r.path)
                        })
                    });
                    let removed_current = op.requests.iter().find(|r| {
                        r.directory
                            && op.success.contains(&r.id)
                            && self
                                .watch_root
                                .as_ref()
                                .is_some_and(|root| root.starts_with(&r.path))
                    });
                    if let Some(request) = removed_current {
                        if let Some(parent) = request.path.parent() {
                            self.navigate(parent.to_owned(), false);
                        }
                        continue;
                    }
                    if self.snapshot.as_ref().is_some_and(|s| s.root == op.root) {
                        self.generation = self
                            .library
                            .open_view(&op.root, self.watch_source_root.as_deref());
                        self.scan_busy = true;
                        self.refresh_pending = false;
                        // Do not undo browsing/navigation performed while the worker ran.
                        if op.detail.is_some() && self.detail.as_ref().map(|v| v.id) == op.detail {
                            if let Some(id) = next {
                                self.open_detail(id);
                            } else {
                                self.close_detail();
                            }
                        }
                    }
                }
            }
        }
        if self.trash_operation.is_some() {
            ctx.request_repaint_after(Duration::from_millis(20));
        }
    }
    pub(super) fn trash_ui(&mut self, ctx: &egui::Context) {
        self.trash_undo_panel=None;self.trash_undo_button=None;
        if let Some(pending)=&self.pending_trash {
            let seconds=pending.deadline.saturating_duration_since(Instant::now()).as_secs_f32().ceil() as u32;
            let mut undo=false;
            let panel=egui::Area::new(egui::Id::unique("media-delete-undo"))
                .order(egui::Order::Tooltip).movable(false)
                .anchor(egui::Align2::RIGHT_TOP,egui::vec2(-12.,48.)).show(ctx,|ui| {
                    modern_ui::toast_frame(ui.style()).show(ui,|ui| {
                        ui.horizontal(|ui| {
                            ui.label(media_browser::ui_format!(msg_media_delete_undo,count=pending.requests.len(),seconds=seconds));
                            let button=ui.button(media_browser::ui_text!(msg_6b5819895525));
                            self.trash_undo_button=Some(button.rect);self.control_focused |= button.has_focus();
                            undo=button.clicked();
                        });
                        ui.small(media_browser::ui_text!(msg_a5ca50de174e));
                    });
                });
            self.trash_undo_panel=Some(panel.response.rect);
            self.control_regions.push(panel.response.rect);
            if undo {self.pending_trash=None;self.trash_undo_panel=None;self.trash_undo_button=None;}
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(op) = &self.trash_operation {
            egui::Area::new(egui::Id::unique("recycle-progress"))
                .order(egui::Order::Tooltip)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12., 48.))
                .interactable(false)
                .show(ctx, |ui| {
                    modern_ui::toast_frame(ui.style()).show(ui, |ui| {
                    ui.set_width(264_f32.min((ctx.content_rect().width() - 48.).max(160.)));
                    ui.spacing_mut().item_spacing.y = 5.;
                    ui.label(egui::RichText::new(if op.permanent {
                        media_browser::ui_text!(msg_b66ddfe77c44)
                    } else if op.submitted {
                        media_browser::ui_text!(msg_8761dfbbab6a)
                    } else {
                        media_browser::ui_text!(msg_f1a3d5eceef2)
                    }).size(13.).strong());
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(12.));
                        ui.label(egui::RichText::new(media_browser::ui_format!(
                            msg_992665840148,
                            op.results.len(),
                            op.requests.len()
                        )).size(11.).color(modern_ui::MUTED));
                        if op.submitted && !op.requests.is_empty() {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("{:.0}%", 100. * op.results.len() as f32 / op.requests.len() as f32)).size(11.).color(modern_ui::MUTED));
                            });
                        }
                    });
                    if op.submitted && !op.requests.is_empty() {
                        ui.add(
                            egui::ProgressBar::new(
                                op.results.len() as f32 / op.requests.len() as f32,
                            )
                            .desired_width(ui.available_width())
                            .desired_height(3.),
                        );
                    }
                    });
                });
        }
        self.permanent_confirmation_ui(ctx);
        if !self.trash_failures.is_empty()
            && self.trash_operation.is_none()
            && self.permanent_prompt.is_none()
        {
            let mut open = true;
            let permanent = self
                .last_trash
                .as_ref()
                .is_some_and(|r| r["permanent"] == true);
            egui::Window::new(if permanent {
                media_browser::ui_text!(msg_f287ca7c5511)
            } else {
                media_browser::ui_text!(msg_6ae8b8b2306a)
            })
            .id(egui::Id::unique("delete-failures"))
            .open(&mut open)
            .default_width(600.)
            .show(ctx, |ui| {
                let permanent = self
                    .last_trash
                    .as_ref()
                    .is_some_and(|r| r["permanent"] == true);
                let partial = self
                    .last_trash
                    .as_ref()
                    .and_then(|r| r["items"].as_array())
                    .is_some_and(|items| {
                        items
                            .iter()
                            .any(|item| item["may_have_deleted_contents"] == true)
                    });
                if permanent && partial {
                    ui.colored_label(modern_ui::DANGER, media_browser::ui_text!(msg_df12b53af38c));
                } else if permanent {
                    ui.label(media_browser::ui_text!(msg_db51a0bbe2f5));
                } else {
                    ui.label(media_browser::ui_text!(msg_0820bea66fea));
                }
                egui::ScrollArea::vertical().max_height(360.).show_rows(
                    ui,
                    64.,
                    self.trash_failures.len(),
                    |ui, range| {
                        for i in range {
                            let (path, error) = &self.trash_failures[i];
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(
                                        path.file_name().unwrap_or_default().to_string_lossy(),
                                    )
                                    .strong(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(display_path(path));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(error).color(modern_ui::DANGER),
                                )
                                .truncate(),
                            )
                            .on_hover_text(error);
                            ui.separator();
                        }
                    },
                );
            });
            if !open {
                self.trash_failures.clear();
            }
        }
    }
    fn permanent_confirmation_ui(&mut self, ctx: &egui::Context) {
        self.permanent_controls = None;
        let Some(mut prompt) = self.permanent_prompt.take() else {
            return;
        };
        let mut confirm = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("permanent-delete-confirmation"))
            .frame(modern_ui::dialog_frame(&ctx.style_of(egui::Theme::Dark)))
            .show(ctx, |ui| {
                ui.set_width(560.);
                ui.heading(media_browser::ui_text!(msg_c5894ceb2892));
                ui.colored_label(modern_ui::DANGER, media_browser::ui_text!(msg_698a707f0bf9));
                ui.label(media_browser::ui_text!(msg_5c5183a3fbca));
                ui.label(media_browser::ui_text!(msg_dbdbed2f733d));
                egui::ScrollArea::vertical()
                    .max_height(240.)
                    .show(ui, |ui| {
                        for request in &prompt.requests {
                            ui.label(display_path(&request.path));
                        }
                    });
                let ack = ui.checkbox(
                    &mut prompt.acknowledged,
                    media_browser::ui_text!(msg_2797f4c5429e),
                );
                ui.horizontal(|ui| {
                    cancel = ui
                        .button(media_browser::ui_text!(msg_d66fd8752973))
                        .clicked();
                    let confirm_button = ui.add_enabled(
                        prompt.acknowledged,
                        egui::Button::new(
                            egui::RichText::new(media_browser::ui_text!(msg_276139281d64))
                                .color(modern_ui::DANGER),
                        ),
                    );
                    confirm = confirm_button.clicked();
                    self.permanent_controls =
                        Some((ack.rect, confirm_button.rect, prompt.acknowledged));
                });
            });
        if cancel || modal.should_close() {
            return;
        }
        if confirm {
            self.start_trash_requests(prompt.prepared, prompt.requests, prompt.ids);
            if let Some(op) = &mut self.trash_operation {
                op.permanent = true;
            }
        } else {
            self.permanent_prompt = Some(prompt);
        }
    }
}
