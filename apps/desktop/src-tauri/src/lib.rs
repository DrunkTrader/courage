use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use courage_store::{ClipRecord, ClipStore};
use serde::Serialize;
use tauri::{Manager, State};

const DEFAULT_REPLAY_SECONDS: u64 = 30;
const CAPTURE_FPS: u32 = 30;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub state: String,
    pub platform: String,
    pub backend: String,
    pub voice_trigger: String,
    pub replay_seconds: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipResponse {
    pub id: i64,
    pub path: String,
    pub duration_ms: i64,
    pub created_at: i64,
    pub title: String,
}

struct AppState {
    data_dir: PathBuf,
    clips_dir: PathBuf,
    store: ClipStore,
    capture: Option<CaptureSession>,
    replay_seconds: u64,
}

struct CaptureSession {
    process: Child,
    segments_dir: PathBuf,
    backend: String,
    audio_enabled: bool,
}

impl AppState {
    fn new(data_dir: PathBuf) -> Result<Self, String> {
        let clips_dir = data_dir.join("clips");
        fs::create_dir_all(&clips_dir).map_err(io_message)?;
        let segments_dir = data_dir.join("rolling-buffer");
        fs::create_dir_all(&segments_dir).map_err(io_message)?;
        let store =
            ClipStore::open(data_dir.join("library.sqlite")).map_err(|error| error.to_string())?;
        Ok(Self {
            data_dir,
            clips_dir,
            store,
            capture: None,
            replay_seconds: DEFAULT_REPLAY_SECONDS,
        })
    }
}

#[tauri::command]
fn engine_status(state: State<'_, Mutex<AppState>>) -> Result<EngineStatus, String> {
    let mut state = state.lock().map_err(|_| "Courage state is unavailable".to_owned())?;
    let running = refresh_capture_status(&mut state);
    let backend = capture_backend_name().unwrap_or_else(|error| error);
    Ok(EngineStatus {
        state: if running { "capturing" } else { "ready" }.to_owned(),
        platform: std::env::consts::OS.to_owned(),
        backend,
        voice_trigger: "browser microphone wake phrase".to_owned(),
        replay_seconds: state.replay_seconds,
    })
}

#[tauri::command]
fn start_capture(
    replay_seconds: Option<u64>,
    state: State<'_, Mutex<AppState>>,
) -> Result<EngineStatus, String> {
    let mut state = state.lock().map_err(|_| "Courage state is unavailable".to_owned())?;
    let replay_seconds = replay_seconds.unwrap_or(DEFAULT_REPLAY_SECONDS).clamp(5, 120);
    let was_running = refresh_capture_status(&mut state);
    if was_running && state.replay_seconds == replay_seconds {
        return status_for(&state, "capturing");
    }
    if was_running {
        if let Some(mut capture) = state.capture.take() {
            let _ = capture.process.kill();
            let _ = capture.process.wait();
        }
    }
    state.replay_seconds = replay_seconds;

    let (mut command, backend, audio_enabled) =
        build_capture_command(&state.data_dir, replay_seconds)?;
    prepare_segments_dir(&state.data_dir.join("rolling-buffer"))?;
    hide_console_window(&mut command);
    let process = command.spawn().map_err(|error| {
        format!("Could not start FFmpeg capture: {error}. Install FFmpeg and add it to PATH.")
    })?;
    let mut process = process;
    std::thread::sleep(Duration::from_millis(500));
    if let Some(status) = process.try_wait().map_err(io_message)? {
        let mut details = String::new();
        if let Some(mut stderr) = process.stderr.take() {
            let _ = stderr.read_to_string(&mut details);
        }
        let details = details.trim();
        return Err(if details.is_empty() {
            format!("FFmpeg capture exited immediately with status {status}. Check display and microphone permissions.")
        } else {
            format!("FFmpeg capture could not start: {details}")
        });
    }
    if let Some(mut stderr) = process.stderr.take() {
        std::thread::spawn(move || {
            let mut sink = io::sink();
            let _ = io::copy(&mut stderr, &mut sink);
        });
    }
    state.capture = Some(CaptureSession {
        process,
        segments_dir: state.data_dir.join("rolling-buffer"),
        backend,
        audio_enabled,
    });
    Ok(status_for_unchecked(&state, "capturing"))
}

#[tauri::command]
fn stop_capture(state: State<'_, Mutex<AppState>>) -> Result<EngineStatus, String> {
    let mut state = state.lock().map_err(|_| "Courage state is unavailable".to_owned())?;
    if let Some(mut capture) = state.capture.take() {
        let _ = capture.process.kill();
        let _ = capture.process.wait();
    }
    status_for(&state, "ready")
}

#[tauri::command]
fn save_replay(state: State<'_, Mutex<AppState>>) -> Result<ClipResponse, String> {
    let mut state = state.lock().map_err(|_| "Courage state is unavailable".to_owned())?;
    if !refresh_capture_status(&mut state) {
        return Err("Replay buffer is not recording. Start capture and grant the requested display/audio permissions.".to_owned());
    }
    let capture = state.capture.as_ref().ok_or("Replay buffer is not recording")?;
    let segments = completed_segments(&capture.segments_dir, state.replay_seconds)?;
    if segments.is_empty() {
        return Err("The replay buffer is warming up. Try again after a few seconds.".to_owned());
    }

    let timestamp = unix_timestamp_millis();
    let output = state.clips_dir.join(format!("courage-{timestamp}.mp4"));
    concat_segments(&segments, &output)?;
    let duration_ms = probe_duration_ms(&output).unwrap_or((state.replay_seconds * 1000) as i64);
    state.store.insert_clip(&output, duration_ms, None).map_err(|error| error.to_string())?;
    let record = state
        .store
        .list_clips()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|clip| clip.path == output)
        .ok_or("Clip was saved but could not be loaded from the library")?;
    Ok(to_response(record))
}

#[tauri::command]
fn list_clips(state: State<'_, Mutex<AppState>>) -> Result<Vec<ClipResponse>, String> {
    let state = state.lock().map_err(|_| "Courage state is unavailable".to_owned())?;
    state
        .store
        .list_clips()
        .map_err(|error| error.to_string())
        .map(|clips| clips.into_iter().map(to_response).collect())
}

fn status_for(state: &AppState, requested_state: &str) -> Result<EngineStatus, String> {
    Ok(status_for_unchecked(state, requested_state))
}

fn status_for_unchecked(state: &AppState, requested_state: &str) -> EngineStatus {
    let (backend, voice_trigger) = state
        .capture
        .as_ref()
        .map(|capture| {
            (
                format!(
                    "{}{}",
                    capture.backend,
                    if capture.audio_enabled { " + microphone" } else { " (video only)" }
                ),
                "browser microphone wake phrase".to_owned(),
            )
        })
        .unwrap_or_else(|| {
            (
                capture_backend_name().unwrap_or_else(|error| error),
                "browser microphone wake phrase".to_owned(),
            )
        });
    EngineStatus {
        state: requested_state.to_owned(),
        platform: std::env::consts::OS.to_owned(),
        backend,
        voice_trigger,
        replay_seconds: state.replay_seconds,
    }
}

fn refresh_capture_status(state: &mut AppState) -> bool {
    let Some(capture) = state.capture.as_mut() else {
        return false;
    };
    match capture.process.try_wait() {
        Ok(None) => true,
        Ok(Some(_)) | Err(_) => {
            state.capture = None;
            false
        }
    }
}

fn build_capture_command(
    data_dir: &Path,
    replay_seconds: u64,
) -> Result<(Command, String, bool), String> {
    let segments = data_dir.join("rolling-buffer").join("segment-%05d.mp4");
    let wrap = (replay_seconds + 15).clamp(30, 150);
    let mut command = Command::new("ffmpeg");
    command.args(["-hide_banner", "-loglevel", "error", "-y"]).args(video_input_args()?.as_slice());

    let mut audio_enabled = false;
    if let Some(audio_args) = audio_input_args() {
        command.args(audio_args.as_slice());
        audio_enabled = true;
    }
    command.args(["-map", "0:v:0"]);
    if audio_enabled {
        command.args(["-map", "1:a:0", "-c:a", "aac", "-ar", "48000", "-b:a", "160k"]);
    }
    command
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            "-pix_fmt",
            "yuv420p",
            "-r",
            &CAPTURE_FPS.to_string(),
            "-g",
            &CAPTURE_FPS.to_string(),
            "-keyint_min",
            &CAPTURE_FPS.to_string(),
            "-sc_threshold",
            "0",
            "-f",
            "segment",
            "-segment_time",
            "1",
            "-segment_wrap",
            &wrap.to_string(),
            "-reset_timestamps",
            "1",
            "-segment_format",
            "mp4",
            "-movflags",
            "+faststart",
        ])
        .arg(segments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    Ok((command, capture_backend_name()?, audio_enabled))
}

fn video_input_args() -> Result<Vec<String>, String> {
    match std::env::consts::OS {
        "windows" => Ok(vec![
            "-f".into(),
            "gdigrab".into(),
            "-framerate".into(),
            CAPTURE_FPS.to_string(),
            "-draw_mouse".into(),
            "1".into(),
            "-i".into(),
            "desktop".into(),
        ]),
        "macos" => Ok(vec![
            "-f".into(),
            "avfoundation".into(),
            "-framerate".into(),
            CAPTURE_FPS.to_string(),
            "-i".into(),
            std::env::var("COURAGE_VIDEO_DEVICE").unwrap_or_else(|_| "1:none".into()),
        ]),
        "linux" => {
            let display = std::env::var("DISPLAY")
                .map_err(|_| "Linux capture requires an X11 DISPLAY. Wayland/PipeWire capture is not connected yet.".to_owned())?;
            Ok(vec![
                "-f".into(),
                "x11grab".into(),
                "-framerate".into(),
                CAPTURE_FPS.to_string(),
                "-i".into(),
                display,
            ])
        }
        platform => Err(format!("Unsupported capture platform: {platform}")),
    }
}

fn audio_input_args() -> Option<Vec<String>> {
    let configured = std::env::var("COURAGE_AUDIO_DEVICE").ok();
    match std::env::consts::OS {
        "windows" => Some(vec![
            "-f".into(),
            "wasapi".into(),
            "-i".into(),
            configured.unwrap_or_else(|| "default".into()),
        ]),
        "macos" => Some(vec![
            "-f".into(),
            "avfoundation".into(),
            "-i".into(),
            format!("none:{}", configured.unwrap_or_else(|| "0".into())),
        ]),
        "linux" => Some(vec![
            "-f".into(),
            "pulse".into(),
            "-i".into(),
            configured.unwrap_or_else(|| "default".into()),
        ]),
        _ => None,
    }
}

fn capture_backend_name() -> Result<String, String> {
    match std::env::consts::OS {
        "windows" => Ok("FFmpeg gdigrab".to_owned()),
        "macos" => Ok("FFmpeg avfoundation".to_owned()),
        "linux" if std::env::var_os("DISPLAY").is_some() => Ok("FFmpeg x11grab".to_owned()),
        "linux" => Err("Linux display capture unavailable: DISPLAY is not set".to_owned()),
        platform => Err(format!("Unsupported capture platform: {platform}")),
    }
}

fn prepare_segments_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(io_message)?;
    for entry in fs::read_dir(path).map_err(io_message)? {
        let entry = entry.map_err(io_message)?;
        if entry.path().extension().is_some_and(|extension| extension == "mp4") {
            fs::remove_file(entry.path()).map_err(io_message)?;
        }
    }
    Ok(())
}

fn completed_segments(path: &Path, replay_seconds: u64) -> Result<Vec<PathBuf>, String> {
    let mut files = fs::read_dir(path)
        .map_err(io_message)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "mp4"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    if files.len() > 1 {
        files.pop();
    }
    let count = (replay_seconds + 2) as usize;
    Ok(files.into_iter().rev().take(count).map(|(_, path)| path).collect::<Vec<_>>()).map(
        |mut paths| {
            paths.reverse();
            paths
        },
    )
}

fn concat_segments(segments: &[PathBuf], output: &Path) -> Result<(), String> {
    let list_path = output.with_extension("concat.txt");
    let list = segments
        .iter()
        .map(|path| {
            let path = path.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
            format!("file '{path}'")
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&list_path, format!("{list}\n")).map_err(io_message)?;
    let temporary = output.with_extension("tmp.mp4");
    let mut command = Command::new("ffmpeg");
    command
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list_path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(&temporary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    hide_console_window(&mut command);
    let result = command.output().map_err(io_message)?;
    let _ = fs::remove_file(&list_path);
    if !result.status.success() {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "FFmpeg could not save the replay: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    fs::rename(&temporary, output).map_err(io_message)
}

fn probe_duration_ms(path: &Path) -> Option<i64> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .ok()?;
    let seconds = String::from_utf8_lossy(&output.stdout).trim().parse::<f64>().ok()?;
    Some((seconds * 1000.0) as i64)
}

fn to_response(record: ClipRecord) -> ClipResponse {
    let title =
        record.path.file_stem().and_then(|name| name.to_str()).unwrap_or("Courage clip").to_owned();
    ClipResponse {
        id: record.id,
        path: record.path.to_string_lossy().into_owned(),
        duration_ms: record.duration_ms,
        created_at: record.created_at,
        title,
    }
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_millis()
}

fn io_message(error: io::Error) -> String {
    error.to_string()
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir().map_err(|error| error.to_string())?;
            let state = AppState::new(data_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(Mutex::new(state));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_status,
            start_capture,
            stop_capture,
            save_replay,
            list_clips
        ])
        .run(tauri::generate_context!())
        .expect("error while running Courage");
}
