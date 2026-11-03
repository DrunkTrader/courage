use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{CoreError, FrameSnapshot, VideoSpec};

#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    pub include_synthetic_audio: bool,
    pub crf: u8,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { include_synthetic_audio: false, crf: 23 }
    }
}

#[derive(Debug)]
pub struct ExportReport {
    pub path: PathBuf,
    pub frame_count: usize,
    pub duration_seconds: f64,
    pub bytes: u64,
}

pub struct ClipExporter {
    spec: VideoSpec,
}

impl ClipExporter {
    pub fn new(spec: VideoSpec) -> Self {
        Self { spec }
    }

    pub fn export(
        &self,
        snapshot: &FrameSnapshot,
        output: impl AsRef<Path>,
        options: ExportOptions,
    ) -> Result<ExportReport, CoreError> {
        if snapshot.frames.is_empty() {
            return Err(CoreError::EmptySnapshot);
        }
        let output = output.as_ref();
        if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|source| CoreError::CreateExportDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let duration_seconds = (snapshot.frames.len() as f64 / self.spec.fps as f64).max(0.001);
        let mut command = Command::new("ffmpeg");
        command
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(["-f", "rawvideo", "-pixel_format", "rgb24"])
            .args(["-video_size", &format!("{}x{}", self.spec.width, self.spec.height)])
            .args(["-framerate", &self.spec.fps.to_string(), "-i", "pipe:0"]);
        if options.include_synthetic_audio {
            command.args([
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency=440:sample_rate=48000:duration={duration_seconds:.3}"),
            ]);
        }
        command.args(["-map", "0:v:0"]);
        if options.include_synthetic_audio {
            command.args(["-map", "1:a:0", "-c:a", "aac", "-b:a", "128k", "-shortest"]);
        }
        command.args([
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            &options.crf.to_string(),
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
        ]);
        command.arg(output);
        command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = command.spawn().map_err(CoreError::FfmpegUnavailable)?;
        {
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                CoreError::FfmpegWrite(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "ffmpeg stdin unavailable",
                ))
            })?;
            for frame in &snapshot.frames {
                stdin.write_all(&frame.rgb24).map_err(CoreError::FfmpegWrite)?;
            }
        }
        let output_status = child.wait_with_output().map_err(CoreError::FfmpegWrite)?;
        if !output_status.status.success() {
            return Err(CoreError::FfmpegFailed {
                status: output_status.status.to_string(),
                stderr: String::from_utf8_lossy(&output_status.stderr).trim().to_owned(),
            });
        }
        let bytes = fs::metadata(output).map_err(CoreError::ExportMetadata)?.len();
        Ok(ExportReport {
            path: output.to_path_buf(),
            frame_count: snapshot.frames.len(),
            duration_seconds,
            bytes,
        })
    }
}
