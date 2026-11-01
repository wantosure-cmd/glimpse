//! User-facing load problems and bounded visible geometry wait tracking.
use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum EmptyListState { Scanning, Failed, Empty, Filtered }

pub(super) fn empty_list_state(finished: bool, failed: bool, source_empty: bool, view_empty: bool) -> Option<EmptyListState> {
    if !view_empty { return None; }
    Some(if !finished { EmptyListState::Scanning }
        else if failed { EmptyListState::Failed }
        else if source_empty { EmptyListState::Empty }
        else { EmptyListState::Filtered })
}

pub(super) fn paint_empty_list(ui: &egui::Ui, rect: Rect, state: EmptyListState) {
    if rect.height() < 90. { return; }
    let (title, hint) = match state {
        EmptyListState::Scanning => (media_browser::ui_text!(msg_06defd29f465), ""),
        EmptyListState::Failed => (media_browser::ui_text!(empty_scan_failed_title), media_browser::ui_text!(empty_scan_failed_hint)),
        EmptyListState::Empty => (media_browser::ui_text!(empty_folder_title), media_browser::ui_text!(empty_folder_hint)),
        EmptyListState::Filtered => (media_browser::ui_text!(empty_filtered_title), media_browser::ui_text!(empty_filtered_hint)),
    };
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.text(rect.center() - egui::vec2(0., 14.), egui::Align2::CENTER_CENTER, title,
        egui::FontId::proportional(18.), modern_ui::TEXT);
    painter.text(rect.center() + egui::vec2(0., 16.), egui::Align2::CENTER_CENTER, hint,
        egui::FontId::proportional(13.), modern_ui::MUTED);
}

#[derive(Default)]
pub(super) struct LoadingState {
    waits: HashMap<(u64, MediaId), Instant>,
    seen: HashSet<(u64, MediaId)>,
    pub issues_button: Option<Rect>,
    pub retry_button: Option<Rect>,
    pub scan_error: Option<String>,
}
impl LoadingState {
    pub fn begin_frame(&mut self) {
        self.seen.clear();
    }
    pub fn slow(&mut self, generation: u64, id: MediaId) -> bool {
        let key = (generation, id);
        if self.waits.len() >= 128 && !self.waits.contains_key(&key) {
            return true;
        }
        self.seen.insert(key);
        self.waits.entry(key).or_insert_with(Instant::now).elapsed() >= Duration::from_secs(3)
    }
    pub fn end_frame(&mut self) {
        self.waits.retain(|key, _| self.seen.contains(key));
    }
}
impl Browser {
    pub(super) fn loading_issues_menu(&mut self, ui: &mut egui::Ui) {
        self.loading.issues_button = None;
        self.loading.retry_button = None;
        let scan_errors = self.snapshot.as_ref().is_some_and(|s| !s.errors.is_empty());
        let failed_open = !self.scan_busy && self.watch_root.is_some() && self.snapshot.is_none();
        if self.failures.is_empty()
            && !scan_errors
            && !failed_open
            && self.watch_warning.is_none()
            && self.loading.scan_error.is_none()
        {
            return;
        }
        let mut retry = false;
        let menu = ui.menu_button(media_browser::ui_text!(loading_issues), |ui| {
            ui.set_max_width(380.);
            if failed_open {
                ui.label(&self.status);
            }
            if let Some(error) = &self.loading.scan_error {
                ui.label(error);
            }
            if let Some(warning) = &self.watch_warning {
                ui.label(warning);
            }
            let mut file_error_shown = false;
            if let Some(snapshot) = &self.snapshot {
                for error in snapshot.errors.iter().take(8) {
                    ui.label(error);
                }
                for record in snapshot.files.iter().filter(|r| r.error.is_some()).take(8) {
                    file_error_shown = true;
                    ui.label(
                        record
                            .file
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    );
                    ui.label(record.error.as_deref().unwrap_or_default());
                    ui.separator();
                }
            }
            if !self.failures.is_empty() {
                ui.label(media_browser::ui_format!(
                    msg_55e264f79959,
                    self.failures.len()
                ));
                if !file_error_shown {
                    if let Some(error) = self.failures.values().next() {
                        ui.label(error);
                    }
                }
            }
            ui.label(media_browser::ui_text!(loading_retry_help));
            let response = ui.button(media_browser::ui_text!(loading_retry));
            self.loading.retry_button = Some(response.rect);
            if response.clicked() {
                retry = true;
                ui.close();
            }
        });
        self.loading.issues_button = Some(menu.response.rect);
        if retry {
            self.refresh_in_place();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_folder_is_distinct_from_scan_errors_filters_and_collapsed_media() {
        assert_eq!(empty_list_state(true, false, true, true), Some(EmptyListState::Empty));
        assert_eq!(empty_list_state(false, false, true, true), Some(EmptyListState::Scanning));
        assert_eq!(empty_list_state(true, true, true, true), Some(EmptyListState::Failed));
        assert_eq!(empty_list_state(true, false, false, true), Some(EmptyListState::Filtered));
        // Existing media stay in the browse sequence even if every folder is collapsed.
        assert_eq!(empty_list_state(true, false, false, false), None);
        assert_eq!(empty_list_state(false, true, true, true), Some(EmptyListState::Scanning));
    }
    #[test]
    fn geometry_wait_tracks_visible_generation_and_retires_offscreen_entries() {
        let mut waits = LoadingState::default();
        waits.begin_frame();
        assert!(!waits.slow(1, 10));
        waits
            .waits
            .insert((1, 10), Instant::now() - Duration::from_secs(4));
        assert!(waits.slow(1, 10));
        assert!(!waits.slow(2, 10));
        waits.end_frame();
        waits.begin_frame();
        assert!(!waits.slow(2, 10));
        waits.end_frame();
        assert!(!waits.waits.contains_key(&(1, 10)));
        waits.begin_frame();
        waits.end_frame();
        assert!(waits.waits.is_empty());
    }
    #[test]
    fn geometry_wait_storage_is_bounded_during_a_large_visible_batch() {
        let mut waits = LoadingState::default();
        waits.begin_frame();
        for id in 0..1000 {
            waits.slow(1, id);
        }
        assert_eq!(waits.waits.len(), 128);
        assert_eq!(waits.seen.len(), 128);
        waits.end_frame();
        waits.begin_frame();
        waits.end_frame();
        assert!(waits.waits.is_empty());
    }
}
