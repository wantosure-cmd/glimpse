//! Normal-window geometry in native logical (96-DPI) points,independent of UI zoom.
pub const MINIMUM: [f32; 2] = [320., 180.];
pub const DEFAULT_SIZE: [f32; 2] = [1100., 760.];

#[derive(Default)]
pub struct WindowRecovery {
    last_normal: Option<[f32; 2]>,
    last_request_ms: Option<u64>,
    minimum_sent: Option<[f32; 2]>,
    scale: Option<[f32; 2]>,
}
pub struct Decision {
    pub minimum: Option<[f32; 2]>,
    pub restore: Option<[f32; 2]>,
    pub retry: bool,
}
fn available(monitor: Option<[f32; 2]>) -> [f32; 2] {
    monitor
        .filter(|m| m.iter().all(|n| n.is_finite() && *n > 0.))
        .map(|m| [(m[0] - 24.).max(1.), (m[1] - 64.).max(1.)])
        .unwrap_or([f32::MAX; 2])
}
impl WindowRecovery {
    /// Wait one input frame after scale changes before trusting viewport geometry.
    pub fn update_scale(&mut self, native: f32, zoom: f32) -> bool {
        let scale = [native, zoom];
        if scale.iter().all(|n| n.is_finite() && *n > 0.) && self.scale != Some(scale) {
            // UI-only zoom does not change the physical native minimum.
            // Reissuing it would implicitly resize and cancel a user resize.
            if self.scale.is_none_or(|old| old[0] != native) {
                self.minimum_sent = None;
            }
            self.scale = Some(scale);
            return true;
        }
        false
    }
    pub fn observe(
        &mut self,
        size: Option<[f32; 2]>,
        monitor: Option<[f32; 2]>,
        minimized: bool,
        maximized: bool,
        fullscreen: bool,
        now_ms: u64,
    ) -> Decision {
        let mut result = Decision {
            minimum: None,
            restore: None,
            retry: false,
        };
        // winit Windows set_min_inner_size re-requests current inner size.
        // Do not send even a minimum constraint while minimized/maximized:
        // those sizes must never become the normal restore bounds.
        if fullscreen || minimized || maximized {
            return result;
        }
        let available = available(monitor);
        let minimum = [MINIMUM[0].min(available[0]), MINIMUM[1].min(available[1])];
        if self.minimum_sent != Some(minimum) {
            self.minimum_sent = Some(minimum);
            result.minimum = Some(minimum);
        }
        // Minimized geometry is an OS placeholder,not a normal-window size.
        // Never unminimize or replace a valid maximized/fullscreen window here.
        let Some(size) = size.filter(|s| s.iter().all(|n| n.is_finite() && *n > 0.)) else {
            return result;
        };
        if size[0] + 1. >= minimum[0] && size[1] + 1. >= minimum[1] {
            self.last_normal = Some(size);
            self.last_request_ms = None;
            return result;
        }
        result.retry = true;
        if self
            .last_request_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < 250)
        {
            return result;
        }
        self.last_request_ms = Some(now_ms);
        let wanted = self.last_normal.unwrap_or(DEFAULT_SIZE);
        result.restore = Some([
            wanted[0].clamp(minimum[0], available[0]),
            wanted[1].clamp(minimum[1], available[1]),
        ]);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tiny_creation_and_damaged_restore_use_default_or_last_normal() {
        let mut state = WindowRecovery::default();
        assert_eq!(
            state
                .observe(Some([210., 34.]), None, false, false, false, 0)
                .restore,
            Some(DEFAULT_SIZE)
        );
        assert!(state
            .observe(Some([820., 600.]), None, false, false, false, 10)
            .restore
            .is_none());
        assert_eq!(
            state
                .observe(Some([210., 34.]), None, false, false, false, 20)
                .restore,
            Some([820., 600.])
        );
        assert!(state
            .observe(Some([210., 34.]), None, false, false, false, 21)
            .restore
            .is_none());
        assert_eq!(
            state
                .observe(Some([210., 34.]), None, false, false, false, 270)
                .restore,
            Some([820., 600.])
        );
    }
    #[test]
    fn minimized_maximized_fullscreen_and_unavailable_sizes_never_poison_normal() {
        let mut state = WindowRecovery::default();
        state.observe(Some([820., 600.]), None, false, false, false, 0);
        for (size, mini, max, full) in [
            (Some([157., 25.]), true, false, false),
            (Some([1920., 1080.]), false, true, false),
            (Some([1920., 1080.]), false, false, true),
            (None, false, false, false),
            (Some([0., 0.]), false, false, false),
            (Some([f32::NAN, 20.]), false, false, false),
        ] {
            assert!(state
                .observe(size, None, mini, max, full, 100)
                .restore
                .is_none());
        }
        assert_eq!(
            state
                .observe(Some([210., 34.]), None, false, false, false, 200)
                .restore,
            Some([820., 600.])
        );
    }
    #[test]
    fn small_monitor_bounds_and_ordinary_resizes_are_preserved() {
        let mut state = WindowRecovery::default();
        let first = state.observe(
            Some([210., 34.]),
            Some([400., 300.]),
            false,
            false,
            false,
            0,
        );
        assert_eq!(first.minimum, Some(MINIMUM));
        assert_eq!(first.restore, Some([376., 236.]));
        assert!(state
            .observe(
                Some([376., 236.]),
                Some([400., 300.]),
                false,
                false,
                false,
                1
            )
            .restore
            .is_none());
        for size in [
            [410., 360.],
            [480., 320.],
            [640., 360.],
            [820., 720.],
            [1550., 900.],
        ] {
            assert!(state
                .observe(Some(size), None, false, false, false, 2)
                .restore
                .is_none());
        }
        assert_eq!(
            state
                .observe(
                    Some([210., 34.]),
                    Some([1280., 720.]),
                    false,
                    false,
                    false,
                    3
                )
                .restore,
            Some([1256., 656.])
        );
    }
    #[test]
    fn scale_change_resends_constraints_but_never_resizes_valid_or_hidden_windows() {
        let mut state = WindowRecovery::default();
        state.update_scale(1.5, 1.);
        let a = state.observe(Some([820., 600.]), None, false, false, false, 0);
        assert_eq!(a.minimum, Some(MINIMUM));
        assert!(a.restore.is_none());
        assert!(state
            .observe(Some([820., 600.]), None, false, false, false, 1)
            .minimum
            .is_none());
        state.update_scale(2., 2.);
        for (mini, max, full) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let a = state.observe(Some([157., 25.]), None, mini, max, full, 2);
            assert!(a.minimum.is_none() && a.restore.is_none());
        }
        let a = state.observe(Some([820., 600.]), None, false, false, false, 3);
        assert_eq!(a.minimum, Some(MINIMUM));
        assert!(a.restore.is_none());
    }
    #[test]
    fn ui_zoom_waits_for_fresh_geometry_and_keeps_native_minimum() {
        let mut state = WindowRecovery::default();
        assert!(state.update_scale(2., 1.));
        state.observe(Some([820., 600.]), None, false, false, false, 0);
        assert!(state.update_scale(2., 0.5));
        assert!(!state.update_scale(2., 0.5));
        let decision = state.observe(Some([410., 360.]), None, false, false, false, 1);
        assert!(decision.minimum.is_none() && decision.restore.is_none());
        let mut small = WindowRecovery::default();
        let decision = small.observe(
            Some([100., 20.]),
            Some([200., 200.]),
            false,
            false,
            false,
            0,
        );
        assert_eq!(decision.minimum, Some([176., 136.]));
        assert_eq!(decision.restore, Some([176., 136.]));
    }
}
