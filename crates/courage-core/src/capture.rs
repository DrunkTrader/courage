use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{BufferStats, CoreError, FrameRingBuffer, FrameSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl VideoSpec {
    pub fn new(width: u32, height: u32, fps: u32) -> Result<Self, CoreError> {
        if width == 0 || height == 0 {
            return Err(CoreError::InvalidDimensions);
        }
        if fps == 0 {
            return Err(CoreError::InvalidFrameRate);
        }
        Ok(Self { width, height, fps })
    }

    pub fn frame_bytes(self) -> usize {
        self.width as usize * self.height as usize * 3
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    pub sequence: u64,
    pub timestamp: Duration,
    pub rgb24: Vec<u8>,
}

/// Platform-independent input boundary for native video backends.
pub trait CaptureSource: Send {
    fn spec(&self) -> VideoSpec;
    fn next_frame(&mut self) -> Result<Option<VideoFrame>, CoreError>;
}

impl VideoFrame {
    pub fn new(
        sequence: u64,
        timestamp: Duration,
        rgb24: Vec<u8>,
        spec: VideoSpec,
    ) -> Result<Self, CoreError> {
        let expected = spec.frame_bytes();
        if rgb24.len() != expected {
            return Err(CoreError::InvalidFrameSize { actual: rgb24.len(), expected });
        }
        Ok(Self { sequence, timestamp, rgb24 })
    }
}

pub struct CaptureEngine {
    spec: VideoSpec,
    buffer: Arc<Mutex<FrameRingBuffer>>,
}

impl CaptureEngine {
    pub fn new(spec: VideoSpec, capacity: usize, max_bytes: usize) -> Result<Self, CoreError> {
        Ok(Self { spec, buffer: Arc::new(Mutex::new(FrameRingBuffer::new(capacity, max_bytes)?)) })
    }

    pub fn spec(&self) -> VideoSpec {
        self.spec
    }

    pub fn push_frame(&self, frame: VideoFrame) -> Result<BufferStats, CoreError> {
        if frame.rgb24.len() != self.spec.frame_bytes() {
            return Err(CoreError::InvalidFrameSize {
                actual: frame.rgb24.len(),
                expected: self.spec.frame_bytes(),
            });
        }
        let mut buffer = self.buffer.lock().map_err(|_| CoreError::StatePoisoned)?;
        buffer.push(frame)
    }

    pub fn snapshot(&self, duration: Duration) -> Result<FrameSnapshot, CoreError> {
        let buffer = self.buffer.lock().map_err(|_| CoreError::StatePoisoned)?;
        buffer.snapshot(duration)
    }

    pub fn stats(&self) -> Result<BufferStats, CoreError> {
        let buffer = self.buffer.lock().map_err(|_| CoreError::StatePoisoned)?;
        Ok(buffer.stats())
    }
}
