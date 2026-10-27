use std::collections::VecDeque;
use std::time::Duration;

use crate::{CoreError, VideoFrame};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferStats {
    pub frame_count: usize,
    pub byte_count: usize,
    pub capacity: usize,
    pub max_bytes: usize,
    pub evicted_frames: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameSnapshot {
    pub frames: Vec<VideoFrame>,
    pub duration: Duration,
}

pub struct FrameRingBuffer {
    capacity: usize,
    max_bytes: usize,
    byte_count: usize,
    evicted_frames: u64,
    frames: VecDeque<VideoFrame>,
}

impl FrameRingBuffer {
    pub fn new(capacity: usize, max_bytes: usize) -> Result<Self, CoreError> {
        if capacity == 0 {
            return Err(CoreError::InvalidBufferCapacity);
        }
        if max_bytes == 0 {
            return Err(CoreError::InvalidByteBudget);
        }
        Ok(Self {
            capacity,
            max_bytes,
            byte_count: 0,
            evicted_frames: 0,
            frames: VecDeque::with_capacity(capacity),
        })
    }

    pub fn push(&mut self, frame: VideoFrame) -> Result<BufferStats, CoreError> {
        let frame_bytes = frame.rgb24.len();
        if frame_bytes > self.max_bytes {
            return Err(CoreError::FrameExceedsByteBudget);
        }
        if let Some(last) = self.frames.back() {
            if frame.timestamp < last.timestamp {
                return Err(CoreError::NonMonotonicTimestamp);
            }
        }
        while self.frames.len() >= self.capacity
            || self.byte_count.saturating_add(frame_bytes) > self.max_bytes
        {
            if let Some(evicted) = self.frames.pop_front() {
                self.byte_count -= evicted.rgb24.len();
                self.evicted_frames += 1;
            }
        }
        self.byte_count += frame_bytes;
        self.frames.push_back(frame);
        Ok(self.stats())
    }

    pub fn snapshot(&self, duration: Duration) -> Result<FrameSnapshot, CoreError> {
        let Some(latest) = self.frames.back() else {
            return Err(CoreError::EmptySnapshot);
        };
        let start = latest.timestamp.saturating_sub(duration);
        let frames: Vec<_> =
            self.frames.iter().filter(|frame| frame.timestamp >= start).cloned().collect();
        if frames.is_empty() {
            return Err(CoreError::EmptySnapshot);
        }
        let actual_duration = latest
            .timestamp
            .saturating_sub(frames.first().map(|frame| frame.timestamp).unwrap_or(start));
        Ok(FrameSnapshot { frames, duration: actual_duration })
    }

    pub fn stats(&self) -> BufferStats {
        BufferStats {
            frame_count: self.frames.len(),
            byte_count: self.byte_count,
            capacity: self.capacity,
            max_bytes: self.max_bytes,
            evicted_frames: self.evicted_frames,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sequence: u64, millis: u64, bytes: usize) -> VideoFrame {
        VideoFrame { sequence, timestamp: Duration::from_millis(millis), rgb24: vec![0; bytes] }
    }

    #[test]
    fn evicts_oldest_frames_by_count_and_bytes() {
        let mut buffer = FrameRingBuffer::new(2, 11).expect("valid buffer");
        buffer.push(frame(0, 0, 5)).expect("push");
        buffer.push(frame(1, 10, 5)).expect("push");
        buffer.push(frame(2, 20, 6)).expect("push");
        let snapshot = buffer.snapshot(Duration::from_secs(1)).expect("snapshot");
        assert_eq!(snapshot.frames.iter().map(|f| f.sequence).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(buffer.stats().byte_count, 11);
    }

    #[test]
    fn rejects_out_of_order_frames() {
        let mut buffer = FrameRingBuffer::new(4, 100).expect("valid buffer");
        buffer.push(frame(0, 10, 5)).expect("push");
        assert!(matches!(
            buffer.push(frame(1, 9, 5)).expect_err("must reject"),
            CoreError::NonMonotonicTimestamp
        ));
    }
}
