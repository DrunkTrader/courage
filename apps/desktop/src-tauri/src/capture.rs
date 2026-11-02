use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::process::ManagedChild;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureState {
    Stopped,
    Starting,
    Recording,
    Stopping,
    Error,
}

pub struct CaptureProcess {
    pub session_id: String,
    pub directory: PathBuf,
    pub state: CaptureState,
    pub error: Option<String>,
    child: Option<ManagedChild>,
}

impl CaptureProcess {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            session_id: directory.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            directory,
            state: CaptureState::Starting,
            error: None,
            child: None,
        }
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(ManagedChild::id)
    }

    pub fn start(
        &mut self,
        command: &mut Command,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> Result<(), String> {
        if self.child.is_some() {
            return Err("Session already owns a process".into());
        }
        if cancel.load(Ordering::Acquire) {
            return self.fail("Start cancelled".into());
        }
        self.child = Some(match ManagedChild::spawn(command) {
            Ok(child) => child,
            Err(error) => return self.fail(format!("FFmpeg spawn: {error}")),
        });
        self.record("capture.starting");
        let deadline = Instant::now() + timeout;
        loop {
            let child = self.child.as_mut().expect("spawned");
            if child.try_wait().map_err(|error| error.to_string())?.is_some() {
                let error = format!("FFmpeg initialization failed: {}", child.stderr());
                return self.fail(error);
            }
            let ready = child.lines().iter().any(|line| {
                line.text
                    .strip_prefix("frame=")
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .is_some_and(|frame| frame > 0)
            });
            if cancel.load(Ordering::Acquire) {
                return self.fail("Start cancelled by stop/exit".into());
            }
            if ready {
                self.state = CaptureState::Recording;
                self.record("capture.recording");
                return Ok(());
            }
            if Instant::now() >= deadline {
                return self.fail("FFmpeg produced no frames before the startup deadline".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn fail(&mut self, message: String) -> Result<(), String> {
        self.state = CaptureState::Error;
        self.error = Some(message.clone());
        self.record("capture.error");
        Err(message)
    }

    pub fn poll(&mut self) -> Result<(), String> {
        if self.state != CaptureState::Recording {
            return Ok(());
        }
        let Some(child) = &mut self.child else {
            return self.fail("Capture owner is missing".into());
        };
        match child.try_wait() {
            Ok(None) => Ok(()),
            result => {
                let details =
                    format!("FFmpeg exited unexpectedly ({result:?}): {}", child.stderr());
                self.fail(details)
            }
        }
    }

    pub fn stop(&mut self) -> Result<(), String> {
        if self.state == CaptureState::Stopped {
            return Ok(());
        }
        self.state = CaptureState::Stopping;
        self.record("capture.stopping");
        if let Some(child) = &mut self.child {
            if let Err(error) = child.shutdown(b"q\n", Duration::from_secs(2)) {
                return self.fail(format!("Owned FFmpeg shutdown failed: {error}"));
            }
        }
        // No writes/readers/jobs remain. Keep the session on failure so Stop can retry.
        if let Err(error) = remove_session(&self.directory) {
            return self.fail(error);
        }
        self.state = CaptureState::Stopped;
        self.error = None;
        self.record("capture.stopped");
        Ok(())
    }

    fn record(&self, event: &str) {
        eprintln!(
            "{}",
            serde_json::json!({ "event": event, "ownerPid": std::process::id(), "sessionId": self.session_id,
            "pid": self.pid(), "directory": self.directory, "state": self.state,
            "forced": self.child.as_ref().is_some_and(|child| child.forced), "error": self.error })
        );
    }
}

impl Drop for CaptureProcess {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("session {} cleanup retained: {error}", self.session_id);
        }
    }
}

fn remove_session(path: &Path) -> Result<(), String> {
    if !path.try_exists().map_err(|error| cleanup_error(path, error))? {
        return Ok(());
    }
    // Enumerate first: the ReadDir directory handle itself is closed before removing
    // the directory. Report the exact failed file, not only its parent directory.
    let entries = fs::read_dir(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for file in entries {
        fs::remove_file(&file).map_err(|error| cleanup_error(&file, error))?;
    }
    fs::remove_dir(path).map_err(|error| cleanup_error(path, error))
}

fn cleanup_error(path: &Path, error: std::io::Error) -> String {
    #[cfg(windows)]
    let owners = crate::windows_process::locking_processes(path);
    #[cfg(not(windows))]
    let owners = "lock owner inspection is Windows-only";
    format!("remove {}: {error}; lock owners: {owners}. Session retained; retry Stop after releasing this path.", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::helper;

    #[test]
    fn sessions_are_isolated_and_failed_cleanup_can_retry() {
        let root = std::env::temp_dir().join(format!(
            "courage-sessions-{}-{}",
            std::process::id(),
            crate::unix_timestamp_millis()
        ));
        let first = crate::new_session_dir(&root).unwrap();
        let second = crate::new_session_dir(&root).unwrap();
        assert_ne!(first, second);
        let saved = root.join("saved.mp4");
        fs::write(&saved, b"existing saved clip").unwrap();
        let mut capture = CaptureProcess::new(first.clone());
        capture
            .start(&mut helper("child_quit"), &AtomicBool::new(false), Duration::from_secs(3))
            .unwrap();
        assert_eq!(capture.state, CaptureState::Recording);
        // A foreign directory must not be recursively deleted or silently ignored.
        let blocker = first.join("foreign-directory");
        fs::create_dir(&blocker).unwrap();
        let error = capture.stop().unwrap_err();
        assert!(error.contains("foreign-directory"));
        assert_eq!(capture.state, CaptureState::Error);
        assert!(capture.child.as_ref().unwrap().released());
        assert!(second.is_dir() && saved.is_file());
        fs::remove_dir(blocker).unwrap();
        capture.stop().unwrap();
        capture.stop().unwrap();
        assert_eq!(capture.state, CaptureState::Stopped);
        assert!(!first.exists());
        assert_eq!(fs::read(saved).unwrap(), b"existing saved clip");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_failure_and_cancellation_remove_only_their_session() {
        let root = std::env::temp_dir().join(format!(
            "courage-start-failure-{}-{}",
            std::process::id(),
            crate::unix_timestamp_millis()
        ));
        for cancelled in [false, true] {
            let dir = crate::new_session_dir(&root).unwrap();
            let mut capture = CaptureProcess::new(dir.clone());
            assert!(capture
                .start(
                    &mut Command::new("courage-missing-test-executable"),
                    &AtomicBool::new(cancelled),
                    Duration::from_millis(50)
                )
                .is_err());
            capture.stop().unwrap();
            assert!(!dir.exists());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initialization_timeout_and_exit_cancellation_reap_before_cleanup() {
        let root = std::env::temp_dir().join(format!(
            "courage-initializing-{}-{}",
            std::process::id(),
            crate::unix_timestamp_millis()
        ));
        for exit in [false, true] {
            let dir = crate::new_session_dir(&root).unwrap();
            let mut capture = CaptureProcess::new(dir.clone());
            let cancel = AtomicBool::new(false);
            std::thread::scope(|scope| {
                if exit {
                    scope.spawn(|| {
                        std::thread::sleep(Duration::from_millis(50));
                        cancel.store(true, Ordering::Release);
                    });
                }
                let error = capture
                    .start(&mut helper("child_initializing"), &cancel, Duration::from_millis(150))
                    .unwrap_err();
                assert!(error.contains(if exit { "cancelled" } else { "deadline" }), "{error}");
                assert!(capture.pid().is_some());
            });
            capture.stop().unwrap();
            assert!(capture.child.as_mut().unwrap().try_wait().unwrap().is_some());
            assert!(capture.child.as_ref().unwrap().released());
            assert!(!dir.exists());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overlapping_shutdown_requests_release_one_session_once() {
        let root = std::env::temp_dir().join(format!(
            "courage-stop-race-{}-{}",
            std::process::id(),
            crate::unix_timestamp_millis()
        ));
        let dir = crate::new_session_dir(&root).unwrap();
        let mut capture = CaptureProcess::new(dir.clone());
        capture
            .start(&mut helper("child_quit"), &AtomicBool::new(false), Duration::from_secs(3))
            .unwrap();
        let owner = std::sync::Mutex::new(capture);
        std::thread::scope(|scope| {
            let first = scope.spawn(|| owner.lock().unwrap().stop());
            let second = scope.spawn(|| owner.lock().unwrap().stop());
            first.join().unwrap().unwrap();
            second.join().unwrap().unwrap();
        });
        assert_eq!(owner.lock().unwrap().state, CaptureState::Stopped);
        assert!(!dir.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn sharing_violation_retains_exact_locked_path_until_handle_closed() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = std::env::temp_dir().join(format!(
            "courage-sharing-{}-{}",
            std::process::id(),
            crate::unix_timestamp_millis()
        ));
        let dir = crate::new_session_dir(&root).unwrap();
        let file = dir.join("segment-00001.mp4");
        fs::write(&file, b"segment").unwrap();
        let lock = fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
        let mut capture = CaptureProcess::new(dir.clone());
        let error = capture.stop().unwrap_err();
        assert!(error.contains("segment-00001.mp4") && error.contains("os error 32"), "{error}");
        assert!(file.is_file());
        drop(lock);
        capture.stop().unwrap();
        assert!(!dir.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
