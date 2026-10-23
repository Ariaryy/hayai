//! Wheel interpolation independent of display refresh rate.

/// A wheel tick takes ownership from an ongoing precision gesture. Windows
/// Direct Manipulation continues emitting Moved events during inertia, followed
/// by Ended; a fresh touch always emits Started, even when interrupting inertia.
#[derive(Default)]
pub struct ScrollHandoff {
    wheel_owns_input: bool,
}

impl ScrollHandoff {
    pub fn wheel(&mut self) {
        self.wheel_owns_input = true;
    }

    pub fn accept_precision(&mut self, phase: gpui::TouchPhase) -> bool {
        match phase {
            gpui::TouchPhase::Started => {
                self.wheel_owns_input = false;
                true
            }
            gpui::TouchPhase::Moved => !self.wheel_owns_input,
            gpui::TouchPhase::Ended => {
                let accept = !self.wheel_owns_input;
                self.wheel_owns_input = false;
                accept
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SmoothScroll {
    pub position: f32,
    pub target: f32,
}

impl SmoothScroll {
    pub fn new(position: f32) -> Self {
        Self {
            position,
            target: position,
        }
    }

    pub fn push(&mut self, delta: f32, max_offset: f32) {
        // Reverse immediately rather than paying off an old wheel backlog.
        if delta * (self.target - self.position) < 0.0 {
            self.target = self.position;
        }
        self.target = (self.target + delta).clamp(-max_offset.max(0.0), 0.0);
    }

    /// Returns true while another display frame is needed.
    pub fn advance(&mut self, seconds: f32, max_offset: f32) -> bool {
        self.target = self.target.clamp(-max_offset.max(0.0), 0.0);
        self.position = self.position.clamp(-max_offset.max(0.0), 0.0);
        // 35ms time constant: responsive wheel motion with no fixed frame count
        // or timer. Equivalent elapsed time produces the same travel at any Hz.
        let fraction = 1.0 - (-seconds.max(0.0) / 0.035).exp();
        self.position += (self.target - self.position) * fraction;
        if (self.target - self.position).abs() <= 0.25 {
            self.position = self.target;
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_takes_over_inertia_until_end_or_a_fresh_touch() {
        use gpui::TouchPhase::*;
        let mut handoff = ScrollHandoff::default();
        assert!(handoff.accept_precision(Started));
        assert!(handoff.accept_precision(Moved));
        handoff.wheel();
        for _ in 0..100 {
            assert!(!handoff.accept_precision(Moved));
        }
        assert!(!handoff.accept_precision(Ended));
        assert!(handoff.accept_precision(Moved));
        handoff.wheel();
        assert!(!handoff.accept_precision(Moved));
        assert!(handoff.accept_precision(Started));
        assert!(handoff.accept_precision(Moved));
        assert!(handoff.accept_precision(Ended));
    }

    #[test]
    fn equal_time_has_equal_travel_at_60_120_and_240_hz() {
        let mut positions = vec![];
        for hz in [60, 120, 240] {
            let mut scroll = SmoothScroll::new(0.0);
            scroll.push(-300.0, 1000.0);
            for _ in 0..hz / 10 {
                scroll.advance(1.0 / hz as f32, 1000.0);
            }
            positions.push(scroll.position);
        }
        for pair in positions.windows(2) {
            assert!((pair[0] - pair[1]).abs() < 0.01);
        }
    }

    #[test]
    fn wheel_bursts_accumulate_and_reversal_is_immediate() {
        let mut scroll = SmoothScroll::new(-100.0);
        scroll.push(-60.0, 1000.0);
        scroll.push(-60.0, 1000.0);
        assert_eq!(scroll.target, -220.0);
        scroll.advance(0.016, 1000.0);
        let previous = scroll.position;
        scroll.push(40.0, 1000.0);
        assert!(scroll.advance(0.016, 1000.0));
        assert!(scroll.position > previous);
    }

    #[test]
    fn bounds_shrink_and_long_frames_settle_without_overshoot() {
        let mut scroll = SmoothScroll::new(-100.0);
        scroll.push(-10000.0, 500.0);
        assert_eq!(scroll.target, -500.0);
        assert!(!scroll.advance(1.0, 80.0));
        assert_eq!(scroll.position, -80.0);
        scroll.push(10000.0, 80.0);
        assert!(!scroll.advance(1.0, 80.0));
        assert_eq!(scroll.position, 0.0);
        scroll.push(-100.0, 0.0);
        assert!(!scroll.advance(0.016, 0.0));
    }
}
