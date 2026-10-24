use super::*;

impl LauncherRoot {
    pub(super) fn cancel_wheel_animation(&mut self) {
        self.wheel_animation = None;
    }

    pub(super) fn on_result_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.keyboard_heading = None;
        if self.action_menu.is_some() {
            self.cancel_wheel_animation();
            return;
        }
        // Precision devices already supply continuous pixel motion and inertia.
        // Let the virtual list consume those events without a second filter.
        if event.delta.precise() {
            // An old touchpad inertia tail must not cancel a mouse animation
            // after the wheel takes over. Started always admits a fresh touch.
            if !self.scroll_handoff.accept_precision(event.touch_phase) {
                cx.stop_propagation();
                return;
            }
            self.cancel_wheel_animation();
            return;
        }
        let delta = event.delta.pixel_delta(window.line_height());
        if delta.y == px(0.0) || delta.x.abs() > delta.y.abs() {
            return;
        }
        let list = self.list.read(cx);
        let handle = list.scroll_handle().clone();
        let max_offset = f32::from(handle.max_offset().y).max(0.0);
        // Pixel input is native and may have left a raw offset beyond the edge
        // before layout clamps it. Start the new wheel owner at the visible edge.
        let current = f32::from(handle.offset().y).clamp(-max_offset, 0.0);
        let mut offset = handle.offset();
        offset.y = px(current);
        handle.set_offset(offset);
        self.scroll_handoff.wheel();
        let generation = list.delegate().results_generation;
        if self.wheel_animation.as_ref().is_some_and(|animation| {
            animation.results_generation != generation
                || (animation.motion.position - current).abs() > 0.5
        }) {
            self.cancel_wheel_animation();
        }
        let start = self.wheel_animation.is_none();
        let animation = self.wheel_animation.get_or_insert_with(|| WheelAnimation {
            motion: crate::scroll::SmoothScroll::new(current),
            last_frame: std::time::Instant::now(),
            results_generation: generation,
        });
        animation.motion.push(f32::from(delta.y), max_offset);
        // Capture before the child's default bubble-phase scroll handler, or
        // every wheel tick would also jump the native scroll offset immediately.
        cx.stop_propagation();
        if start {
            // Input handlers cannot call request_animation_frame: that API
            // reads current_view, which only exists during layout/painting.
            // Mark the root dirty; render advances motion and requests frames.
            cx.notify();
        }
    }

    /// Advance before the list is laid out; only render may request more frames.
    pub(super) fn advance_wheel_frame(&mut self, cx: &mut Context<Self>) -> bool {
        let visible = cx
            .global::<LauncherGlobal>()
            .clone_handle()
            .borrow()
            .visible;
        let list = self.list.read(cx);
        let handle = list.scroll_handle().clone();
        let generation = list.delegate().results_generation;
        let Some(animation) = self.wheel_animation.as_mut() else {
            return false;
        };
        if !visible
            || self.action_menu.is_some()
            || animation.results_generation != generation
            || (f32::from(handle.offset().y) - animation.motion.position).abs() > 0.5
        {
            self.cancel_wheel_animation();
            return false;
        }
        let now = std::time::Instant::now();
        let active = animation.motion.advance(
            now.duration_since(animation.last_frame).as_secs_f32(),
            f32::from(handle.max_offset().y),
        );
        animation.last_frame = now;
        let mut offset = handle.offset();
        offset.y = px(animation.motion.position);
        handle.set_offset(offset);
        self.list.update(cx, |_, cx| cx.notify());
        if !active {
            self.cancel_wheel_animation();
        }
        active
    }
}
