use std::fs;
use std::io::{self, Read};
#[cfg(windows)]
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
#[cfg(windows)]
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use courage_store::{ClipRecord, ClipStore};
use serde::{Deserialize, Serialize};
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

const DEFAULT_REPLAY_SECONDS: u64 = 30;
const DEFAULT_HOTKEY: &str = "CommandOrControl+Shift+1";
const DEFAULT_WAKE_PHRASE: &str = "Courage";
const DEFAULT_COMMAND_PHRASE: &str = "have it";
#[cfg(windows)]
const VOICE_COOLDOWN: Duration = Duration::from_secs(2);
#[cfg(windows)]
const VOICE_CONFIDENCE_THRESHOLD: f32 = 0.72;

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
    pub fps: u32,
    pub resolution: String,
    pub video_encoder: String,
    pub cursor_capture: bool,
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
pub struct VoiceTestResponse {
    pub normalized: String,
    pub wake_matched: bool,
    pub command_matched: bool,
    pub would_trigger: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRecognitionResult {
    pub phrase: String,
    pub confidence: f32,
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
    pub fps: u32,
    pub resolution_preset: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_quality: String,
    pub video_encoder: String,
    pub hardware_acceleration: String,
    pub cursor_capture: bool,
    pub audio_enabled: bool,
    pub audio_sample_rate: u32,
    pub audio_channels: u8,
    pub capture_source: String,
    pub voice_wake_phrase: String,
    pub voice_command_phrase: String,
    pub voice_wake_only: bool,
    pub theme: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub output_dir: String,
    pub selected_audio_device: Option<String>,
    pub hotkey: String,
    pub replay_seconds: u64,
    pub voice_enabled: bool,
    pub fps: u32,
    pub resolution_preset: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_quality: String,
    pub video_encoder: String,
    pub hardware_acceleration: String,
    pub cursor_capture: bool,
    pub audio_enabled: bool,
    pub audio_sample_rate: u32,
    pub audio_channels: u8,
    pub capture_source: String,
    pub voice_wake_phrase: String,
    pub voice_command_phrase: String,
    pub voice_wake_only: bool,
    pub theme: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SettingsFile {
    output_dir: Option<String>,
    selected_audio_device: Option<String>,
    hotkey: String,
    replay_seconds: u64,
    voice_enabled: bool,
    fps: u32,
    resolution_preset: String,
    width: Option<u32>,
    height: Option<u32>,
    video_quality: String,
    video_encoder: String,
    hardware_acceleration: String,
    cursor_capture: bool,
    audio_enabled: bool,
    audio_sample_rate: u32,
    audio_channels: u8,
    capture_source: String,
    voice_wake_phrase: String,
    voice_command_phrase: String,
    voice_wake_only: bool,
    theme: String,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            output_dir: None,
            selected_audio_device: None,
            hotkey: DEFAULT_HOTKEY.to_owned(),
            replay_seconds: DEFAULT_REPLAY_SECONDS,
            voice_enabled: true,
            fps: 30,
            resolution_preset: "native".to_owned(),
            width: None,
            height: None,
            video_quality: "balanced".to_owned(),
            video_encoder: "auto".to_owned(),
            hardware_acceleration: "auto".to_owned(),
            cursor_capture: true,
            audio_enabled: true,
            audio_sample_rate: 48_000,
            audio_channels: 2,
            capture_source: "desktop".to_owned(),
            voice_wake_phrase: DEFAULT_WAKE_PHRASE.to_owned(),
            voice_command_phrase: DEFAULT_COMMAND_PHRASE.to_owned(),
            voice_wake_only: false,
            theme: "dark".to_owned(),
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
    #[cfg(windows)]
    last_voice_save: Option<Instant>,
}

struct CaptureSession {
    process: Child,
    segments_dir: PathBuf,
    backend: String,
    audio_enabled: bool,
    encoder: String,
    fps: u32,
    resolution: String,
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
            #[cfg(windows)]
            last_voice_save: None,
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
        match fs::rename(&temporary, &self.settings_path) {
            Ok(()) => Ok(()),
            Err(_error) if self.settings_path.exists() => {
                let backup = self.settings_path.with_extension("json.bak");
                let _ = fs::remove_file(&backup);
                fs::rename(&self.settings_path, &backup).map_err(io_message)?;
                match fs::rename(&temporary, &self.settings_path) {
                    Ok(()) => {
                        let _ = fs::remove_file(backup);
                        Ok(())
                    }
                    Err(replace_error) => {
                        let _ = fs::remove_file(&self.settings_path);
                        if let Err(restore_error) = fs::rename(&backup, &self.settings_path) {
                            return Err(format!(
                                "Could not replace settings ({replace_error}); restoring the previous settings also failed ({restore_error})."
                            ));
                        }
                        Err(io_message(replace_error))
                    }
                }
            }
            Err(error) => {
                let _ = fs::remove_file(temporary);
                Err(io_message(error))
            }
        }
    }
}

#[tauri::command]
fn engine_status(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<EngineStatus, String> {
    let mut state = lock_state(&state)?;
    let running = refresh_capture_status(&mut state);
    match reconcile_library(&mut state) {
        Ok(true) => {
            let _ = app.emit("library-changed", ());
        }
        Ok(false) => {}
        Err(error) => {
            let _ = app.emit("library-error", error);
        }
    }
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
fn test_voice_phrase(
    phrase: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<VoiceTestResponse, String> {
    let state = lock_state(&state)?;
    let normalized = normalize_phrase(&phrase);
    let wake = normalize_phrase(&state.settings.voice_wake_phrase);
    let command = normalize_phrase(&state.settings.voice_command_phrase);
    let wake_matched = normalized == wake || normalized.starts_with(&(wake.clone() + " "));
    let command_matched =
        normalized == command || normalized.ends_with(&(" ".to_owned() + &command));
    Ok(VoiceTestResponse {
        normalized: normalized.clone(),
        wake_matched,
        command_matched,
        would_trigger: voice_would_trigger(
            &normalized,
            &wake,
            &command,
            state.settings.voice_wake_only,
        ),
    })
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
    validate_settings(&settings)?;
    fs::create_dir_all(&output_dir).map_err(io_message)?;
    state.settings = SettingsFile {
        output_dir: Some(output_dir.to_string_lossy().into_owned()),
        selected_audio_device: settings.selected_audio_device,
        hotkey: settings.hotkey,
        replay_seconds: settings.replay_seconds.clamp(5, 120),
        voice_enabled: settings.voice_enabled,
        fps: settings.fps,
        resolution_preset: settings.resolution_preset,
        width: settings.width,
        height: settings.height,
        video_quality: settings.video_quality,
        video_encoder: settings.video_encoder,
        hardware_acceleration: settings.hardware_acceleration,
        cursor_capture: settings.cursor_capture,
        audio_enabled: settings.audio_enabled,
        audio_sample_rate: settings.audio_sample_rate,
        audio_channels: settings.audio_channels,
        capture_source: settings.capture_source,
        voice_wake_phrase: settings.voice_wake_phrase,
        voice_command_phrase: settings.voice_command_phrase,
        voice_wake_only: settings.voice_wake_only,
        theme: settings.theme,
    };
    state.persist_settings()?;
    diagnostic_event("settings.applied", "capture and UI settings persisted");
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
    validate_settings_file(&state.settings)?;
    if state.settings.audio_enabled {
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
    }
    let segments_dir = new_session_dir(&state.data_dir)?;
    let (mut command, backend, audio_enabled, encoder, fps, resolution) =
        match build_capture_command(&state, &segments_dir) {
            Ok(command) => command,
            Err(error) => {
                let _ = fs::remove_dir_all(&segments_dir);
                return Err(error);
            }
        };
    diagnostic_event(
        "capture.starting",
        &format!("backend={backend} fps={fps} resolution={resolution} encoder={encoder}"),
    );
    hide_console_window(&mut command);
    let mut process = match command.spawn() {
        Ok(process) => process,
        Err(error) => {
            let _ = fs::remove_dir_all(&segments_dir);
            diagnostic_event("capture.start_failed", &format!("FFmpeg spawn failed: {error}"));
            return Err(
                "Capture could not start. Verify that FFmpeg is installed and available on PATH."
                    .to_owned(),
            );
        }
    };
    let process_status = match process.try_wait() {
        Ok(status) => status,
        Err(error) => {
            let _ = process.kill();
            let _ = process.wait();
            let _ = fs::remove_dir_all(&segments_dir);
            diagnostic_event(
                "capture.start_failed",
                &format!("FFmpeg status check failed: {error}"),
            );
            return Err("Capture could not be initialized. Check the display and audio settings."
                .to_owned());
        }
    };
    if let Some(status) = process_status {
        let mut details = String::new();
        if let Some(mut stderr) = process.stderr.take() {
            let _ = stderr.read_to_string(&mut details);
        }
        let _ = fs::remove_dir_all(&segments_dir);
        let details = details.trim();
        diagnostic_event(
            "capture.start_failed",
            &format!("FFmpeg exited immediately with {status}: {details}"),
        );
        return Err(
            "Capture stopped immediately. Check the selected display, microphone, and FFmpeg settings."
                .to_owned(),
        );
    }
    drain_stderr(process.stderr.take());
    state.capture = Some(CaptureSession {
        process,
        segments_dir,
        backend,
        audio_enabled,
        encoder,
        fps,
        resolution,
    });
    diagnostic_event("capture.started", "FFmpeg process is running");
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
    let result = stop_capture_inner(&mut state);
    stop_voice_listener(&mut state);
    result?;
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
    let mut state = lock_state(&state)?;
    reconcile_library(&mut state)?;
    state
        .store
        .list_clips()
        .map_err(|error| error.to_string())
        .map(|clips| clips.into_iter().map(to_response).collect())
}

#[tauri::command]
fn delete_clip(app: AppHandle, id: i64, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let state = lock_state(&state)?;
    let record = state
        .store
        .list_clips()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|clip| clip.id == id)
        .ok_or("Clip was not found in the local library")?;
    if record.path.exists() {
        fs::remove_file(&record.path)
            .map_err(|error| format!("Could not delete clip {}: {error}", record.path.display()))?;
    }
    state.store.delete_clip(id).map_err(|error| error.to_string())?;
    diagnostic_event("library.clip_deleted", &format!("id={id}"));
    let _ = app.emit("library-changed", ());
    Ok(())
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
        fps: state.settings.fps,
        resolution_preset: state.settings.resolution_preset.clone(),
        width: state.settings.width,
        height: state.settings.height,
        video_quality: state.settings.video_quality.clone(),
        video_encoder: state.settings.video_encoder.clone(),
        hardware_acceleration: state.settings.hardware_acceleration.clone(),
        cursor_capture: state.settings.cursor_capture,
        audio_enabled: state.settings.audio_enabled,
        audio_sample_rate: state.settings.audio_sample_rate,
        audio_channels: state.settings.audio_channels,
        capture_source: state.settings.capture_source.clone(),
        voice_wake_phrase: state.settings.voice_wake_phrase.clone(),
        voice_command_phrase: state.settings.voice_command_phrase.clone(),
        voice_wake_only: state.settings.voice_wake_only,
        theme: state.settings.theme.clone(),
    })
}

fn reconcile_library(state: &mut AppState) -> Result<bool, String> {
    let mut changed = false;
    for record in state.store.list_clips().map_err(|error| error.to_string())? {
        if !record.path.is_file() {
            changed |= state.store.delete_clip(record.id).map_err(|error| error.to_string())?;
        }
    }
    let output_dir = state.output_dir();
    for entry in fs::read_dir(&output_dir).map_err(|error| {
        format!("Could not reconcile clip folder {}: {error}", output_dir.display())
    })? {
        let entry = entry.map_err(io_message)?;
        let path = entry.path();
        let is_temporary = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".tmp."));
        if !is_temporary
            && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
            && is_stable_clip(&path)
            && !state.store.contains_path(&path).map_err(|error| error.to_string())?
        {
            let duration_ms = probe_duration_ms(&path).unwrap_or(0);
            state.store.insert_clip(&path, duration_ms, None).map_err(|error| error.to_string())?;
            changed = true;
        }
    }
    Ok(changed)
}

fn is_stable_clip(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else { return false };
    if metadata.len() == 0 {
        return false;
    }
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_none_or(|age| age >= Duration::from_secs(2))
}

fn validate_settings(settings: &SettingsInput) -> Result<(), String> {
    validate_capture_values(CaptureValidation {
        replay_seconds: settings.replay_seconds,
        fps: settings.fps,
        resolution_preset: &settings.resolution_preset,
        width: settings.width,
        height: settings.height,
        quality: &settings.video_quality,
        encoder: &settings.video_encoder,
        hardware: &settings.hardware_acceleration,
        sample_rate: settings.audio_sample_rate,
        channels: settings.audio_channels,
        source: &settings.capture_source,
        theme: &settings.theme,
        wake_phrase: &settings.voice_wake_phrase,
        command_phrase: &settings.voice_command_phrase,
    })
}

fn validate_settings_file(settings: &SettingsFile) -> Result<(), String> {
    validate_capture_values(CaptureValidation {
        replay_seconds: settings.replay_seconds,
        fps: settings.fps,
        resolution_preset: &settings.resolution_preset,
        width: settings.width,
        height: settings.height,
        quality: &settings.video_quality,
        encoder: &settings.video_encoder,
        hardware: &settings.hardware_acceleration,
        sample_rate: settings.audio_sample_rate,
        channels: settings.audio_channels,
        source: &settings.capture_source,
        theme: &settings.theme,
        wake_phrase: &settings.voice_wake_phrase,
        command_phrase: &settings.voice_command_phrase,
    })
}

struct CaptureValidation<'a> {
    replay_seconds: u64,
    fps: u32,
    resolution_preset: &'a str,
    width: Option<u32>,
    height: Option<u32>,
    quality: &'a str,
    encoder: &'a str,
    hardware: &'a str,
    sample_rate: u32,
    channels: u8,
    source: &'a str,
    theme: &'a str,
    wake_phrase: &'a str,
    command_phrase: &'a str,
}

fn validate_capture_values(values: CaptureValidation<'_>) -> Result<(), String> {
    if !(15..=120).contains(&values.fps) {
        return Err("Frame rate must be between 15 and 120 FPS.".to_owned());
    }
    if !(5..=120).contains(&values.replay_seconds) {
        return Err("Replay duration must be between 5 and 120 seconds.".to_owned());
    }
    if !["native", "1920x1080", "1600x900", "1280x720", "custom"]
        .contains(&values.resolution_preset)
    {
        return Err("Unsupported capture resolution preset.".to_owned());
    }
    if values.resolution_preset == "custom" {
        let (Some(width), Some(height)) = (values.width, values.height) else {
            return Err("Custom resolution requires both width and height.".to_owned());
        };
        if !(320..=7680).contains(&width) || !(240..=4320).contains(&height) {
            return Err("Custom resolution must be between 320x240 and 7680x4320.".to_owned());
        }
    }
    if !["performance", "balanced", "quality"].contains(&values.quality) {
        return Err("Unsupported video quality preset.".to_owned());
    }
    if !["auto", "libx264", "h264_nvenc", "h264_amf", "h264_qsv"].contains(&values.encoder) {
        return Err("Unsupported video encoder preference.".to_owned());
    }
    if !["auto", "libx264", "h264_nvenc", "h264_amf", "h264_qsv"].contains(&values.hardware) {
        return Err("Unsupported hardware encoder preference.".to_owned());
    }
    if ![44_100, 48_000].contains(&values.sample_rate) || ![1, 2].contains(&values.channels) {
        return Err("Audio must use 44.1/48 kHz and one or two channels.".to_owned());
    }
    if values.source != "desktop" {
        return Err("Only full-desktop capture is supported by this backend.".to_owned());
    }
    if !["light", "dark", "rusty"].contains(&values.theme) {
        return Err("Theme must be light, dark, or rusty.".to_owned());
    }
    if normalize_phrase(values.wake_phrase).is_empty()
        || normalize_phrase(values.command_phrase).is_empty()
    {
        return Err("Voice wake and command phrases cannot be empty.".to_owned());
    }
    if values.wake_phrase.chars().count() > 80 || values.command_phrase.chars().count() > 80 {
        return Err("Voice wake and command phrases must be 80 characters or fewer.".to_owned());
    }
    Ok(())
}

fn resolution_label(settings: &SettingsFile) -> String {
    match settings.resolution_preset.as_str() {
        "native" => "Native display".to_owned(),
        "custom" => match (settings.width, settings.height) {
            (Some(width), Some(height)) => format!("{width}×{height}"),
            _ => "Custom (invalid)".to_owned(),
        },
        preset => preset.replace('x', "×"),
    }
}

fn status_for(state: &AppState, requested_state: &str) -> EngineStatus {
    let (backend, fps, resolution, video_encoder) = state
        .capture
        .as_ref()
        .map(|capture| {
            (
                format!(
                    "{}{}",
                    capture.backend,
                    if capture.audio_enabled { " + microphone" } else { " (video only)" }
                ),
                capture.fps,
                capture.resolution.clone(),
                capture.encoder.clone(),
            )
        })
        .unwrap_or_else(|| {
            (
                capture_backend_name().unwrap_or_else(|error| error),
                state.settings.fps,
                resolution_label(&state.settings),
                state.settings.video_encoder.clone(),
            )
        });
    EngineStatus {
        state: requested_state.to_owned(),
        platform: std::env::consts::OS.to_owned(),
        backend,
        audio_device: state.settings.selected_audio_device.clone(),
        voice_trigger: voice_trigger_status(state),
        output_dir: state.output_dir().to_string_lossy().into_owned(),
        replay_seconds: state.settings.replay_seconds,
        fps,
        resolution,
        video_encoder,
        cursor_capture: state.settings.cursor_capture,
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
    let diagnostic = match capture.process.try_wait() {
        Ok(None) => return true,
        Ok(Some(status)) => format!("FFmpeg exited with {status}"),
        Err(error) => format!("Could not poll FFmpeg capture: {error}"),
    };
    if let Some(mut capture) = state.capture.take() {
        let _ = capture.process.wait();
        let _ = fs::remove_dir_all(capture.segments_dir);
    }
    stop_voice_listener(state);
    diagnostic_event("capture.ended_unexpectedly", &diagnostic);
    false
}

fn stop_capture_inner(state: &mut AppState) -> Result<(), String> {
    if let Some(mut capture) = state.capture.take() {
        let mut failure = None;
        if let Err(error) = capture.process.kill() {
            if !matches!(error.kind(), io::ErrorKind::InvalidInput | io::ErrorKind::NotFound) {
                failure = Some(format!("Could not stop capture process: {error}"));
            }
        }
        if let Err(error) = capture.process.wait() {
            failure.get_or_insert_with(|| format!("Could not reap capture process: {error}"));
        }
        if let Err(error) = fs::remove_dir_all(&capture.segments_dir) {
            failure.get_or_insert_with(|| {
                format!("Capture stopped, but its temporary session could not be removed: {error}")
            });
        }
        if let Some(failure) = failure {
            diagnostic_event("capture.stop_failed", &failure);
            return Err(
                "Capture stopped, but temporary resources could not be fully released. Close any player using a clip and try again."
                    .to_owned(),
            );
        }
        diagnostic_event("capture.stopped", "FFmpeg process reaped and session removed");
    }
    Ok(())
}

fn build_capture_command(
    state: &AppState,
    segments_dir: &Path,
) -> Result<(Command, String, bool, String, u32, String), String> {
    let segments = segments_dir.join("segment-%05d.mp4");
    let wrap = (state.settings.replay_seconds + 15).clamp(30, 150);
    let mut command = Command::new("ffmpeg");
    command.args(["-hide_banner", "-loglevel", "error", "-y"]);
    let fps = state.settings.fps;
    command.args(video_input_args(state)?.as_slice());
    let audio_args = audio_input_args(state);
    let audio_enabled = audio_args.is_some();
    if let Some(args) = audio_args {
        command.args(args.as_slice());
    }
    command.args(["-map", "0:v:0"]);
    if audio_enabled {
        command.args([
            "-map",
            "1:a:0",
            "-c:a",
            "aac",
            "-ar",
            &state.settings.audio_sample_rate.to_string(),
            "-ac",
            &state.settings.audio_channels.to_string(),
            "-b:a",
            "160k",
            "-af",
            "aresample=async=1:first_pts=0",
        ]);
    }
    if let Some((width, height)) = capture_dimensions(&state.settings) {
        command.args(["-vf", &format!("scale={width}:{height}")]);
    }
    let encoder = resolve_video_encoder(
        &state.settings.video_encoder,
        &state.settings.hardware_acceleration,
    )?;
    let (preset, crf) = video_quality_args(&state.settings.video_quality)?;
    command.args(["-c:v", &encoder]);
    match encoder.as_str() {
        "libx264" => command.args(["-preset", preset, "-tune", "zerolatency", "-crf", crf]),
        "h264_nvenc" => command.args(["-preset", "p4", "-rc", "vbr", "-cq", crf]),
        "h264_amf" => {
            command.args(["-quality", "balanced", "-rc", "cqp", "-qp_i", crf, "-qp_p", crf])
        }
        "h264_qsv" => command.args(["-preset", "medium", "-global_quality", crf]),
        _ => return Err(format!("Unsupported resolved encoder: {encoder}")),
    };
    command
        .args([
            "-pix_fmt",
            "yuv420p",
            "-r",
            &fps.to_string(),
            "-g",
            &fps.to_string(),
            "-keyint_min",
            &fps.to_string(),
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
    Ok((
        command,
        capture_backend_name()?,
        audio_enabled,
        encoder,
        fps,
        resolution_label(&state.settings),
    ))
}

fn video_input_args(state: &AppState) -> Result<Vec<String>, String> {
    let fps = state.settings.fps.to_string();
    match std::env::consts::OS {
        "windows" => {
            let mut args = vec!["-f".into(), "gdigrab".into(), "-framerate".into(), fps];
            args.extend([
                "-draw_mouse".into(),
                if state.settings.cursor_capture { "1" } else { "0" }.into(),
                "-i".into(),
                "desktop".into(),
            ]);
            Ok(args)
        }
        "macos" => {
            let mut args = vec!["-f".into(), "avfoundation".into(), "-framerate".into(), fps];
            args.extend([
                "-i".into(),
                std::env::var("COURAGE_VIDEO_DEVICE").unwrap_or_else(|_| "1:none".into()),
            ]);
            Ok(args)
        }
        "linux" => {
            let display = std::env::var("DISPLAY")
                .map_err(|_| "Linux capture requires an X11 DISPLAY. Wayland/PipeWire capture is not connected yet.".to_owned())?;
            let mut args = vec!["-f".into(), "x11grab".into(), "-framerate".into(), fps];
            args.extend(["-i".into(), display]);
            Ok(args)
        }
        platform => Err(format!("Unsupported capture platform: {platform}")),
    }
}

fn capture_dimensions(settings: &SettingsFile) -> Option<(u32, u32)> {
    match settings.resolution_preset.as_str() {
        "1920x1080" => Some((1920, 1080)),
        "1600x900" => Some((1600, 900)),
        "1280x720" => Some((1280, 720)),
        "custom" => settings.width.zip(settings.height),
        _ => None,
    }
}

fn audio_input_args(state: &AppState) -> Option<Vec<String>> {
    if !state.settings.audio_enabled {
        return None;
    }
    let device = state.settings.selected_audio_device.as_deref()?;
    Some(match std::env::consts::OS {
        "windows" => vec!["-f".into(), "dshow".into(), "-i".into(), format!("audio={device}")],
        "macos" => vec!["-f".into(), "avfoundation".into(), "-i".into(), format!("none:{device}")],
        "linux" => vec!["-f".into(), "pulse".into(), "-i".into(), device.to_owned()],
        _ => return None,
    })
}

fn video_quality_args(quality: &str) -> Result<(&'static str, &'static str), String> {
    match quality {
        "performance" => Ok(("ultrafast", "28")),
        "balanced" => Ok(("veryfast", "23")),
        "quality" => Ok(("medium", "18")),
        _ => Err(format!("Unsupported video quality preset: {quality}")),
    }
}

fn resolve_video_encoder(preference: &str, hardware: &str) -> Result<String, String> {
    let requested = if preference == "auto" { hardware } else { preference };
    let candidates = match requested {
        "auto" => ["h264_nvenc", "h264_amf", "h264_qsv", "libx264"].as_slice(),
        "libx264" => ["libx264"].as_slice(),
        "h264_nvenc" => ["h264_nvenc", "libx264"].as_slice(),
        "h264_amf" => ["h264_amf", "libx264"].as_slice(),
        "h264_qsv" => ["h264_qsv", "libx264"].as_slice(),
        _ => return Err(format!("Unsupported video encoder preference: {preference}")),
    };
    let mut probe = Command::new("ffmpeg");
    probe.args(["-hide_banner", "-encoders"]);
    hide_console_window(&mut probe);
    let encoders =
        probe.output().map_err(|error| format!("Could not inspect FFmpeg encoders: {error}"))?;
    let text = String::from_utf8_lossy(&encoders.stdout);
    Ok(candidates
        .iter()
        .filter(|candidate| {
            text.lines().any(|line| line.split_whitespace().any(|field| field == **candidate))
        })
        .find(|candidate| encoder_works(candidate))
        .unwrap_or(&"libx264")
        .to_string())
}

fn encoder_works(encoder: &&str) -> bool {
    if *encoder == "libx264" {
        return true;
    }
    let mut command = Command::new("ffmpeg");
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=16x16:r=1",
        "-frames:v",
        "1",
        "-c:v",
        encoder,
        "-f",
        "null",
        "-",
    ]);
    hide_console_window(&mut command);
    command.output().is_ok_and(|output| output.status.success())
}

fn new_session_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let root = data_dir.join("rolling-buffer");
    fs::create_dir_all(&root).map_err(io_message)?;
    for suffix in 0..100 {
        let session = root.join(format!("session-{}-{suffix}", unix_timestamp_millis()));
        match fs::create_dir(&session) {
            Ok(()) => return Ok(session),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Could not allocate a unique capture session directory.".to_owned())
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
    diagnostic_event("library.clip_saved", &format!("id={id} bytes={size}"));
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
    let result = match command.output() {
        Ok(result) => result,
        Err(error) => {
            let _ = fs::remove_file(&list_path);
            let _ = fs::remove_file(&temporary);
            diagnostic_event("library.save_failed", &format!("FFmpeg spawn failed: {error}"));
            return Err(
                "Could not start the clip exporter. Verify that FFmpeg is available.".to_owned()
            );
        }
    };
    let _ = fs::remove_file(&list_path);
    if !result.status.success() {
        let _ = fs::remove_file(&temporary);
        diagnostic_event(
            "library.save_failed",
            &format!("FFmpeg export failed: {}", String::from_utf8_lossy(&result.stderr).trim()),
        );
        return Err(
            "Could not save the replay. The buffer may still be warming up; try again.".to_owned()
        );
    }
    if let Err(error) = fs::rename(&temporary, output) {
        let _ = fs::remove_file(&temporary);
        diagnostic_event("library.save_failed", &format!("clip rename failed: {error}"));
        return Err("The clip was encoded but could not be moved into the clip folder.".to_owned());
    }
    Ok(())
}

fn probe_duration_ms(path: &Path) -> Option<i64> {
    let mut command = Command::new("ffprobe");
    command.args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(path);
    hide_console_window(&mut command);
    let output = command.output().ok()?;
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

fn normalize_phrase(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_alphanumeric() {
            normalized.extend(character.to_lowercase());
        } else if !normalized.ends_with(' ') {
            normalized.push(' ');
        }
    }
    normalized.trim().to_owned()
}

fn voice_would_trigger(heard: &str, wake: &str, command: &str, wake_only: bool) -> bool {
    if wake_only {
        heard == wake
    } else {
        heard == format!("{wake} {command}")
    }
}

#[cfg(windows)]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(windows)]
fn handle_voice_trigger(app: &AppHandle) -> Option<Result<ClipResponse, String>> {
    let state = app.state::<Mutex<AppState>>();
    let mut state = state.lock().ok()?;
    if state.last_voice_save.is_some_and(|last| last.elapsed() < VOICE_COOLDOWN) {
        return None;
    }
    let result = save_replay_inner(&mut state);
    if result.is_ok() {
        state.last_voice_save = Some(Instant::now());
    }
    Some(result)
}

#[cfg(windows)]
fn parse_voice_result(line: &str) -> Option<VoiceRecognitionResult> {
    let mut fields = line.strip_prefix("VOICE_RESULT\t")?.splitn(2, '\t');
    let phrase = fields.next()?.to_owned();
    let confidence = fields.next()?.parse::<f32>().ok()?;
    Some(VoiceRecognitionResult { phrase, confidence })
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
        let wake_phrase = normalize_phrase(&state.settings.voice_wake_phrase);
        let command_phrase = normalize_phrase(&state.settings.voice_command_phrase);
        let expected_phrase = if state.settings.voice_wake_only {
            wake_phrase.clone()
        } else {
            format!("{wake_phrase} {command_phrase}")
        };
        let expected_phrase = powershell_quote(&expected_phrase);
        let script = format!(
            r#"
Add-Type -AssemblyName System.Speech
$recognizer = New-Object System.Speech.Recognition.SpeechRecognitionEngine
# System.Speech binds to the Windows default input device. The selected
# recording device remains authoritative for FFmpeg capture.
$recognizer.SetInputToDefaultAudioDevice()
$grammar = New-Object System.Speech.Recognition.DictationGrammar
$recognizer.LoadGrammar($grammar)
while ($true) {{
  $result = $recognizer.Recognize()
  if ($result) {{
    $normalized = (($result.Text.ToLowerInvariant() -replace '[^\p{{L}}\p{{Nd}}]+', ' ') -replace '\s+', ' ').Trim()
    $confidence = $result.Confidence.ToString([Globalization.CultureInfo]::InvariantCulture)
    [Console]::WriteLine("VOICE_RESULT`t$normalized`t$confidence")
    [Console]::Out.Flush()
  }}
  if ($result -and $normalized -eq {expected_phrase} -and $result.Confidence -ge {confidence_threshold}) {{
    [Console]::WriteLine('COURAGE_TRIGGER')
    [Console]::Out.Flush()
  }}
}}
"#,
            expected_phrase = expected_phrase,
            confidence_threshold = VOICE_CONFIDENCE_THRESHOLD,
        );
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        hide_console_window(&mut command);
        let mut process = command
            .spawn()
            .map_err(|error| format!("Windows voice trigger could not start: {error}"))?;
        let stdout = match process.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = process.kill();
                let _ = process.wait();
                return Err(
                    "Windows voice trigger did not provide an audio recognition stream".to_owned()
                );
            }
        };
        let app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(result) = parse_voice_result(&line) {
                    let _ = app.emit("voice-result", result);
                    continue;
                }
                if line.trim() != "COURAGE_TRIGGER" {
                    continue;
                }
                if let Some(result) = handle_voice_trigger(&app) {
                    match result {
                        Ok(clip) => notify_clip_saved(&app, &clip, "Voice trigger"),
                        Err(error) => {
                            let _ = app.emit("capture-error", error);
                        }
                    }
                }
            }
            let should_report = app
                .state::<Mutex<AppState>>()
                .lock()
                .map(|state| state.capture.is_some() && state.voice.is_some())
                .unwrap_or(false);
            if should_report {
                let _ = app.emit(
                    "capture-error",
                    "Windows voice trigger stopped. Check microphone privacy permissions and restart capture.",
                );
            }
        });
        state.voice = Some(VoiceSession { process });
        Ok(())
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

fn diagnostic_event(event: &str, details: &str) {
    eprintln!(
        "{}",
        serde_json::json!({
            "event": event,
            "details": details,
            "timestamp": unix_timestamp_seconds(),
        })
    );
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

#[cfg(windows)]
fn disable_windows_browser_surface(app: &AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview_window("main")
        .ok_or("Courage main WebView was not available during setup")?;
    let failure = std::sync::Arc::new(Mutex::new(None::<String>));
    let callback_failure = std::sync::Arc::clone(&failure);
    webview
        .with_webview(move |webview| {
            let result = unsafe {
                webview
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.Settings())
                    .and_then(|settings| settings.SetAreDefaultContextMenusEnabled(false))
            };
            if let Err(error) = result {
                if let Ok(mut failure) = callback_failure.lock() {
                    *failure = Some(format!("Could not disable WebView2 context menus: {error}"));
                }
            }
        })
        .map_err(|error| format!("Could not configure the Courage WebView: {error}"))?;
    if let Some(error) = failure.lock().map_err(|_| "WebView setup state is unavailable")?.take() {
        return Err(error);
    }
    Ok(())
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

            #[cfg(windows)]
            disable_windows_browser_surface(app.handle())
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;

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
                            let _ = stop_capture_inner(&mut state);
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
            test_voice_phrase,
            save_settings,
            start_capture,
            stop_capture,
            save_replay,
            list_clips,
            delete_clip,
            open_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running Courage");
}

#[cfg(test)]
mod tests {
    use super::{normalize_phrase, voice_would_trigger, SettingsFile};

    #[test]
    fn normalizes_voice_punctuation_and_case() {
        assert_eq!(normalize_phrase(" Courage, SAVE-that! "), "courage save that");
    }

    #[test]
    fn normalizes_unicode_letters() {
        assert_eq!(normalize_phrase("Écho — Café"), "écho café");
    }

    #[test]
    fn wake_phrase_alone_requires_explicit_wake_only_mode() {
        assert!(!voice_would_trigger("courage", "courage", "save that", false));
        assert!(voice_would_trigger("courage", "courage", "save that", true));
        assert!(voice_would_trigger("courage save that", "courage", "save that", false));
    }

    #[test]
    fn old_settings_get_new_capture_defaults() {
        let settings: SettingsFile = serde_json::from_str(
            r#"{
                "hotkey": "Alt+F10",
                "replaySeconds": 45,
                "voiceEnabled": false
            }"#,
        )
        .expect("legacy settings");
        assert_eq!(settings.hotkey, "Alt+F10");
        assert_eq!(settings.replay_seconds, 45);
        assert!(!settings.voice_enabled);
        assert_eq!(settings.fps, 30);
        assert_eq!(settings.resolution_preset, "native");
        assert_eq!(settings.theme, "dark");
    }
}
