//! Chrome motion: toast slide-in, popup fade-in, tab-switch sweep and the
//! Working spinner.
//!
//! `observe` compares this frame's chrome against the last one and queues an
//! effect for whatever just appeared. Every effect is clipped to a chrome rect
//! taken from the hit map, never a pane rect, so terminal output is never
//! animated. Frame 0 of each effect is today's glyph and colour, and with
//! motion off nothing is queued, the spinner stays on frame 0 and `deadline`
//! asks for no extra wake-ups.

use std::mem::Discriminant;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use tachyonfx::{fx, EffectManager, Interpolation, Motion};

use super::{ClientShellOverlay, Palette, ShellHitMap};

const FRAME: Duration = Duration::from_millis(16);
const SPIN_STEP: Duration = Duration::from_millis(100);
const SPIN_FRAMES: u8 = crate::ui::SPINNER.len() as u8;

// `EffectManager<K>: Default` needs `K: Default`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum FxKey {
    #[default]
    Toast,
    Overlay,
    Tab,
}

/// What `observe` needs from one composed frame.
pub(super) struct FxFrame<'a> {
    pub(super) hits: &'a ShellHitMap,
    pub(super) overlay: Option<&'a ClientShellOverlay>,
    /// Identity of the visible toast (its deadline is unique per toast).
    pub(super) toast: Option<Instant>,
    pub(super) active_tab: Option<&'a str>,
    /// A Working status glyph is on screen, so the spinner should turn.
    pub(super) working_visible: bool,
    pub(super) palette: &'a Palette,
}

pub(super) struct ShellFx {
    mgr: EffectManager<FxKey>,
    last: Option<Instant>,
    spin: u8,
    spin_at: Option<Instant>,
    prev_toast: Option<Instant>,
    prev_overlay: Option<Discriminant<ClientShellOverlay>>,
    prev_tab: Option<String>,
    toast_visible: bool,
    working_visible: bool,
    reduce_motion: bool,
    enabled: bool,
}

pub(super) fn reduce_motion_from_env() -> bool {
    reduce_motion(std::env::var("HERDR_REDUCE_MOTION").ok().as_deref())
}

fn reduce_motion(value: Option<&str>) -> bool {
    value == Some("1")
}

impl ShellFx {
    /// `reduce_motion` pins motion off regardless of `ui.animations`.
    pub(super) fn new(reduce_motion: bool) -> Self {
        Self {
            mgr: EffectManager::default(),
            last: None,
            spin: 0,
            spin_at: None,
            prev_toast: None,
            prev_overlay: None,
            prev_tab: None,
            toast_visible: false,
            working_visible: false,
            reduce_motion,
            enabled: !reduce_motion,
        }
    }

    pub(super) fn set_enabled(&mut self, animations: bool) {
        let enabled = animations && !self.reduce_motion;
        if !enabled && self.enabled {
            self.mgr = EffectManager::default();
            self.spin = 0;
            self.spin_at = None;
        }
        self.enabled = enabled;
    }

    pub(super) fn spin(&self) -> u8 {
        self.spin
    }

    pub(super) fn observe(&mut self, frame: FxFrame<'_>, now: Instant) {
        let overlay = frame.overlay.map(std::mem::discriminant);
        let new_toast = frame.toast.is_some() && frame.toast != self.prev_toast;
        let new_overlay = overlay.is_some() && overlay != self.prev_overlay;
        let new_tab = self.prev_tab.is_some()
            && frame.active_tab.is_some()
            && frame.active_tab != self.prev_tab.as_deref();
        self.prev_toast = frame.toast;
        self.prev_overlay = overlay;
        self.prev_tab = frame.active_tab.map(str::to_owned);
        self.toast_visible = frame.toast.is_some();
        self.working_visible = frame.working_visible;
        if !self.working_visible {
            self.spin = 0;
            self.spin_at = None;
        } else if self.spin_at.is_none() {
            self.spin_at = Some(now);
        }
        if !self.enabled {
            return;
        }

        let palette = frame.palette;
        let rgb = |color: Color| matches!(color, Color::Rgb(..));
        let was_idle = !self.mgr.is_running();
        let mut queued = false;
        let toast_rect = frame.hits.notification_toast;
        if new_toast && !toast_rect.is_empty() {
            let slide = fx::slide_in(
                Motion::RightToLeft,
                8,
                0,
                palette.panel_bg,
                (220, Interpolation::QuadOut),
            );
            let effect = if rgb(palette.panel_bg) {
                fx::parallel(&[
                    slide,
                    fx::fade_from_fg(palette.panel_bg, (220, Interpolation::QuadOut)),
                ])
            } else {
                slide
            };
            self.mgr
                .add_unique_effect(FxKey::Toast, effect.with_area(toast_rect));
            queued = true;
        }
        let overlay_rect = frame.hits.overlay_area;
        if new_overlay && !overlay_rect.is_empty() && rgb(palette.panel_bg) {
            self.mgr.add_unique_effect(
                FxKey::Overlay,
                fx::fade_from(
                    palette.panel_bg,
                    palette.panel_bg,
                    (180, Interpolation::QuadOut),
                )
                .with_area(overlay_rect),
            );
            queued = true;
        }
        if new_tab && rgb(palette.accent) {
            if let Some(rect) = frame
                .hits
                .tabs
                .iter()
                .find(|(_, tab_id)| Some(tab_id.as_str()) == frame.active_tab)
                .map(|(rect, _)| *rect)
            {
                self.mgr.add_unique_effect(
                    FxKey::Tab,
                    fx::sweep_in(Motion::LeftToRight, 6, 0, palette.accent, 200).with_area(rect),
                );
                queued = true;
            }
        }
        if queued && was_idle {
            // Start the clock now so the first processed frame is frame 0.
            self.last = Some(now);
        }
    }

    pub(super) fn running(&self) -> bool {
        self.enabled && self.mgr.is_running()
    }

    pub(super) fn process(&mut self, buf: &mut Buffer, now: Instant) {
        let elapsed = self
            .last
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last = Some(now);
        let millis = u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX);
        let area: Rect = buf.area;
        self.mgr
            .process_effects(tachyonfx::Duration::from_millis(millis), buf, area);
    }

    /// When the render loop should wake next for motion, if at all.
    pub(super) fn deadline(&self, now: Instant) -> Option<Instant> {
        if !self.enabled {
            return None;
        }
        if self.mgr.is_running() {
            return Some(now + FRAME);
        }
        if self.working_visible || self.toast_visible {
            return Some(self.spin_at.unwrap_or(now) + SPIN_STEP);
        }
        None
    }

    /// Advance the spinner (and the toast countdown) and say whether to repaint.
    pub(super) fn tick(&mut self, now: Instant) -> bool {
        if !self.enabled {
            return false;
        }
        if self.mgr.is_running() {
            return true;
        }
        if !(self.working_visible || self.toast_visible) {
            return false;
        }
        let due = self
            .spin_at
            .is_none_or(|at| now.saturating_duration_since(at) >= SPIN_STEP);
        if due {
            self.spin_at = Some(now);
            if self.working_visible {
                self.spin = (self.spin + 1) % SPIN_FRAMES;
            }
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette::catppuccin()
    }

    fn frame<'a>(
        hits: &'a ShellHitMap,
        toast: Option<Instant>,
        working: bool,
        palette: &'a Palette,
    ) -> FxFrame<'a> {
        FxFrame {
            hits,
            overlay: None,
            toast,
            active_tab: None,
            working_visible: working,
            palette,
        }
    }

    #[test]
    fn idle_fx_has_no_deadline_and_effects_ask_for_frames() {
        let now = Instant::now();
        let palette = palette();
        let mut fx = ShellFx::new(false);
        assert_eq!(fx.deadline(now), None);
        assert!(!fx.running());

        let hits = ShellHitMap {
            notification_toast: Rect::new(10, 1, 20, 4),
            ..ShellHitMap::default()
        };
        fx.observe(frame(&hits, Some(now), false, &palette), now);
        assert!(fx.running());
        let deadline = fx.deadline(now).expect("effect running");
        assert!(deadline.duration_since(now) <= Duration::from_millis(16));
    }

    #[test]
    fn new_toast_runs_until_its_effect_finishes() {
        let now = Instant::now();
        let palette = palette();
        let mut fx = ShellFx::new(false);
        let hits = ShellHitMap {
            notification_toast: Rect::new(10, 1, 20, 4),
            ..ShellHitMap::default()
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 10));
        fx.observe(frame(&hits, Some(now), false, &palette), now);
        assert!(fx.running());
        fx.process(&mut buffer, now);
        fx.process(&mut buffer, now + Duration::from_millis(150));
        assert!(fx.running());
        fx.process(&mut buffer, now + Duration::from_millis(300));
        assert!(!fx.running());
        // The same toast on the next frame does not restart it.
        fx.observe(frame(&hits, Some(now), false, &palette), now);
        assert!(!fx.running());
    }

    #[test]
    fn disabled_fx_never_ticks_or_queues() {
        let now = Instant::now();
        let palette = palette();
        let mut fx = ShellFx::new(false);
        fx.set_enabled(false);
        let hits = ShellHitMap {
            notification_toast: Rect::new(10, 1, 20, 4),
            ..ShellHitMap::default()
        };
        fx.observe(frame(&hits, Some(now), true, &palette), now);
        assert!(!fx.running());
        assert!(!fx.tick(now + Duration::from_secs(1)));
        assert_eq!(fx.deadline(now), None);
        assert_eq!(fx.spin(), 0);
    }

    #[test]
    fn reduce_motion_overrides_animations() {
        let mut fx = ShellFx::new(true);
        fx.set_enabled(true);
        assert!(!fx.tick(Instant::now()));
        assert_eq!(fx.deadline(Instant::now()), None);
    }

    #[test]
    fn reduce_motion_env_value_one_turns_motion_off() {
        assert!(reduce_motion(Some("1")));
        assert!(!reduce_motion(Some("0")));
        assert!(!reduce_motion(None));
    }

    #[test]
    fn spinner_cycles_all_frames_and_frame_zero_is_the_static_glyph() {
        use crate::api::schema::AgentStatus;
        use crate::config::{ChromeStyle, StatusIndicatorStyle};
        let now = Instant::now();
        let palette = palette();
        let hits = ShellHitMap::default();
        let mut fx = ShellFx::new(false);
        fx.observe(frame(&hits, None, true, &palette), now);
        assert_eq!(
            super::super::status_icon_frame(
                AgentStatus::Working,
                StatusIndicatorStyle::Symbols,
                ChromeStyle::Modern,
                fx.spin()
            ),
            super::super::status_icon(AgentStatus::Working, StatusIndicatorStyle::Symbols)
        );
        let mut seen = std::collections::HashSet::new();
        for step in 1..=SPIN_FRAMES as u64 {
            assert!(fx.tick(now + SPIN_STEP * step as u32));
            seen.insert(fx.spin());
        }
        assert_eq!(seen.len(), usize::from(SPIN_FRAMES));
        assert_eq!(fx.spin(), 0, "a full cycle returns to the static frame");
        // Between steps nothing repaints.
        assert!(!fx.tick(now + SPIN_STEP * SPIN_FRAMES as u32 + Duration::from_millis(10)));
    }
}
