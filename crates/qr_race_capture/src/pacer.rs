//! Frame-rate policing. A 60fps device feeding a 30fps budget must drop frames
//! deliberately rather than letting the decoder lag behind the camera.

use crate::error::CaptureError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PacerStats {
    pub accepted: u64,
    pub dropped: u64,
}

#[derive(Debug)]
pub struct Pacer {
    interval_ms: u64,
    last_accepted_ms: Option<u64>,
    stats: PacerStats,
}

impl Pacer {
    pub fn new(target_fps: u32) -> Result<Self, CaptureError> {
        if target_fps == 0 {
            return Err(CaptureError::ZeroFrameRate);
        }
        Ok(Self {
            // Integer division caps the rate a little below the target (30fps
            // becomes 33ms); overshooting the budget is the failure that matters.
            interval_ms: (1000 / u64::from(target_fps)).max(1),
            last_accepted_ms: None,
            stats: PacerStats::default(),
        })
    }

    pub fn interval_ms(&self) -> u64 {
        self.interval_ms
    }

    /// Accepts the frame at `now_ms` if a full interval has passed.
    pub fn should_accept(&mut self, now_ms: u64) -> bool {
        let due = match self.last_accepted_ms {
            None => true,
            Some(last) => now_ms >= last.saturating_add(self.interval_ms),
        };
        if due {
            self.last_accepted_ms = Some(now_ms);
            self.stats.accepted += 1;
        } else {
            self.stats.dropped += 1;
        }
        due
    }

    pub fn stats(&self) -> PacerStats {
        self.stats
    }

    pub fn reset(&mut self) {
        self.last_accepted_ms = None;
        self.stats = PacerStats::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_zero_target() {
        assert_eq!(Pacer::new(0).unwrap_err(), CaptureError::ZeroFrameRate);
    }

    #[test]
    fn thirty_fps_means_one_frame_per_33ms() {
        let mut pacer = Pacer::new(30).unwrap();
        assert_eq!(pacer.interval_ms(), 33);
        assert!(pacer.should_accept(0));
        assert!(!pacer.should_accept(10));
        assert!(!pacer.should_accept(32));
        assert!(pacer.should_accept(33));
        assert!(pacer.should_accept(66));
        assert_eq!(
            pacer.stats(),
            PacerStats {
                accepted: 3,
                dropped: 2
            }
        );
    }

    #[test]
    fn high_target_rates_still_have_a_floor() {
        let pacer = Pacer::new(2000).unwrap();
        assert_eq!(pacer.interval_ms(), 1);
    }

    #[test]
    fn reset_forgets_history() {
        let mut pacer = Pacer::new(30).unwrap();
        assert!(pacer.should_accept(0));
        pacer.reset();
        assert!(pacer.should_accept(1));
        assert_eq!(pacer.stats().accepted, 1);
        assert_eq!(pacer.stats().dropped, 0);
    }
}
