use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod capture;
mod process;
mod voice;
#[cfg(windows)]
mod windows_process;
use capture::{CaptureProcess, CaptureState};
use process::ManagedChild;
use voice::{
    Diagnostics as VoiceDiagnostics, Matcher, RecognitionResult, SpeechEvent, VoiceConfig,
};

use courage_store::{ClipRecord, ClipStore};
use serde::{Deserialize, Serialize};
use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

const DEFAULT_REPLAY_SECONDS: u64 = 30;
const DEFAULT_HOTKEY: &str = "CommandOrControl+Shift+1";
const DEFAULT_WAKE_PHRASE: &str = "Courage";
const DEFAULT_COMMAND_PHRASE: &str = "have it";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub revision: u64,
    pub state: CaptureState,
    pub session_id: Option<String>,
    pub pid: Option<u32>,
    pub session_directory: Option<String>,
    pub error: Option<String>,
    pub platform: String,
    pub backend: String,
    pub audio_device: Option<String>,
    pub voice_trigger: String,
    pub voice: VoiceDiagnostics,
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
    pub voice_locale: String,
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
    pub voice_locale: String,
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
    voice_locale: String,
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
            voice_locale: "en-US".into(),
            theme: "dark".to_owned(),
        }
    }
}

// ponytail: one mutex serializes bounded process/file work. Slow storage can still
// delay status; move library I/O to a separate owner if measurements require it.
struct AppState {
    data_dir: PathBuf,
    default_output_dir: PathBuf,
    settings_path: PathBuf,
    settings: SettingsFile,
    store: ClipStore,
    control: Arc<Control>,
    error: Option<String>,
    capture: Option<CaptureSession>,
    voice: Option<VoiceSession>,
    voice_diagnostics: VoiceDiagnostics,
}

struct CaptureSession {
    owner: CaptureProcess,
    backend: String,
    audio_enabled: bool,
    encoder: String,
    fps: u32,
    resolution: String,
}

struct VoiceSession {
    process: ManagedChild,
    config: VoiceConfig,
    generation: String,
    capture_session: Option<String>,
    test_mode: bool,
    matcher: Matcher,
    deadline: Instant,
    ready: bool,
    test_until: Option<Instant>,
}

#[derive(Default)]
struct Control {
    revision: AtomicU64,
    stop_epoch: AtomicU64,
    exiting: AtomicBool,
    exit_complete: AtomicBool,
    stop_requested: AtomicBool,
    operation: AtomicBool,
    stops: AtomicUsize,
}

impl Control {
    fn prepare_start(&self, requested_epoch: u64) -> Result<(), String> {
        let cancelled = || {
            self.exiting.load(Ordering::Acquire)
                || self.stops.load(Ordering::Acquire) > 0
                || self.stop_epoch.load(Ordering::Acquire) != requested_epoch
        };
        if cancelled() {
            return Err("Start cancelled by a later stop/exit".into());
        }
        self.stop_requested.store(false, Ordering::Release);
        // Stop can arrive while we clear the previous session's cancellation flag.
        if cancelled() {
            self.stop_requested.store(true, Ordering::Release);
            return Err("Start cancelled by a later stop/exit".into());
        }
        Ok(())
    }
}

struct Operation(Arc<Control>);
impl Operation {
    fn acquire(control: &Arc<Control>) -> Result<Self, String> {
        if control.exiting.load(Ordering::Acquire) || control.stops.load(Ordering::Acquire) > 0 {
            return Err("Courage is stopping".into());
        }
        if control.operation.swap(true, Ordering::AcqRel) {
            return Err("A capture operation is already in progress".into());
        }
        Ok(Self(control.clone()))
    }
}
struct StopRequest(Arc<Control>);
impl StopRequest {
    fn new(control: &Arc<Control>) -> Self {
        control.stops.fetch_add(1, Ordering::AcqRel);
        control.stop_epoch.fetch_add(1, Ordering::AcqRel);
        control.stop_requested.store(true, Ordering::Release);
        Self(control.clone())
    }
}
impl Drop for StopRequest {
    fn drop(&mut self) {
        self.0.stops.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.0.operation.store(false, Ordering::Release);
    }
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
        let voice_diagnostics = VoiceDiagnostics::new(&settings.voice_locale);
        Ok(Self {
            data_dir,
            default_output_dir,
            settings_path,
            settings,
            store,
            control: Arc::new(Control::default()),
            error: None,
            capture: None,
            voice: None,
            voice_diagnostics,
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
async fn engine_status(app: AppHandle) -> Result<EngineStatus, String> {
    with_state(app, |app, state| {
        refresh_capture_status(state);
        match reconcile_library(state) {
            Ok(true) => {
                let _ = app.emit("library-changed", ());
            }
            Ok(false) => {}
            Err(error) => {
                let _ = app.emit("library-error", error);
            }
        }
        Ok(status_for(state))
    })
    .await
}

#[tauri::command]
async fn get_settings(app: AppHandle) -> Result<SettingsResponse, String> {
    with_state(app, |_, state| settings_response(state)).await
}

#[tauri::command]
async fn list_audio_devices(app: AppHandle) -> Result<Vec<AudioDevice>, String> {
    with_state(app, |_, _| discover_audio_devices()).await
}

#[tauri::command]
fn test_voice_phrase(phrase: String, config: VoiceConfig) -> voice::Decision {
    Matcher::default().decide(
        &config,
        &SpeechEvent {
            kind: "result".into(),
            id: 1,
            phrase,
            confidence: Some(1.0),
            final_result: true,
            ..Default::default()
        },
        Instant::now(),
        Instant::now(),
    )
}

#[tauri::command]
async fn save_settings(
    settings: SettingsInput,
    app: AppHandle,
) -> Result<SettingsResponse, String> {
    let operation = Operation::acquire(app.state::<Arc<Control>>().inner())?;
    with_state(app, move |app, state| {
        let _operation = operation;
        let output_dir = PathBuf::from(settings.output_dir.trim());
        if !output_dir.is_absolute() {
            return Err("Clip folder must be an absolute path.".to_owned());
        }
        if settings.hotkey.trim().is_empty() {
            return Err("Save hotkey cannot be empty.".to_owned());
        }
        validate_settings(&settings)?;
        fs::create_dir_all(&output_dir).map_err(io_message)?;
        let previous = state.settings.clone();
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
            voice_locale: settings.voice_locale,
            theme: settings.theme,
        };
        if let Err(error) = state.persist_settings() {
            state.settings = previous;
            return Err(error);
        }
        // Apply grammar changes at the same serialized boundary as persistence.
        if previous.voice_enabled != state.settings.voice_enabled
            || voice_config(&previous) != voice_config(&state.settings)
            || state.voice_diagnostics.test_mode
        {
            stop_voice_listener(state)?;
            if cfg!(windows)
                && state.settings.voice_enabled
                && state
                    .capture
                    .as_ref()
                    .is_some_and(|capture| capture.owner.state == CaptureState::Recording)
            {
                if let Err(error) = start_voice_listener(app, state) {
                    let _ = app.emit("capture-error", error);
                }
            }
        }
        diagnostic_event("settings.applied", "capture and UI settings persisted");
        settings_response(state)
    })
    .await
}

#[tauri::command]
async fn start_capture(app: AppHandle) -> Result<EngineStatus, String> {
    let control = app.state::<Arc<Control>>().inner().clone();
    let epoch = control.stop_epoch.load(Ordering::Acquire);
    let operation = Operation::acquire(&control)?;
    with_state(app, move |app, state| {
        let _operation = operation;
        if let Err(error) = start_capture_inner(app, state, epoch) {
            state.error = Some(error.clone());
            let _ = app.emit("engine-status", status_for(state));
            return Err(error);
        }
        Ok(status_for(state))
    })
    .await
}

fn start_capture_inner(
    app: &AppHandle,
    state: &mut AppState,
    requested_epoch: u64,
) -> Result<(), String> {
    state.control.prepare_start(requested_epoch)?;
    if refresh_capture_status(state) {
        return Ok(());
    }
    stop_voice_listener(state)?;
    stop_capture_inner(state)?;
    state.error = None;
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
            state.settings.selected_audio_device = devices
                .iter()
                .find(|device| device.is_default)
                .or_else(|| devices.first())
                .map(|device| device.id.clone());
        }
    }
    let segments_dir = new_session_dir(&state.data_dir)?;
    let (mut command, backend, audio_enabled, encoder, fps, resolution) =
        match build_capture_command(state, &segments_dir) {
            Ok(command) => command,
            Err(error) => {
                let _ = fs::remove_dir_all(&segments_dir);
                return Err(error);
            }
        };
    state.capture = Some(CaptureSession {
        owner: CaptureProcess::new(segments_dir),
        backend,
        audio_enabled,
        encoder,
        fps,
        resolution,
    });
    let _ = app.emit("engine-status", status_for(state));
    let result = state.capture.as_mut().expect("session assigned").owner.start(
        &mut command,
        &state.control.stop_requested,
        Duration::from_secs(10),
    );
    if let Err(error) = result {
        let cleanup = stop_capture_inner(state);
        state.error = Some(match cleanup {
            Ok(()) => error,
            Err(cleanup) => format!("{error}; {cleanup}"),
        });
        let _ = app.emit("engine-status", status_for(state));
        return Err(state.error.clone().unwrap_or_default());
    }
    if cfg!(windows) && state.settings.voice_enabled {
        if let Err(error) = start_voice_listener(app, state) {
            let _ = app.emit("capture-error", error);
        }
    }
    let _ = app.emit("engine-status", status_for(state));
    Ok(())
}

#[tauri::command]
async fn stop_capture(app: AppHandle) -> Result<EngineStatus, String> {
    let request = StopRequest::new(app.state::<Arc<Control>>().inner());
    with_state(app, move |app, state| {
        let _request = request;
        if let Some(capture) = &mut state.capture {
            capture.owner.state = CaptureState::Stopping;
        }
        let _ = app.emit("engine-status", status_for(state));
        let voice = stop_voice_listener(state);
        let capture = stop_capture_inner(state);
        state.error = voice.as_ref().err().or(capture.as_ref().err()).cloned();
        let _ = app.emit("engine-status", status_for(state));
        voice?;
        capture?;
        Ok(status_for(state))
    })
    .await
}

#[tauri::command]
async fn save_replay(app: AppHandle) -> Result<ClipResponse, String> {
    let operation = Operation::acquire(app.state::<Arc<Control>>().inner())?;
    with_state(app, move |app, state| {
        let _operation = operation;
        let clip = save_replay_inner(state)?;
        notify_clip_saved(app, &clip, "Replay saved");
        Ok(clip)
    })
    .await
}

async fn with_state<T: Send + 'static>(
    app: AppHandle,
    action: impl FnOnce(&AppHandle, &mut AppState) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Mutex<AppState>>();
        let mut state = state.lock().map_err(|_| "Courage state is unavailable")?;
        if state.control.exiting.load(Ordering::Acquire) {
            return Err("Courage is shutting down".into());
        }
        action(&app, &mut state)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn list_clips(app: AppHandle) -> Result<Vec<ClipResponse>, String> {
    with_state(app, |_, state| {
        reconcile_library(state)?;
        state
            .store
            .list_clips()
            .map_err(|error| error.to_string())
            .map(|clips| clips.into_iter().map(to_response).collect())
    })
    .await
}

#[tauri::command]
async fn delete_clip(app: AppHandle, id: i64) -> Result<(), String> {
    with_state(app, move |app, state| {
        let record = state
            .store
            .list_clips()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|clip| clip.id == id)
            .ok_or("Clip was not found in the local library")?;
        if record.path.exists() {
            fs::remove_file(&record.path).map_err(|error| {
                format!("Could not delete clip {}: {error}", record.path.display())
            })?;
        }
        state.store.delete_clip(id).map_err(|error| error.to_string())?;
        diagnostic_event("library.clip_deleted", &format!("id={id}"));
        let _ = app.emit("library-changed", ());
        Ok(())
    })
    .await
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
        voice_locale: state.settings.voice_locale.clone(),
        theme: state.settings.theme.clone(),
    })
}

fn reconcile_library(state: &mut AppState) -> Result<bool, String> {
    let mut changed = false;
    for record in state.store.list_clips().map_err(|error| error.to_string())? {
        if state.control.exiting.load(Ordering::Acquire) {
            return Err("Library reconciliation cancelled by exit".into());
        }
        if !record.path.is_file() {
            changed |= state.store.delete_clip(record.id).map_err(|error| error.to_string())?;
        }
    }
    let output_dir = state.output_dir();
    for entry in fs::read_dir(&output_dir).map_err(|error| {
        format!("Could not reconcile clip folder {}: {error}", output_dir.display())
    })? {
        if state.control.exiting.load(Ordering::Acquire) {
            return Err("Library reconciliation cancelled by exit".into());
        }
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
    VoiceConfig {
        wake_phrase: settings.voice_wake_phrase.clone(),
        command_phrase: settings.voice_command_phrase.clone(),
        wake_only: settings.voice_wake_only,
        locale: settings.voice_locale.clone(),
    }
    .validate()?;
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
    })
}

fn validate_settings_file(settings: &SettingsFile) -> Result<(), String> {
    voice_config(settings).validate()?;
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

fn status_for(state: &AppState) -> EngineStatus {
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
        revision: state.control.revision.fetch_add(1, Ordering::Relaxed) + 1,
        state: state.capture.as_ref().map(|capture| capture.owner.state).unwrap_or(
            if state.error.is_some() { CaptureState::Error } else { CaptureState::Stopped },
        ),
        session_id: state.capture.as_ref().map(|capture| capture.owner.session_id.clone()),
        pid: state.capture.as_ref().and_then(|capture| capture.owner.pid()),
        session_directory: state
            .capture
            .as_ref()
            .map(|capture| capture.owner.directory.to_string_lossy().into_owned()),
        error: state
            .capture
            .as_ref()
            .and_then(|capture| capture.owner.error.clone())
            .or_else(|| state.error.clone()),
        platform: std::env::consts::OS.to_owned(),
        backend,
        audio_device: state.settings.selected_audio_device.clone(),
        voice_trigger: voice_trigger_status(state),
        voice: state.voice_diagnostics.clone(),
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
    if state.voice_diagnostics.listening {
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
    if let Some(capture) = state.capture.as_mut() {
        if let Err(error) = capture.owner.poll() {
            state.error = Some(error);
            let voice = stop_voice_listener(state);
            let cleanup = stop_capture_inner(state);
            if let Err(error) = voice.and(cleanup) {
                state.error = Some(error);
            }
            return false;
        }
        return capture.owner.state == CaptureState::Recording;
    }
    false
}

fn stop_capture_inner(state: &mut AppState) -> Result<(), String> {
    if let Some(capture) = state.capture.as_mut() {
        capture.owner.stop()?;
    }
    state.capture = None;
    Ok(())
}

fn build_capture_command(
    state: &AppState,
    segments_dir: &Path,
) -> Result<(Command, String, bool, String, u32, String), String> {
    let segments = segments_dir.join("segment-%05d.mp4");
    let wrap = (state.settings.replay_seconds + 15).clamp(30, 150);
    let mut command = Command::new("ffmpeg");
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-stdin",
        "-progress",
        "pipe:1",
        "-stats_period",
        "0.1",
    ]);
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
        &state.control.stop_requested,
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
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
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

fn resolve_video_encoder(
    preference: &str,
    hardware: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
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
    let encoders = ManagedChild::output(&mut probe, Duration::from_secs(5), Some(cancel))
        .map_err(|error| format!("Could not inspect FFmpeg encoders: {error}"))?;
    let text = String::from_utf8_lossy(&encoders.stdout);
    Ok(candidates
        .iter()
        .filter(|candidate| {
            text.lines().any(|line| line.split_whitespace().any(|field| field == **candidate))
        })
        .find(|candidate| encoder_works(candidate, cancel))
        .unwrap_or(&"libx264")
        .to_string())
}

fn encoder_works(encoder: &&str, cancel: &AtomicBool) -> bool {
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
    ManagedChild::output(&mut command, Duration::from_secs(3), Some(cancel))
        .is_ok_and(|output| output.status.success())
}

fn new_session_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let root = data_dir.join("rolling-buffer");
    fs::create_dir_all(&root).map_err(io_message)?;
    for suffix in 0..100 {
        let session = root.join(format!(
            "session-{}-{}-{suffix}",
            std::process::id(),
            unix_timestamp_millis()
        ));
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
    let output = match ManagedChild::output(
        Command::new("ffmpeg").args([
            "-hide_banner",
            "-list_devices",
            "true",
            "-f",
            "dshow",
            "-i",
            "dummy",
        ]),
        Duration::from_secs(5),
        None,
    ) {
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
            // Enumeration order does not identify the Windows default speech input.
            is_default: false,
        });
    }
    Ok(devices)
}

fn discover_avfoundation_audio_devices() -> Result<Vec<AudioDevice>, String> {
    let output = match ManagedChild::output(
        Command::new("ffmpeg").args([
            "-hide_banner",
            "-f",
            "avfoundation",
            "-list_devices",
            "true",
            "-i",
            "",
        ]),
        Duration::from_secs(5),
        None,
    ) {
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
    let output = match ManagedChild::output(
        Command::new("pactl").args(["list", "short", "sources"]),
        Duration::from_secs(5),
        None,
    ) {
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
    if state.control.exiting.load(Ordering::Acquire)
        || state.control.stop_requested.load(Ordering::Acquire)
    {
        return Err("Save rejected: capture is stopping".into());
    }
    if !refresh_capture_status(state) {
        return Err(
            "Replay buffer is not recording. Start capture and check the selected microphone."
                .to_owned(),
        );
    }
    let capture = state.capture.as_ref().ok_or("Replay buffer is not recording")?;
    let segments = completed_segments(&capture.owner.directory, state.settings.replay_seconds)?;
    if segments.is_empty() {
        return Err("The replay buffer is warming up. Try again after a few seconds.".to_owned());
    }
    let output_dir = state.output_dir();
    fs::create_dir_all(&output_dir).map_err(io_message)?;
    let timestamp = unix_timestamp_millis();
    let output = output_dir.join(format!("courage-{timestamp}.mp4"));
    concat_segments(&segments, &output, &state.control.stop_requested)?;
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

fn concat_segments(segments: &[PathBuf], output: &Path, cancel: &AtomicBool) -> Result<(), String> {
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
    let result = match ManagedChild::output(&mut command, Duration::from_secs(30), Some(cancel)) {
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
    let output = ManagedChild::output(&mut command, Duration::from_secs(5), None).ok()?;
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

fn voice_config(settings: &SettingsFile) -> VoiceConfig {
    VoiceConfig {
        wake_phrase: settings.voice_wake_phrase.clone(),
        command_phrase: settings.voice_command_phrase.clone(),
        wake_only: settings.voice_wake_only,
        locale: settings.voice_locale.clone(),
    }
}

fn start_voice_listener(app: &AppHandle, state: &mut AppState) -> Result<(), String> {
    start_voice_session(app, state, voice_config(&state.settings), false)
}

fn start_voice_session(
    app: &AppHandle,
    state: &mut AppState,
    config: VoiceConfig,
    test_mode: bool,
) -> Result<(), String> {
    config.validate()?;
    stop_voice_listener(state)?;
    if state.control.exiting.load(Ordering::Acquire)
        || (!test_mode && state.control.stop_requested.load(Ordering::Acquire))
    {
        return Err("Courage is shutting down".into());
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            voice::SCRIPT,
        ]);
        let mut process = ManagedChild::spawn(&mut command)
            .map_err(|error| format!("Windows Speech could not start: {error}"))?;
        process
            .send(
                format!("{}\n", serde_json::to_string(&config).map_err(|error| error.to_string())?)
                    .as_bytes(),
            )
            .map_err(io_message)?;
        let generation = format!("voice-{}-{}", process.id(), unix_timestamp_millis());
        state.voice_diagnostics = VoiceDiagnostics::new(&config.locale);
        state.voice_diagnostics.generation = Some(generation.clone());
        state.voice_diagnostics.test_mode = test_mode;
        state.voice_diagnostics.availability = "starting".into();
        state.voice = Some(VoiceSession {
            process,
            config,
            generation,
            capture_session: state.capture.as_ref().map(|capture| capture.owner.session_id.clone()),
            test_mode,
            matcher: Matcher::default(),
            deadline: Instant::now() + Duration::from_secs(15),
            ready: false,
            test_until: test_mode.then(|| Instant::now() + Duration::from_secs(30)),
        });
        let _ = app.emit("engine-status", status_for(state));
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, test_mode);
        Err("Microphone recognition requires Windows System.Speech".into())
    }
}

fn stop_voice_listener(state: &mut AppState) -> Result<(), String> {
    if let Some(voice) = state.voice.as_mut() {
        voice.process.shutdown(b"stop\n", Duration::from_secs(3)).map_err(io_message)?;
    }
    state.voice = None;
    state.voice_diagnostics.listening = false;
    Ok(())
}

#[tauri::command]
async fn start_voice_test(app: AppHandle, config: VoiceConfig) -> Result<VoiceDiagnostics, String> {
    let operation = Operation::acquire(app.state::<Arc<Control>>().inner())?;
    with_state(app, move |app, state| {
        let _operation = operation;
        start_voice_session(app, state, config, true)?;
        Ok(state.voice_diagnostics.clone())
    })
    .await
}

#[tauri::command]
async fn stop_voice_test(app: AppHandle) -> Result<VoiceDiagnostics, String> {
    with_state(app, |app, state| {
        if state.voice.as_ref().is_some_and(|voice| voice.test_mode) {
            stop_voice_listener(state)?;
        }
        let _ = app.emit("engine-status", status_for(state));
        Ok(state.voice_diagnostics.clone())
    })
    .await
}

fn poll_voice(app: &AppHandle, state: &mut AppState) {
    let Some(voice) = state.voice.as_mut() else { return };
    let now = Instant::now();
    let mut failure = None;
    let mut changed = false;
    let expired = voice.test_until.is_some_and(|until| now >= until);
    let mut results = Vec::new();
    for line in voice.process.lines() {
        let Ok(event) = serde_json::from_str::<SpeechEvent>(&line.text) else { continue };
        match event.kind.as_str() {
            "ready" => {
                voice.ready = true;
                state.voice_diagnostics.availability = "available".into();
                state.voice_diagnostics.listening = true;
                if let Some(locale) = event.locale {
                    state.voice_diagnostics.locale = locale;
                }
                if let Some(microphone) = event.microphone {
                    state.voice_diagnostics.microphone = microphone;
                }
                state.voice_diagnostics.audio_format = event.audio_format;
                changed = true;
            }
            "error" => {
                failure = Some(event.error.unwrap_or_else(|| "Recognition engine failed".into()))
            }
            "result" => {
                let decision = voice.matcher.decide(&voice.config, &event, line.received, now);
                results.push(RecognitionResult {
                    generation: voice.generation.clone(),
                    phrase: event.phrase,
                    confidence: event.confidence,
                    decision,
                    test_mode: voice.test_mode,
                    saved: false,
                });
            }
            _ => {}
        }
    }
    if !voice.ready && now >= voice.deadline {
        failure = Some("Recognition engine did not become ready before its deadline".into());
    }
    if !matches!(voice.process.try_wait(), Ok(None)) {
        failure.get_or_insert_with(|| {
            format!("Recognition worker exited: {}", voice.process.stderr())
        });
    }
    let same_session = voice.capture_session.as_ref()
        == state.capture.as_ref().map(|capture| &capture.owner.session_id)
        && state.settings.voice_enabled
        && voice.config == voice_config(&state.settings);
    for mut result in results {
        if result.decision.would_trigger {
            if result.test_mode {
                result.decision.reason = "Matched in microphone test; saving is disabled".into();
            } else if !same_session || !state.voice_diagnostics.listening || failure.is_some() {
                result.decision.would_trigger = false;
                result.decision.reason = "Inactive recording session or failed recognizer".into();
            } else {
                let saved = Operation::acquire(&state.control)
                    .and_then(|_operation| save_replay_inner(state));
                match saved {
                    Ok(clip) => {
                        result.saved = true;
                        notify_clip_saved(app, &clip, "Voice trigger");
                    }
                    Err(error) => {
                        result.decision.reason = format!("Save not performed: {error}");
                    }
                }
            }
        }
        let _ = app.emit("voice-result", result);
    }
    if expired || failure.is_some() {
        if let Err(error) = stop_voice_listener(state) {
            failure = Some(error);
        }
        if let Some(error) = failure {
            state.voice_diagnostics.availability = "error".into();
            state.voice_diagnostics.error = Some(error);
        }
        changed = true;
    }
    if changed {
        let _ = app.emit("engine-status", status_for(state));
    }
}

fn start_supervisor(app: &AppHandle) -> io::Result<()> {
    let handle = app.clone();
    let control = app.state::<Arc<Control>>().inner().clone();
    let worker =
        std::thread::Builder::new().name("courage-supervisor".into()).spawn(move || {
            while !control.exit_complete.load(Ordering::Acquire) {
                if !control.exiting.load(Ordering::Acquire) {
                    if let Ok(mut state) = handle.state::<Mutex<AppState>>().try_lock() {
                        refresh_capture_status(&mut state);
                        poll_voice(&handle, &mut state);
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        })?;
    *app.state::<Mutex<Option<std::thread::JoinHandle<()>>>>()
        .lock()
        .map_err(|_| io::Error::other("supervisor state unavailable"))? = Some(worker);
    Ok(())
}

fn load_settings(path: &Path) -> Result<SettingsFile, String> {
    if !path.exists() {
        return Ok(SettingsFile::default());
    }
    let data = fs::read(path).map_err(io_message)?;
    serde_json::from_slice(&data)
        .map_err(|error| format!("Could not read Courage settings: {error}"))
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

#[tauri::command]
fn hide_to_tray(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    request_exit(&app);
}

fn request_exit(app: &AppHandle) {
    let control = app.state::<Arc<Control>>().inner().clone();
    if control.exiting.swap(true, Ordering::AcqRel) {
        return;
    }
    control.stop_epoch.fetch_add(1, Ordering::AcqRel);
    control.stop_requested.store(true, Ordering::Release);
    let _ = app.emit("app-exiting", ());
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = (|| {
            let state = app.state::<Mutex<AppState>>();
            let mut state = state.lock().map_err(|_| "Shutdown state unavailable".to_owned())?;
            let voice = stop_voice_listener(&mut state);
            let capture = stop_capture_inner(&mut state);
            voice.and(capture)
        })();
        if let Err(error) = result {
            // Keep the owner and window alive when cleanup failed. A repeated Quit retries.
            diagnostic_event("app.shutdown_failed", &error);
            let _ = app.emit("capture-error", format!("Shutdown incomplete: {error}"));
            let _ = app.emit("app-exit-failed", ());
            control.exiting.store(false, Ordering::Release);
            show_main_window(&app);
            return;
        }
        control.exit_complete.store(true, Ordering::Release);
        if let Some(worker) = app
            .state::<Mutex<Option<std::thread::JoinHandle<()>>>>()
            .lock()
            .ok()
            .and_then(|mut worker| worker.take())
        {
            let _ = worker.join();
        }
        diagnostic_event(
            "app.shutdown_complete",
            "owned processes reaped; readers joined; sessions removed",
        );
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir().map_err(|error| error.to_string())?;
            let state = AppState::new(data_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(state.control.clone());
            app.manage(Mutex::new(state));
            app.manage(Mutex::new(None::<std::thread::JoinHandle<()>>));

            let show = MenuItem::with_id(app, "show", "Open Courage", true, None::<&str>)?;
            let hide = MenuItem::with_id(
                app,
                "hide",
                "Hide to tray (keep recording)",
                true,
                None::<&str>,
            )?;
            let save = MenuItem::with_id(app, "save", "Save last replay", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Courage", true, None::<&str>)?;
            let menu = MenuBuilder::new(app).items(&[&show, &hide, &save, &quit]).build()?;
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
                    "hide" => hide_to_tray(app.clone()),
                    "save" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(error) = save_replay(app.clone()).await {
                                let _ = app.emit("capture-error", error);
                            }
                        });
                    }
                    "quit" => request_exit(app),
                    _ => {}
                })
                .build(app)?;
            start_supervisor(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                request_exit(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            engine_status,
            get_settings,
            list_audio_devices,
            test_voice_phrase,
            start_voice_test,
            stop_voice_test,
            save_settings,
            start_capture,
            stop_capture,
            save_replay,
            list_clips,
            delete_clip,
            open_path,
            hide_to_tray,
            quit_app
        ])
        .build(tauri::generate_context!())
        .expect("error while running Courage");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app.state::<Arc<Control>>().exit_complete.load(Ordering::Acquire) {
                api.prevent_exit();
                request_exit(app);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{normalize_phrase, SettingsFile};

    #[test]
    fn normalizes_voice_punctuation_and_case() {
        assert_eq!(normalize_phrase(" Courage, SAVE-that! "), "courage save that");
    }

    #[test]
    fn normalizes_unicode_letters() {
        assert_eq!(normalize_phrase("Écho — Café"), "écho café");
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

    #[test]
    fn concurrent_stop_requests_block_new_operations_until_all_finish() {
        use super::*;
        let control = Arc::new(Control::default());
        let operation = Operation::acquire(&control).unwrap();
        assert!(Operation::acquire(&control).is_err());
        let first = StopRequest::new(&control);
        let second = StopRequest::new(&control);
        drop(operation);
        drop(first);
        assert!(Operation::acquire(&control).is_err());
        drop(second);
        assert!(Operation::acquire(&control).is_ok());
        control.exiting.store(true, Ordering::Release);
        assert!(Operation::acquire(&control).is_err());
    }

    #[test]
    fn queued_start_cannot_resurrect_capture_after_a_later_stop_finishes() {
        use super::*;
        let control = Arc::new(Control::default());
        let queued_epoch = control.stop_epoch.load(Ordering::Acquire);
        let queued = Operation::acquire(&control).unwrap();
        drop(StopRequest::new(&control)); // Stop wins the state mutex first.
        assert_eq!(control.stops.load(Ordering::Acquire), 0);
        assert!(control.prepare_start(queued_epoch).is_err());
        assert!(control.stop_requested.load(Ordering::Acquire));
        drop(queued);
        let _fresh = Operation::acquire(&control).unwrap();
        assert!(control.prepare_start(control.stop_epoch.load(Ordering::Acquire)).is_ok());
        assert!(!control.stop_requested.load(Ordering::Acquire));
    }

    #[test]
    #[ignore = "requires installed ffmpeg/ffprobe; run cargo test -p courage-desktop real_ffmpeg -- --ignored --nocapture"]
    fn real_ffmpeg_twenty_cycles_preserve_saved_clips() {
        use super::*;
        let root = std::env::temp_dir().join(format!(
            "courage-real-{}-{}",
            std::process::id(),
            unix_timestamp_millis()
        ));
        let mut state = AppState::new(root.clone()).unwrap();
        state.settings.voice_enabled = false;
        state.settings.replay_seconds = 5;
        let mut previous_sessions = std::collections::HashSet::new();
        let mut saved = Vec::new();
        for cycle in 0..20 {
            let dir = new_session_dir(&root).unwrap();
            assert!(previous_sessions.insert(dir.clone()));
            let mut command = Command::new("ffmpeg");
            command
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-stdin",
                    "-progress",
                    "pipe:1",
                    "-stats_period",
                    "0.1",
                    "-re",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=10",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=48000",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-tune",
                    "zerolatency",
                    "-g",
                    "10",
                    "-c:a",
                    "aac",
                    "-f",
                    "segment",
                    "-segment_time",
                    "1",
                    "-reset_timestamps",
                    "1",
                ])
                .arg(dir.join("segment-%05d.mp4"));
            let mut owner = CaptureProcess::new(dir.clone());
            owner
                .start(&mut command, &state.control.stop_requested, Duration::from_secs(5))
                .unwrap();
            state.capture = Some(CaptureSession {
                owner,
                backend: "test lavfi".into(),
                audio_enabled: true,
                encoder: "libx264".into(),
                fps: 10,
                resolution: "64x64".into(),
            });
            if cycle >= 10 {
                let deadline = Instant::now() + Duration::from_secs(5);
                while completed_segments(&dir, 5).unwrap().is_empty() {
                    assert!(Instant::now() < deadline, "no closed segment in cycle {cycle}");
                    std::thread::sleep(Duration::from_millis(20));
                }
                let clip = save_replay_inner(&mut state).unwrap();
                assert!(probe_duration_ms(Path::new(&clip.path)).unwrap() > 0);
                saved.push(PathBuf::from(clip.path));
            }
            stop_capture_inner(&mut state).unwrap();
            stop_capture_inner(&mut state).unwrap();
            assert!(!dir.exists());
            for clip in &saved {
                assert!(clip.is_file());
            }
        }
        assert_eq!(state.store.list_clips().unwrap().len(), 10);
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }
}
