//! Desktop camera capture (Windows Media Foundation, Linux V4L2, macOS
//! AVFoundation) through nokhwa, exposing the platform-neutral capture trait.

use std::time::{SystemTime, UNIX_EPOCH};

use nokhwa::pixel_format::RgbAFormat;
use nokhwa::utils::{
    ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
};
use nokhwa::{Camera, query};
use qr_race_capture::{CaptureError, CaptureSource, CapturedFrame, rgba_to_gray};

/// What to ask the device for. MJPEG keeps USB bandwidth down, which is what
/// lets a 1080p sensor actually deliver a rate near 30fps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// A desktop webcam as a [`CaptureSource`].
pub struct CameraSource {
    camera: Camera,
    description: String,
}

impl CameraSource {
    /// Opens camera `index` at the highest frame rate the device offers, asking
    /// for RGBA output so the gray conversion is one place for every platform.
    pub fn open(index: u32) -> Result<Self, CaptureError> {
        Self::open_requested(index, RequestedFormatType::AbsoluteHighestFrameRate)
    }

    /// Opens the closest available format to `config`, preferring MJPEG.
    pub fn open_with(index: u32, config: CameraConfig) -> Result<Self, CaptureError> {
        let format =
            CameraFormat::new_from(config.width, config.height, FrameFormat::MJPEG, config.fps);
        Self::open_requested(index, RequestedFormatType::Closest(format))
    }

    fn open_requested(index: u32, requested: RequestedFormatType) -> Result<Self, CaptureError> {
        let mut camera = Camera::new(
            CameraIndex::Index(index),
            RequestedFormat::new::<RgbAFormat>(requested),
        )
        .map_err(source_error)?;
        camera.open_stream().map_err(source_error)?;

        let resolution = camera.resolution();
        let description = format!(
            "camera {index}: {} @ {}x{} ({} fps, request: {requested})",
            camera.info().human_name(),
            resolution.width_x,
            resolution.height_y,
            camera.frame_rate()
        );
        Ok(Self {
            camera,
            description,
        })
    }

    pub fn resolution(&self) -> (u32, u32) {
        let resolution = self.camera.resolution();
        (resolution.width_x, resolution.height_y)
    }

    pub fn frame_rate(&self) -> u32 {
        self.camera.frame_rate()
    }
}

impl CaptureSource for CameraSource {
    fn next_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError> {
        let buffer = self.camera.frame().map_err(source_error)?;
        let image = buffer.decode_image::<RgbAFormat>().map_err(source_error)?;
        let (width, height) = image.dimensions();
        let gray = rgba_to_gray(width, height, image.as_raw())?;
        Ok(Some(CapturedFrame {
            gray,
            captured_at_ms: now_ms(),
        }))
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn resolution(&self) -> Option<(u32, u32)> {
        let resolution = self.camera.resolution();
        Some((resolution.width_x, resolution.height_y))
    }
}

impl Drop for CameraSource {
    fn drop(&mut self) {
        let _ = self.camera.stop_stream();
    }
}

/// Human-readable list of attached cameras, for diagnostics and `--list`.
pub fn list_cameras() -> Result<Vec<String>, CaptureError> {
    let cameras = query(ApiBackend::Auto).map_err(source_error)?;
    Ok(cameras
        .iter()
        .map(|camera| {
            format!(
                "{}: {} — {}",
                camera.index(),
                camera.human_name(),
                camera.description()
            )
        })
        .collect())
}

fn source_error(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Source(error.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nokhwa::utils::{FrameFormat, Resolution};

    #[test]
    fn a_raw_rgb_buffer_becomes_a_gray_frame() {
        let buffer = nokhwa::Buffer::new(
            Resolution::new(2, 1),
            &[255, 0, 0, 0, 255, 0],
            FrameFormat::RAWRGB,
        );
        let image = buffer.decode_image::<RgbAFormat>().unwrap();
        assert_eq!(image.dimensions(), (2, 1));
        let gray = rgba_to_gray(2, 1, image.as_raw()).unwrap();
        assert_eq!(gray.pixel(0, 0), Some(76), "red");
        assert_eq!(gray.pixel(1, 0), Some(149), "green");
    }

    #[test]
    #[ignore = "needs a camera and camera permission"]
    fn opens_the_default_camera() {
        let mut source = CameraSource::open(0).unwrap();
        let frame = source.next_frame().unwrap().unwrap();
        assert!(frame.gray.width() > 0);
        assert!(frame.gray.height() > 0);
    }
}
