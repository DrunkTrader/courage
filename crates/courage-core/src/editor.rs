use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::CoreError;

#[derive(Debug)]
pub struct EditReport {
    pub path: PathBuf,
    pub duration_seconds: f64,
    pub bytes: u64,
}

pub struct ClipEditor;

impl ClipEditor {
    pub fn trim(
        input: impl AsRef<Path>,
        output: impl AsRef<Path>,
        start: Duration,
        end: Duration,
    ) -> Result<EditReport, CoreError> {
        if end <= start {
            return Err(CoreError::InvalidTrimRange);
        }
        let input = input.as_ref();
        let output = output.as_ref();
        if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|source| CoreError::CreateExportDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let duration = end.saturating_sub(start).as_secs_f64();
        let mut command = Command::new("ffmpeg");
        command
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(["-ss", &format_duration(start), "-i"])
            .arg(input)
            .args(["-t", &format!("{duration:.3}"), "-map", "0"])
            .args([
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "23",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-movflags",
                "+faststart",
            ])
            .arg(output)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let output_status = command.output().map_err(CoreError::FfmpegUnavailable)?;
        if !output_status.status.success() {
            return Err(CoreError::FfmpegFailed {
                status: output_status.status.to_string(),
                stderr: String::from_utf8_lossy(&output_status.stderr).trim().to_owned(),
            });
        }
        let bytes = fs::metadata(output).map_err(CoreError::ExportMetadata)?.len();
        Ok(EditReport { path: output.to_path_buf(), duration_seconds: duration, bytes })
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.3}", duration.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_backwards_trim_range() {
        let result = ClipEditor::trim(
            "input.mp4",
            "output.mp4",
            Duration::from_secs(2),
            Duration::from_secs(1),
        );
        assert!(matches!(result, Err(CoreError::InvalidTrimRange)));
    }
}
