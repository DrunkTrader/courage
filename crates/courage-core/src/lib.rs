//! Platform-independent capture primitives for Courage.
//!
//! The core deliberately accepts frames instead of opening a display or audio
//! device. Native backends can feed the same bounded engine on each platform,
//! while deterministic sources keep the media path testable in CI.

mod buffer;
mod editor;
mod error;
mod export;
mod synthetic;

pub use buffer::{BufferStats, FrameRingBuffer, FrameSnapshot, VideoFrame, VideoSpec};
pub use editor::{trim, EditReport};
pub use error::CoreError;
pub use export::{ClipExporter, ExportOptions, ExportReport};
pub use synthetic::SyntheticSource;
