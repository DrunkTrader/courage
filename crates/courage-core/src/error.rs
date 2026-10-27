use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("video dimensions must be non-zero")]
    InvalidDimensions,
    #[error("frame rate must be greater than zero")]
    InvalidFrameRate,
    #[error("frame has {actual} bytes but {expected} bytes are required")]
    InvalidFrameSize { actual: usize, expected: usize },
    #[error("frame timestamp must not move backwards")]
    NonMonotonicTimestamp,
    #[error("buffer capacity must be greater than zero")]
    InvalidBufferCapacity,
    #[error("buffer byte budget must be greater than zero")]
    InvalidByteBudget,
    #[error("capture state lock is poisoned")]
    StatePoisoned,
    #[error("export queue capacity must be greater than zero")]
    InvalidExportQueueCapacity,
    #[error("export queue is full")]
    ExportQueueFull,
    #[error("export queue worker is closed")]
    ExportQueueClosed,
    #[error("failed to start export worker: {0}")]
    ExportWorkerSpawn(#[source] std::io::Error),
    #[error("frame is larger than the buffer byte budget")]
    FrameExceedsByteBudget,
    #[error("no frames are available to export")]
    EmptySnapshot,
    #[error("trim end must be greater than trim start")]
    InvalidTrimRange,
    #[error("failed to create export directory {path}: {source}")]
    CreateExportDirectory { path: PathBuf, source: std::io::Error },
    #[error("failed to start ffmpeg: {0}")]
    FfmpegUnavailable(#[source] std::io::Error),
    #[error("failed to write frames to ffmpeg: {0}")]
    FfmpegWrite(#[source] std::io::Error),
    #[error("ffmpeg failed with status {status}: {stderr}")]
    FfmpegFailed { status: String, stderr: String },
    #[error("failed to inspect exported file: {0}")]
    ExportMetadata(#[source] std::io::Error),
}
