//! A capture session couples one platform source to a frame budget.

use qr_race_scan::GrayFrame;

use crate::error::CaptureError;
use crate::pacer::Pacer;
use crate::source::CaptureSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureConfig {
    pub width: u32,
    pub height: u32,
    pub target_fps: u32,
}

impl CaptureConfig {
    pub fn new(width: u32, height: u32, target_fps: u32) -> Result<Self, CaptureError> {
        if width == 0 || height == 0 {
            return Err(CaptureError::EmptyDimensions);
        }
        if target_fps == 0 {
            return Err(CaptureError::ZeroFrameRate);
        }
        Ok(Self {
            width,
            height,
            target_fps,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureStats {
    pub frames_in: u64,
    pub frames_out: u64,
    pub dropped: u64,
}

pub struct CaptureSession<S: CaptureSource> {
    source: S,
    pacer: Pacer,
    config: CaptureConfig,
    stats: CaptureStats,
}

impl<S: CaptureSource> CaptureSession<S> {
    pub fn new(source: S, config: CaptureConfig) -> Result<Self, CaptureError> {
        let pacer = Pacer::new(config.target_fps)?;
        Ok(Self {
            source,
            pacer,
            config,
            stats: CaptureStats::default(),
        })
    }

    /// Pulls frames until one is due. Returns `None` when the source ends.
    pub fn next_frame(&mut self) -> Result<Option<GrayFrame>, CaptureError> {
        loop {
            let Some(frame) = self.source.next_frame()? else {
                return Ok(None);
            };
            self.stats.frames_in += 1;
            if self.pacer.should_accept(frame.captured_at_ms) {
                self.stats.frames_out += 1;
                return Ok(Some(frame.gray));
            }
            self.stats.dropped += 1;
        }
    }

    pub fn config(&self) -> CaptureConfig {
        self.config
    }

    pub fn stats(&self) -> CaptureStats {
        self.stats
    }

    pub fn source_description(&self) -> &str {
        self.source.description()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{CaptureSource, CapturedFrame};

    struct FakeSource {
        frames: Vec<CapturedFrame>,
        index: usize,
    }

    impl FakeSource {
        fn new(timestamps: &[u64]) -> Self {
            let frames = timestamps
                .iter()
                .map(|stamp| CapturedFrame {
                    gray: GrayFrame::new(2, 2, vec![128; 4]).unwrap(),
                    captured_at_ms: *stamp,
                })
                .collect();
            Self { frames, index: 0 }
        }
    }

    impl CaptureSource for FakeSource {
        fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError> {
            let frame = self.frames.get(self.index).cloned();
            self.index += 1;
            Ok(frame)
        }

        fn description(&self) -> &str {
            "fake"
        }
    }

    #[test]
    fn the_session_keeps_only_due_frames() {
        let config = CaptureConfig::new(1280, 720, 30).unwrap();
        let mut session =
            CaptureSession::new(FakeSource::new(&[0, 10, 33, 66, 99]), config).unwrap();

        let mut delivered = 0;
        while let Some(frame) = session.next_frame().unwrap() {
            assert_eq!(frame.width(), 2);
            delivered += 1;
        }
        assert_eq!(delivered, 4);
        assert_eq!(
            session.stats(),
            CaptureStats {
                frames_in: 5,
                frames_out: 4,
                dropped: 1
            }
        );
        assert_eq!(session.source_description(), "fake");
    }

    #[test]
    fn config_rejects_impossible_values() {
        assert_eq!(
            CaptureConfig::new(0, 720, 30),
            Err(CaptureError::EmptyDimensions)
        );
        assert_eq!(
            CaptureConfig::new(1280, 720, 0),
            Err(CaptureError::ZeroFrameRate)
        );
    }
}
