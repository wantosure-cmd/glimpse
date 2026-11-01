//! One routing policy. The GUI adapter removes consumed raw events before widgets see them.
use crate::domain::MediaId;
#[derive(Default)]
pub struct KeyLatch {
    down: std::collections::HashSet<u32>,
}
impl KeyLatch {
    pub fn transition(&mut self, key: u32, pressed: bool, repeat: bool) -> bool {
        if pressed {
            self.down.insert(key) && !repeat
        } else {
            self.down.remove(&key);
            false
        }
    }
    pub fn clear(&mut self) {
        self.down.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Video(MediaId),
    Image,
    Filmstrip,
    Other,
    Control,
}
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub detail: bool,
    pub hit: Hit,
    pub blocked: bool,
    pub shift: bool,
}
#[derive(Clone, Copy, Debug)]
pub enum Input {
    Wheel { delta: f32, pixels: bool },
    Back,
    Forward,
    Previous,
    Next,
    TogglePause,
    Fullscreen,
    Escape,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Pass,
    Consume,
    Seek { id: MediaId, seconds: f64 },
    History { forward: bool },
    Zoom(f32),
    Navigate(i32),
    Previous,
    Next,
    TogglePause,
    Fullscreen,
    Escape,
}
#[derive(Default)]
pub struct InputRouter {
    target: Option<MediaId>,
    filmstrip: bool,
    pixels: f32,
    lines: f32,
}
impl InputRouter {
    pub fn update_context(&mut self, context: Context) {
        let target = if context.blocked || context.hit == Hit::Control {
            None
        } else if let Hit::Video(id) = context.hit {
            Some(id)
        } else {
            None
        };
        let filmstrip = context.detail && context.hit == Hit::Filmstrip && !context.blocked;
        if self.target != target || self.filmstrip != filmstrip {
            self.filmstrip = filmstrip;
            self.target = target;
            self.pixels = 0.;
            self.lines = 0.;
        }
    }
    pub fn route(&mut self, context: Context, input: Input) -> Action {
        self.update_context(context);
        let target = self.target;
        let keyboard = matches!(
            input,
            Input::Previous | Input::Next | Input::TogglePause | Input::Fullscreen | Input::Escape
        );
        if context.blocked || (context.hit == Hit::Control && !keyboard) {
            return Action::Pass;
        }
        match input {
            Input::Previous => {
                if context.detail {
                    Action::Previous
                } else {
                    Action::Pass
                }
            }
            Input::Next => {
                if context.detail {
                    Action::Next
                } else {
                    Action::Pass
                }
            }
            Input::TogglePause => {
                if context.detail {
                    Action::TogglePause
                } else {
                    Action::Pass
                }
            }
            Input::Fullscreen => Action::Fullscreen,
            Input::Escape => Action::Escape,
            Input::Wheel { delta, pixels } => {
                if !delta.is_finite() {
                    return Action::Consume;
                }
                if self.filmstrip {
                    let (accumulator, threshold) = if pixels {
                        (&mut self.pixels, 45.)
                    } else {
                        (&mut self.lines, 1.)
                    };
                    *accumulator += delta;
                    let steps = (*accumulator / threshold).trunc().clamp(-32., 32.) as i32;
                    *accumulator -= steps as f32 * threshold;
                    return if steps == 0 {
                        Action::Consume
                    } else {
                        Action::Navigate(-steps)
                    };
                }
                if context.shift && !context.detail {
                    self.reset();
                    return Action::Pass;
                }
                if let Some(id) = target {
                    let (accumulator, threshold) = if pixels {
                        (&mut self.pixels, 100.)
                    } else {
                        (&mut self.lines, 1.)
                    };
                    *accumulator += delta;
                    let steps = (*accumulator / threshold).trunc();
                    *accumulator -= steps * threshold;
                    if steps == 0. {
                        Action::Consume
                    } else {
                        Action::Seek {
                            id,
                            seconds: steps as f64 * 30.,
                        }
                    }
                } else if context.detail {
                    if context.hit == Hit::Image {
                        Action::Zoom(delta * if pixels { 1. } else { 60. })
                    } else {
                        Action::Consume
                    }
                } else {
                    Action::Pass
                }
            }
            Input::Back | Input::Forward => {
                let forward = matches!(input, Input::Forward);
                if let Some(id) = target {
                    Action::Seek {
                        id,
                        seconds: if forward { 5. } else { -5. },
                    }
                } else if context.detail {
                    if matches!(context.hit, Hit::Image | Hit::Filmstrip) {
                        if forward {
                            Action::Next
                        } else {
                            Action::Previous
                        }
                    } else {
                        Action::Consume
                    }
                } else {
                    Action::History { forward }
                }
            }
        }
    }
    pub fn reset(&mut self) {
        self.target = None;
        self.filmstrip = false;
        self.pixels = 0.;
        self.lines = 0.;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ctx(detail: bool, hit: Hit) -> Context {
        Context {
            detail,
            hit,
            blocked: false,
            shift: false,
        }
    }
    #[test]
    fn side_keys_never_navigate_from_details_or_seek_and_navigate_together() {
        let mut r = InputRouter::default();
        assert_eq!(
            r.route(ctx(false, Hit::Video(1)), Input::Back),
            Action::Seek {
                id: 1,
                seconds: -5.
            }
        );
        assert_eq!(
            r.route(ctx(false, Hit::Other), Input::Back),
            Action::History { forward: false }
        );
        assert_eq!(r.route(ctx(true, Hit::Other), Input::Back), Action::Consume);
        assert_eq!(r.route(ctx(true, Hit::Image), Input::Forward), Action::Next);
    }
    #[test]
    fn precision_wheel_accumulates_and_resets_when_target_changes() {
        let mut r = InputRouter::default();
        let wheel = Input::Wheel {
            delta: 60.,
            pixels: true,
        };
        assert_eq!(r.route(ctx(false, Hit::Video(1)), wheel), Action::Consume);
        assert_eq!(
            r.route(ctx(false, Hit::Video(1)), wheel),
            Action::Seek {
                id: 1,
                seconds: 30.
            }
        );
        assert_eq!(r.route(ctx(false, Hit::Video(2)), wheel), Action::Consume);
        assert_eq!(
            r.route(
                ctx(false, Hit::Video(2)),
                Input::Wheel {
                    delta: -160.,
                    pixels: true
                }
            ),
            Action::Seek {
                id: 2,
                seconds: -30.
            }
        );
    }
    #[test]
    fn shift_and_controls_preserve_their_own_wheel() {
        let mut r = InputRouter::default();
        let wheel = Input::Wheel {
            delta: 1.,
            pixels: false,
        };
        let mut c = ctx(false, Hit::Video(1));
        c.shift = true;
        assert_eq!(r.route(c, wheel), Action::Pass);
        c.shift = false;
        c.blocked = true;
        assert_eq!(r.route(c, Input::Back), Action::Pass);
        assert_eq!(r.route(ctx(true, Hit::Control), wheel), Action::Pass);
    }
    #[test]
    fn raw_backend_repeat_false_still_only_triggers_one_single_press_action() {
        let mut latch = KeyLatch::default();
        assert!(latch.transition(1, true, false));
        assert!(!latch.transition(1, true, false));
        assert!(!latch.transition(1, false, false));
        assert!(latch.transition(1, true, false));
        latch.clear();
        assert!(latch.transition(1, true, false));
        latch.clear();
        assert!(!latch.transition(1, true, true));
    }
    #[test]
    fn pointer_over_control_does_not_hide_unfocused_keyboard_shortcuts() {
        let mut r = InputRouter::default();
        let context = ctx(true, Hit::Control);
        assert_eq!(r.route(context, Input::Fullscreen), Action::Fullscreen);
        assert_eq!(r.route(context, Input::Escape), Action::Escape);
        let mut focused = context;
        focused.blocked = true;
        assert_eq!(r.route(focused, Input::Next), Action::Pass);
    }
}

#[cfg(test)]
mod filmstrip_tests {
    use super::*;
    #[test]
    fn strip_wheel_batches_navigation_and_does_not_seek_or_zoom() {
        let mut router = InputRouter::default();
        let mut c = Context {
            detail: true,
            hit: Hit::Filmstrip,
            blocked: false,
            shift: false,
        };
        assert_eq!(
            router.route(
                c,
                Input::Wheel {
                    delta: -3.,
                    pixels: false
                }
            ),
            Action::Navigate(3)
        );
        assert_eq!(
            router.route(
                c,
                Input::Wheel {
                    delta: -25.,
                    pixels: true
                }
            ),
            Action::Consume
        );
        assert_eq!(
            router.route(
                c,
                Input::Wheel {
                    delta: -25.,
                    pixels: true
                }
            ),
            Action::Navigate(1)
        );
        c.hit = Hit::Control;
        assert_eq!(
            router.route(
                c,
                Input::Wheel {
                    delta: -100.,
                    pixels: true
                }
            ),
            Action::Pass
        );
        c.hit = Hit::Filmstrip;
        assert_eq!(
            router.route(
                c,
                Input::Wheel {
                    delta: -25.,
                    pixels: true
                }
            ),
            Action::Consume
        );
        c.blocked = true;
        assert_eq!(router.route(c, Input::Forward), Action::Pass);
    }
}
