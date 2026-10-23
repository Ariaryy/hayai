//! Wheel interpolation independent of display refresh rate.

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
        // Input can observe a native offset before layout clamps it. Normalize
        // both ends before adding the next tick so hidden overscroll is not debt.
        self.position = self.position.clamp(-max_offset.max(0.0), 0.0);
        self.target = self.target.clamp(-max_offset.max(0.0), 0.0);
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
    fn first_reverse_tick_is_not_consumed_by_unclamped_overscroll() {
        let mut top = SmoothScroll::new(1000.0);
        top.push(-60.0, 500.0);
        assert_eq!(top.position, 0.0);
        assert_eq!(top.target, -60.0);
        top.advance(0.016, 500.0);
        assert!(top.position < 0.0);

        let mut bottom = SmoothScroll::new(-1500.0);
        bottom.push(60.0, 500.0);
        assert_eq!(bottom.position, -500.0);
        assert_eq!(bottom.target, -440.0);
        bottom.advance(0.016, 500.0);
        assert!(bottom.position > -500.0);
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
