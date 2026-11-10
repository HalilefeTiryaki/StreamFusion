mod commands;
mod config;
mod engine;
mod hub;
mod metrics;
mod models;
mod rtmp_server;

use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub engine: Arc<engine::Engine>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(AppState {
                engine: Arc::new(engine::Engine::new(handle.clone())),
            });
            metrics::spawn_system_metrics(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_platform_config,
            commands::set_platform_live,
            commands::start_engine,
            commands::stop_engine,
            commands::engine_status,
            commands::check_ffmpeg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running StreamFusion");
}
