use super::*;
use crate::scroll::{ScrollHandoff, SmoothScroll};
use gpui::{ScrollHandle, ScrollWheelEvent};
use std::time::Instant;

struct WheelMotion {
    motion: SmoothScroll,
    last_frame: Instant,
}

#[derive(Default)]
pub(super) struct SettingsScroll {
    pub handle: ScrollHandle,
    motion: Option<WheelMotion>,
    handoff: ScrollHandoff,
    pub drag_anchor: Option<f32>,
    pub hovered: bool,
    #[cfg(test)]
    pub frame_samples: Vec<Instant>,
}

impl SettingsScroll {
    pub fn cancel(&mut self) {
        self.motion = None;
        self.drag_anchor = None;
    }

    pub fn reset(&mut self) {
        self.cancel();
        self.handoff = ScrollHandoff::default();
        self.handle.set_offset(gpui::point(px(0.0), px(0.0)));
    }

    pub fn wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &Window,
        cx: &mut Context<SettingsView>,
    ) {
        if event.delta.precise() {
            // Reuse the launcher's wheel/touchpad ownership handoff, keeping
            // native precision inertia unfiltered and rejecting its old tail.
            if self.handoff.accept_precision(event.touch_phase) {
                self.cancel();
            } else {
                cx.stop_propagation();
            }
            return;
        }
        let delta = event.delta.pixel_delta(window.line_height());
        let maximum = f32::from(self.handle.max_offset().y).max(0.0);
        if delta.y == px(0.0) || delta.x.abs() > delta.y.abs() || maximum == 0.0 {
            return;
        }
        let current = f32::from(self.handle.offset().y).clamp(-maximum, 0.0);
        self.set_offset(current);
        self.handoff.wheel();
        self.drag_anchor = None;
        if self
            .motion
            .as_ref()
            .is_some_and(|animation| (animation.motion.position - current).abs() > 0.5)
        {
            self.cancel();
        }
        let start = self.motion.is_none();
        #[cfg(test)]
        if start {
            self.frame_samples.clear();
        }
        let animation = self.motion.get_or_insert_with(|| WheelMotion {
            motion: SmoothScroll::new(current),
            last_frame: Instant::now(),
        });
        animation.motion.push(f32::from(delta.y), maximum);
        cx.stop_propagation();
        if start {
            cx.notify();
        }
    }

    /// Render is the only place allowed to schedule another GPUI frame.
    pub fn advance(&mut self, active: bool) -> bool {
        if !active {
            self.cancel();
            self.hovered = false;
            return false;
        }
        let Some(animation) = self.motion.as_mut() else {
            return false;
        };
        if (f32::from(self.handle.offset().y) - animation.motion.position).abs() > 0.5 {
            self.cancel();
            return false;
        }
        let now = Instant::now();
        #[cfg(test)]
        self.frame_samples.push(now);
        let active = animation.motion.advance(
            now.duration_since(animation.last_frame).as_secs_f32(),
            f32::from(self.handle.max_offset().y),
        );
        animation.last_frame = now;
        let position = animation.motion.position;
        self.set_offset(position);
        if !active {
            self.motion = None;
        }
        active
    }

    pub fn set_offset(&self, y: f32) {
        let mut offset = self.handle.offset();
        offset.y = px(y.clamp(-f32::from(self.handle.max_offset().y).max(0.0), 0.0));
        self.handle.set_offset(offset);
    }

    #[cfg(test)]
    pub fn animating(&self) -> bool {
        self.motion.is_some()
    }
}

impl SettingsView {
    pub(super) fn navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        // Moving focus emits Blur later. Flush edits before dropping their
        // entities/subscriptions, or clicking the sidebar would lose a draft.
        if self.page == Page::Commands && !self.commit_aliases(cx) {
            return;
        }
        self.cancel_recording();
        self.scrolling.reset();
        self.page = page;
        self.alias_editor = if page == Page::Commands {
            Some(super::commands::AliasEditor::new(window, cx))
        } else {
            None
        };
        window.focus(&self.focus, cx);
        cx.notify();
    }
}
