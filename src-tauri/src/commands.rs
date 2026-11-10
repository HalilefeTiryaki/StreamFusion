use std::process::Stdio;

use tauri::{AppHandle, State};

use crate::models::{AppConfig, Platform, PlatformConfig};
use crate::AppState;

#[tauri::command]
pub fn get_config(app: AppHandle) -> Result<AppConfig, String> {
    crate::config::load_config(&app)
}

#[tauri::command]
pub fn set_platform_config(
    app: AppHandle,
    platform: Platform,
    config: PlatformConfig,
) -> Result<(), String> {
    let mut cfg = crate::config::load_config(&app)?;
    cfg.platforms.insert(platform, config);
    crate::config::save_config(&app, &cfg)
}

/// Toggles a platform on/off. Persists the flag and, when the engine is
/// running, starts or stops that platform's forwarder immediately.
#[tauri::command]
pub async fn set_platform_live(
    app: AppHandle,
    state: State<'_, AppState>,
    platform: Platform,
    live: bool,
) -> Result<(), String> {
    let mut cfg = crate::config::load_config(&app)?;
    let entry = cfg
        .platforms
        .entry(platform)
        .or_insert_with(|| PlatformConfig::default_for(platform));
    entry.enabled = live;
    let entry = entry.clone();
    crate::config::save_config(&app, &cfg)?;
    state.engine.set_platform_live(platform, &entry, live).await
}

#[tauri::command]
pub async fn start_engine(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let cfg = crate::config::load_config(&app)?;
    state.engine.start(&cfg).await
}

#[tauri::command]
pub async fn stop_engine(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn engine_status(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.engine.is_running().await)
}

/// Returns the FFmpeg version banner, or an error if FFmpeg is not on PATH.
#[tauri::command]
pub async fn check_ffmpeg() -> Result<String, String> {
    let mut cmd = tokio::process::Command::new("ffmpeg");
    cmd.arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let output = cmd
        .output()
        .await
        .map_err(|_| "FFmpeg was not found on PATH".to_string())?;
    if !output.status.success() {
        return Err("FFmpeg returned an error".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("ffmpeg")
        .to_string())
}
