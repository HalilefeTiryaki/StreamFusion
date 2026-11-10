use std::time::Duration;

use sysinfo::System;
use tauri::{AppHandle, Emitter};

use crate::models::SystemMetricsEvent;

const MIB: u64 = 1024 * 1024;

/// Emits `system-metrics` events (CPU / RAM) once per second for the dashboard.
pub fn spawn_system_metrics(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut sys = System::new();
        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let _ = app.emit(
                "system-metrics",
                SystemMetricsEvent {
                    cpu_percent: sys.global_cpu_usage(),
                    mem_used_mb: sys.used_memory() / MIB,
                    mem_total_mb: sys.total_memory() / MIB,
                },
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}
