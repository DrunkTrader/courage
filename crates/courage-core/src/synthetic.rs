use std::time::Duration;

use crate::{CaptureSource, CoreError, VideoFrame, VideoSpec};

pub struct SyntheticSource {
    spec: VideoSpec,
    next_sequence: u64,
}

impl SyntheticSource {
    pub fn new(spec: VideoSpec) -> Self {
        Self { spec, next_sequence: 0 }
    }

    fn make_frame(&self, sequence: u64) -> Result<VideoFrame, CoreError> {
        let mut rgb24 = vec![0_u8; self.spec.frame_bytes()];
        let width = self.spec.width as usize;
        let height = self.spec.height as usize;
        let moving_x = (sequence as usize * 7) % width;
        let moving_y = (sequence as usize * 3) % height;
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 3;
                let in_marker =
                    x.abs_diff(moving_x) < width / 12 && y.abs_diff(moving_y) < height / 12;
                rgb24[index] = ((x * 255) / width) as u8;
                rgb24[index + 1] = ((y * 255) / height) as u8;
                rgb24[index + 2] = if in_marker { 240 } else { 48 };
            }
        }
        let timestamp = Duration::from_secs_f64(sequence as f64 / self.spec.fps as f64);
        VideoFrame::new(sequence, timestamp, rgb24, self.spec)
    }
}

impl Iterator for SyntheticSource {
    type Item = Result<VideoFrame, CoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.make_frame(self.next_sequence);
        self.next_sequence = self.next_sequence.saturating_add(1);
        Some(result)
    }
}

impl CaptureSource for SyntheticSource {
    fn spec(&self) -> VideoSpec {
        self.spec
    }

    fn next_frame(&mut self) -> Result<Option<VideoFrame>, CoreError> {
        self.next().transpose()
    }
}
