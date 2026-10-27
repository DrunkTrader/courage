use courage_core::VideoSpec;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EngineStatus {
    pub state: &'static str,
    pub platform: &'static str,
    pub backend: &'static str,
}

#[tauri::command]
fn engine_status() -> EngineStatus {
    let _default_spec = VideoSpec::new(1920, 1080, 60).ok();
    EngineStatus {
        state: "unavailable",
        platform: std::env::consts::OS,
        backend: "native capture backend not connected",
    }
}

#[tauri::command]
fn save_replay() -> Result<(), String> {
    Err("native capture backend is not connected in this build".to_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![engine_status, save_replay])
        .run(tauri::generate_context!())
        .expect("error while running Courage");
}
