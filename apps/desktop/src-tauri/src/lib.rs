use std::fs;
use std::io::{self, Read};
#[cfg(windows)]
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use courage_store::{ClipRecord, ClipStore};
use serde::{Deserialize, Serialize};
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

const DEFAULT_REPLAY_SECONDS: u64 = 30;
const CAPTURE_FPS: u32 = 30;
const DEFAULT_HOTKEY: &str = "CommandOrControl+Shift+1";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub state: String,
    pub platform: String,
    pub backend: String,
    pub audio_device: Option<String>,
    pub voice_trigger: String,
    pub output_dir: String,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResponse {
    pub output_dir: String,
    pub default_output_dir: String,
    pub audio_devices: Vec<AudioDevice>,
    pub selected_audio_device: Option<String>,
    pub hotkey: String,
    pub replay_seconds: u64,
    pub voice_enabled: bool,
    pub native_voice_available: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub output_dir: String,
    pub selected_audio_device: Option<String>,
    pub hotkey: String,
    pub replay_seconds: u64,
    pub voice_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SettingsFile {
    output_dir: Option<String>,
    selected_audio_device: Option<String>,
    hotkey: String,
    replay_seconds: u64,
    voice_enabled: bool,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            output_dir: None,
            selected_audio_device: None,
            hotkey: DEFAULT_HOTKEY.to_owned(),
            replay_seconds: DEFAULT_REPLAY_SECONDS,
            voice_enabled: true,
        }
    }
}

struct AppState {
    data_dir: PathBuf,
    default_output_dir: PathBuf,
    settings_path: PathBuf,
    settings: SettingsFile,
    store: ClipStore,
    capture: Option<CaptureSession>,
    voice: Option<VoiceSession>,
}

struct CaptureSession {
    process: Child,
    segments_dir: PathBuf,
    backend: String,
    audio_enabled: bool,
}

struct VoiceSession {
    process: Child,
}

impl AppState {
    fn new(data_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&data_dir).map_err(io_message)?;
        let default_output_dir = data_dir.join("clips");
        fs::create_dir_all(&default_output_dir).map_err(io_message)?;
        let segments_dir = data_dir.join("rolling-buffer");
        fs::create_dir_all(&segments_dir).map_err(io_message)?;
        let settings_path = data_dir.join("settings.json");
        let settings = load_settings(&settings_path)?;
        let output_dir = settings
            .output_dir
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| default_output_dir.clone());
        fs::create_dir_all(&output_dir).map_err(io_message)?;
        let store =
            ClipStore::open(data_dir.join("library.sqlite")).map_err(|error| error.to_string())?;
        Ok(Self {
            data_dir,
            default_output_dir,
            settings_path,
            settings,
            store,
            capture: None,
            voice: None,
        })
    }

    fn output_dir(&self) -> PathBuf {
        self.settings
            .output_dir
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| self.default_output_dir.clone())
    }

    fn persist_settings(&self) -> Result<(), String> {
        let temporary = self.settings_path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(&self.settings).map_err(|error| error.to_string())?;
        fs::write(&temporary, data).map_err(io_message)?;
        fs::rename(temporary, &self.settings_path).map_err(io_message)
    }
}

#[tauri::command]
fn engine_status(state: State<'_, Mutex<AppState>>) -> Result<EngineStatus, String> {
    let mut state = lock_state(&state)?;
    let running = refresh_capture_status(&mut state);
    Ok(status_for(&state, if running { "capturing" } else { "ready" }))
}

#[tauri::command]
fn get_settings(state: State<'_, Mutex<AppState>>) -> Result<SettingsResponse, String> {
    let state = lock_state(&state)?;
    settings_response(&state)
}

#[tauri::command]
fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    discover_audio_devices()
}

#[tauri::command]
fn save_settings(
    settings: SettingsInput,
    state: State<'_, Mutex<AppState>>,
) -> Result<SettingsResponse, String> {
    let mut state = lock_state(&state)?;
    let output_dir = PathBuf::from(settings.output_dir.trim());
    if !output_dir.is_absolute() {
        return Err("Clip folder must be an absolute path.".to_owned());
    }
    if settings.hotkey.trim().is_empty() {
        return Err("Save hotkey cannot be empty.".to_owned());
    }
    fs::create_dir_all(&output_dir).map_err(io_message)?;
    state.settings = SettingsFile {
        output_dir: Some(output_dir.to_string_lossy().into_owned()),
        selected_audio_device: settings.selected_audio_device,
        hotkey: settings.hotkey,
        replay_seconds: settings.replay_seconds.clamp(5, 120),
        voice_enabled: settings.voice_enabled,
    };
    state.persist_settings()?;
    settings_response(&state)
}

#[tauri::command]
fn start_capture(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<EngineStatus, String> {
    let mut state = lock_state(&state)?;
    let was_running = refresh_capture_status(&mut state);
    if was_running {
        return Ok(status_for(&state, "capturing"));
    }
    let devices = discover_audio_devices()?;
    if devices.is_empty() {
        state.settings.selected_audio_device = None;
    } else if state
        .settings
        .selected_audio_device
        .as_ref()
        .is_none_or(|selected| !devices.iter().any(|device| &device.id == selected))
    {
        state.settings.selected_audio_device =
            devices.into_iter().find(|device| device.is_default).map(|device| device.id);
    }
    let (mut command, backend, audio_enabled) = build_capture_command(&state)?;
    let segments_dir = state.data_dir.join("rolling-buffer");
    prepare_segments_dir(&segments_dir)?;
    hide_console_window(&mut command);
    let mut process = command.spawn().map_err(|error| {
        format!("Could not start FFmpeg capture: {error}. Install FFmpeg and add it to PATH.")
    })?;
    std::thread::sleep(Duration::from_millis(500));
    if let Some(status) = process.try_wait().map_err(io_message)? {
        let mut details = String::new();
        if let Some(mut stderr) = process.stderr.take() {
            let _ = stderr.read_to_string(&mut details);
        }
        let details = details.trim();
        return Err(if details.is_empty() {
            format!("FFmpeg capture exited immediately with status {status}. Check the selected display and microphone.")
        } else {
            format!("FFmpeg capture could not start: {details}")
        });
    }
    drain_stderr(process.stderr.take());
    state.capture = Some(CaptureSession { process, segments_dir, backend, audio_enabled });
    if state.settings.voice_enabled {
        if let Err(error) = start_voice_listener(&app, &mut state) {
            let _ = app.emit("capture-error", error);
        }
    }
    Ok(status_for(&state, "capturing"))
}

#[tauri::command]
fn stop_capture(state: State<'_, Mutex<AppState>>) -> Result<EngineStatus, String> {
    let mut state = lock_state(&state)?;
    stop_capture_inner(&mut state);
    stop_voice_listener(&mut state);
    Ok(status_for(&state, "ready"))
}

#[tauri::command]
fn save_replay(app: AppHandle, state: State<'_, Mutex<AppState>>) -> Result<ClipResponse, String> {
    let mut state = lock_state(&state)?;
    let clip = save_replay_inner(&mut state)?;
    notify_clip_saved(&app, &clip, "Replay saved");
    Ok(clip)
}

#[tauri::command]
fn list_clips(state: State<'_, Mutex<AppState>>) -> Result<Vec<ClipResponse>, String> {
    let state = lock_state(&state)?;
    state
        .store
        .list_clips()
        .map_err(|error| error.to_string())
        .map(|clips| clips.into_iter().map(to_response).collect())
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }
    let result = if cfg!(windows) {
        Command::new("explorer.exe").arg(&path).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&path).spawn()
    } else {
        Command::new("xdg-open").arg(&path).spawn()
    };
    result.map(|_| ()).map_err(io_message)
}

fn lock_state<'a>(
    state: &'a State<'_, Mutex<AppState>>,
) -> Result<std::sync::MutexGuard<'a, AppState>, String> {
    state.lock().map_err(|_| "Courage state is unavailable".to_owned())
}

fn settings_response(state: &AppState) -> Result<SettingsResponse, String> {
    let devices = discover_audio_devices()?;
    let selected = state
        .settings
        .selected_audio_device
        .clone()
        .filter(|selected| devices.iter().any(|device| &device.id == selected))
        .or_else(|| devices.iter().find(|device| device.is_default).map(|device| device.id.clone()))
        .or_else(|| devices.first().map(|device| device.id.clone()));
    Ok(SettingsResponse {
        output_dir: state.output_dir().to_string_lossy().into_owned(),
        default_output_dir: state.default_output_dir.to_string_lossy().into_owned(),
        audio_devices: devices,
        selected_audio_device: selected,
        hotkey: state.settings.hotkey.clone(),
        replay_seconds: state.settings.replay_seconds,
        voice_enabled: state.settings.voice_enabled,
        native_voice_available: cfg!(windows),
    })
}

fn status_for(state: &AppState, requested_state: &str) -> EngineStatus {
    let backend = state
        .capture
        .as_ref()
        .map(|capture| {
            format!(
                "{}{}",
                capture.backend,
                if capture.audio_enabled { " + microphone" } else { " (video only)" }
            )
        })
        .unwrap_or_else(|| capture_backend_name().unwrap_or_else(|error| error));
    EngineStatus {
        state: requested_state.to_owned(),
        platform: std::env::consts::OS.to_owned(),
        backend,
        audio_device: state.settings.selected_audio_device.clone(),
        voice_trigger: voice_trigger_status(state),
        output_dir: state.output_dir().to_string_lossy().into_owned(),
        replay_seconds: state.settings.replay_seconds,
    }
}

fn voice_trigger_status(state: &AppState) -> String {
    if !state.settings.voice_enabled {
        return "Voice trigger disabled".to_owned();
    }
    if state.voice.is_some() {
        return if cfg!(windows) {
            "Windows Speech · listening in tray".to_owned()
        } else {
            "Voice trigger active while app is open".to_owned()
        };
    }
    if cfg!(windows) {
        "Windows Speech · waiting for capture".to_owned()
    } else {
        "Native phrase trigger is not available on this platform".to_owned()
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

fn stop_capture_inner(state: &mut AppState) {
    if let Some(mut capture) = state.capture.take() {
        let _ = capture.process.kill();
        let _ = capture.process.wait();
    }
}

fn build_capture_command(state: &AppState) -> Result<(Command, String, bool), String> {
    let segments = state.data_dir.join("rolling-buffer").join("segment-%05d.mp4");
    let wrap = (state.settings.replay_seconds + 15).clamp(30, 150);
    let mut command = Command::new("ffmpeg");
    command.args(["-hide_banner", "-loglevel", "error", "-y"]);
    command.args(video_input_args()?.as_slice());
    let audio_args = audio_input_args(state.settings.selected_audio_device.as_deref());
    let audio_enabled = audio_args.is_some();
    if let Some(args) = audio_args {
        command.args(args.as_slice());
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

fn audio_input_args(device: Option<&str>) -> Option<Vec<String>> {
    let device = device?;
    match std::env::consts::OS {
        "windows" => {
            Some(vec!["-f".into(), "dshow".into(), "-i".into(), format!("audio={device}")])
        }
        "macos" => {
            Some(vec!["-f".into(), "avfoundation".into(), "-i".into(), format!("none:{device}")])
        }
        "linux" => Some(vec!["-f".into(), "pulse".into(), "-i".into(), device.to_owned()]),
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

fn discover_audio_devices() -> Result<Vec<AudioDevice>, String> {
    match std::env::consts::OS {
        "windows" => discover_windows_audio_devices(),
        "macos" => discover_avfoundation_audio_devices(),
        "linux" => discover_pulse_audio_devices(),
        platform => Err(format!("Audio device discovery is not available on {platform}")),
    }
}

fn discover_windows_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let output = match Command::new("ffmpeg")
        .args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return Ok(Vec::new()),
    };
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let mut devices = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("(audio)") {
            continue;
        }
        let Some(start) = trimmed.find('"') else { continue };
        let Some(end) = trimmed[start + 1..].find('"') else { continue };
        let name = trimmed[start + 1..start + 1 + end].to_owned();
        devices.push(AudioDevice {
            id: name.clone(),
            name,
            backend: "DirectShow".to_owned(),
            is_default: devices.is_empty(),
        });
    }
    Ok(devices)
}

fn discover_avfoundation_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let output = match Command::new("ffmpeg")
        .args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .output()
    {
        Ok(output) => output,
        Err(_) => return Ok(Vec::new()),
    };
    let text = String::from_utf8_lossy(&output.stderr);
    let mut in_audio = false;
    let mut devices = Vec::new();
    for line in text.lines() {
        if line.contains("audio devices") {
            in_audio = true;
            continue;
        }
        if !in_audio {
            continue;
        }
        let Some(open) = line.rfind('[') else { continue };
        let Some(close) = line[open..].find(']') else { continue };
        let id = line[open + 1..open + close].trim().to_owned();
        let name = line[open + close + 1..].trim().to_owned();
        if !id.is_empty() && !name.is_empty() {
            devices.push(AudioDevice {
                is_default: devices.is_empty(),
                id,
                name,
                backend: "AVFoundation".to_owned(),
            });
        }
    }
    Ok(devices)
}

fn discover_pulse_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let output = match Command::new("pactl").args(["list", "short", "sources"]).output() {
        Ok(output) => output,
        Err(_) => return Ok(Vec::new()),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 2 || fields[1].contains(".monitor") {
            continue;
        }
        let id = fields[1].to_owned();
        devices.push(AudioDevice {
            is_default: id == "@DEFAULT_SOURCE@" || devices.is_empty(),
            name: id.clone(),
            id,
            backend: "PulseAudio".to_owned(),
        });
    }
    Ok(devices)
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
    if files.is_empty() {
        return Ok(Vec::new());
    }
    files.pop();
    let count = replay_seconds as usize;
    let mut paths = files.into_iter().rev().take(count).map(|(_, path)| path).collect::<Vec<_>>();
    paths.reverse();
    Ok(paths)
}

fn save_replay_inner(state: &mut AppState) -> Result<ClipResponse, String> {
    if !refresh_capture_status(state) {
        return Err(
            "Replay buffer is not recording. Start capture and check the selected microphone."
                .to_owned(),
        );
    }
    let capture = state.capture.as_ref().ok_or("Replay buffer is not recording")?;
    let segments = completed_segments(&capture.segments_dir, state.settings.replay_seconds)?;
    if segments.is_empty() {
        return Err("The replay buffer is warming up. Try again after a few seconds.".to_owned());
    }
    let output_dir = state.output_dir();
    fs::create_dir_all(&output_dir).map_err(io_message)?;
    let timestamp = unix_timestamp_millis();
    let output = output_dir.join(format!("courage-{timestamp}.mp4"));
    concat_segments(&segments, &output)?;
    let size = fs::metadata(&output).map_err(io_message)?.len();
    if size == 0 {
        return Err("FFmpeg created an empty clip. Check the selected capture devices.".to_owned());
    }
    let duration_ms =
        probe_duration_ms(&output).unwrap_or((state.settings.replay_seconds * 1000) as i64);
    let id =
        state.store.insert_clip(&output, duration_ms, None).map_err(|error| error.to_string())?;
    Ok(ClipResponse {
        id,
        path: output.to_string_lossy().into_owned(),
        duration_ms,
        created_at: unix_timestamp_seconds(),
        title: output
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Courage clip")
            .to_owned(),
    })
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

fn notify_clip_saved(app: &AppHandle, clip: &ClipResponse, trigger: &str) {
    #[cfg(windows)]
    {
        use tauri_winrt_notification::{Duration as ToastDuration, Toast};

        let _ = Toast::new(Toast::POWERSHELL_APP_ID)
            .title("Courage · clip saved")
            .text1(trigger)
            .text2(&clip.path)
            .duration(ToastDuration::Short)
            .show();
    }
    #[cfg(not(windows))]
    let _ = (app, trigger, clip);
    let _ = app.emit("clip-saved", clip);
}

fn start_voice_listener(app: &AppHandle, state: &mut AppState) -> Result<(), String> {
    #[cfg(windows)]
    {
        if let Some(voice) = state.voice.as_mut() {
            if voice.process.try_wait().map_err(io_message)?.is_none() {
                return Ok(());
            }
        }
        state.voice = None;
        let script = r#"
Add-Type -AssemblyName System.Speech
$recognizer = New-Object System.Speech.Recognition.SpeechRecognitionEngine
$recognizer.SetInputToDefaultAudioDevice()
$choices = New-Object System.Speech.Recognition.Choices
$choices.Add('Courage have it')
$builder = New-Object System.Speech.Recognition.GrammarBuilder
$builder.Append($choices)
$grammar = New-Object System.Speech.Recognition.Grammar -ArgumentList $builder
$recognizer.LoadGrammar($grammar)
while ($true) {
  $result = $recognizer.Recognize()
  if ($result -and $result.Text -match '(?i)courage\s+have\s+it') {
    [Console]::WriteLine('COURAGE_TRIGGER')
    [Console]::Out.Flush()
  }
}
"#;
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hide_console_window(&mut command);
        let mut process = command
            .spawn()
            .map_err(|error| format!("Windows voice trigger could not start: {error}"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or("Windows voice trigger did not provide an audio recognition stream")?;
        let app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.trim() != "COURAGE_TRIGGER" {
                    continue;
                }
                let result = {
                    let state = app.state::<Mutex<AppState>>();
                    state.lock().ok().and_then(|mut state| save_replay_inner(&mut state).ok())
                };
                if let Some(clip) = result {
                    notify_clip_saved(&app, &clip, "Voice trigger");
                } else {
                    let _ = app.emit(
                        "capture-error",
                        "Voice trigger heard, but the replay buffer is not ready.",
                    );
                }
            }
            let _ = app.emit("capture-error", "Windows voice trigger stopped. Check microphone privacy permissions and restart capture.");
        });
        state.voice = Some(VoiceSession { process });
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state);
        Ok(())
    }
}

fn stop_voice_listener(state: &mut AppState) {
    if let Some(mut voice) = state.voice.take() {
        let _ = voice.process.kill();
        let _ = voice.process.wait();
    }
}

fn load_settings(path: &Path) -> Result<SettingsFile, String> {
    if !path.exists() {
        return Ok(SettingsFile::default());
    }
    let data = fs::read(path).map_err(io_message)?;
    serde_json::from_slice(&data)
        .map_err(|error| format!("Could not read Courage settings: {error}"))
}

fn drain_stderr(stderr: Option<impl Read + Send + 'static>) {
    if let Some(mut stderr) = stderr {
        std::thread::spawn(move || {
            let mut sink = io::sink();
            let _ = io::copy(&mut stderr, &mut sink);
        });
    }
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_millis()
}

fn unix_timestamp_seconds() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs() as i64
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

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir().map_err(|error| error.to_string())?;
            let state = AppState::new(data_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(Mutex::new(state));

            let show = MenuItem::with_id(app, "show", "Open Courage", true, None::<&str>)?;
            let save = MenuItem::with_id(app, "save", "Save last replay", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Courage", true, None::<&str>)?;
            let menu = MenuBuilder::new(app).items(&[&show, &save, &quit]).build()?;
            TrayIconBuilder::with_id("courage-tray")
                .icon(app.default_window_icon().cloned().ok_or("Courage tray icon is missing")?)
                .tooltip("Courage · replay buffer armed")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "save" => {
                        let result = {
                            let state = app.state::<Mutex<AppState>>();
                            state
                                .lock()
                                .ok()
                                .and_then(|mut state| save_replay_inner(&mut state).ok())
                        };
                        if let Some(clip) = result {
                            notify_clip_saved(app, &clip, "Tray action");
                        }
                    }
                    "quit" => {
                        if let Ok(mut state) = app.state::<Mutex<AppState>>().lock() {
                            stop_capture_inner(&mut state);
                            stop_voice_listener(&mut state);
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            engine_status,
            get_settings,
            list_audio_devices,
            save_settings,
            start_capture,
            stop_capture,
            save_replay,
            list_clips,
            open_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running Courage");
}
