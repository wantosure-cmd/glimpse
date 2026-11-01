//! Playback intent, separate from asynchronous decoder observations and GUI state.
use crate::domain::MediaId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeekRequest {
    pub load_id: u64,
    pub seek_id: u64,
    pub target: f64,
}

#[derive(Default, Debug)]
pub struct SeekQueue {
    media: Option<MediaId>,
    pub load_id: u64,
    sequence: u64,
    pending: Option<f64>,
    dirty: bool,
    last_sent_ms: Option<u64>,
}

impl SeekQueue {
    /// A new decoder load is distinct even when the catalog media ID is reused.
    pub fn begin_load(&mut self, media: MediaId) {
        self.media = None;
        self.bind(Some(media));
    }
    /// The same source moving between preview and detail keeps its pending intent.
    pub fn bind(&mut self, media: Option<MediaId>) {
        if self.media != media {
            self.media = media;
            self.load_id = self.load_id.wrapping_add(1);
            self.pending = None;
            self.dirty = false;
            self.last_sent_ms = None;
        }
    }
    pub fn target(&self) -> Option<f64> {
        self.pending
    }
    pub fn relative(
        &mut self,
        delta: f64,
        observed: f64,
        duration: Option<f64>,
        seekable: bool,
    ) -> Result<f64, &'static str> {
        let base = self.pending.unwrap_or(observed);
        self.absolute(base + delta, duration, seekable)
    }
    pub fn absolute(
        &mut self,
        target: f64,
        duration: Option<f64>,
        seekable: bool,
    ) -> Result<f64, &'static str> {
        let duration = duration
            .filter(|d| d.is_finite() && *d > 0.)
            .ok_or(crate::ui_text!(msg_678f535336b7))?;
        if !seekable || self.media.is_none() {
            return Err(crate::ui_text!(msg_147b721c0afb));
        }
        if !target.is_finite() {
            return Err(crate::ui_text!(msg_6d265760d273));
        }
        let target = target.clamp(0., duration);
        self.sequence = self.sequence.wrapping_add(1);
        self.pending = Some(target);
        self.dirty = true;
        Ok(target)
    }
    pub fn due(&mut self, now_ms: u64) -> Option<SeekRequest> {
        if !self.dirty
            || self
                .last_sent_ms
                .is_some_and(|last| now_ms.saturating_sub(last) < 80)
        {
            return None;
        }
        self.dirty = false;
        self.last_sent_ms = Some(now_ms);
        Some(SeekRequest {
            load_id: self.load_id,
            seek_id: self.sequence,
            target: self.pending?,
        })
    }
    /// A stale callback cannot acknowledge a newer request or a different source.
    pub fn acknowledge(&mut self, request: SeekRequest, observed: f64) -> bool {
        if request.load_id == self.load_id
            && request.seek_id == self.sequence
            && !self.dirty
            && observed.is_finite()
            && (observed - request.target).abs() <= 0.5
        {
            self.pending = None;
            return true;
        }
        false
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn five_rapid_seeks_accumulate_against_intent_not_late_observation() {
        let mut q = SeekQueue::default();
        q.bind(Some(1));
        for _ in 0..5 {
            q.relative(30., 10., Some(500.), true).unwrap();
        }
        assert_eq!(q.due(0).unwrap().target, 160.);
    }
    #[test]
    fn throttle_retains_latest_target_and_rejects_old_completion() {
        let mut q = SeekQueue::default();
        q.bind(Some(1));
        q.relative(30., 10., Some(500.), true).unwrap();
        let first = q.due(0).unwrap();
        q.relative(30., 10., Some(500.), true).unwrap();
        assert!(q.due(79).is_none());
        assert!(!q.acknowledge(first, 40.));
        let second = q.due(80).unwrap();
        assert_eq!(second.target, 70.);
        assert!(!q.acknowledge(first, 40.));
        assert!(q.acknowledge(second, 70.));
        assert!(q.target().is_none());
    }
    #[test]
    fn source_switch_and_unknown_duration_never_use_old_target() {
        let mut q = SeekQueue::default();
        q.bind(Some(1));
        q.relative(30., 10., Some(50.), true).unwrap();
        let old = q.due(0).unwrap();
        q.bind(Some(2));
        assert!(!q.acknowledge(old, 40.));
        assert!(q.relative(5., 0., None, true).is_err());
        assert!(q.target().is_none());
        assert_eq!(q.relative(-30., 0., Some(50.), true).unwrap(), 0.);
        assert_eq!(q.relative(300., 0., Some(50.), true).unwrap(), 50.);
        assert!(q.relative(5., 0., Some(50.), false).is_err());
    }
    #[test]
    fn same_source_transition_preserves_pending_but_cancel_does_not_send() {
        let mut q = SeekQueue::default();
        q.bind(Some(1));
        q.relative(5., 10., Some(50.), true).unwrap();
        q.bind(Some(1));
        assert_eq!(q.target(), Some(15.));
        q.cancel();
        assert!(q.due(0).is_none());
    }
    #[test]
    fn reloading_same_catalog_id_invalidates_prior_load_and_seek() {
        let mut q = SeekQueue::default();
        q.begin_load(7);
        q.relative(5., 10., Some(60.), true).unwrap();
        let old = q.due(0).unwrap();
        q.begin_load(7);
        assert!(q.load_id > old.load_id);
        assert!(q.target().is_none());
        q.relative(5., 10., Some(60.), true).unwrap();
        let current = q.due(100).unwrap();
        assert!(!q.acknowledge(old, 15.));
        assert!(q.acknowledge(current, 15.));
    }
}
