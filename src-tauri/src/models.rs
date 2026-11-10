use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Twitch,
    Youtube,
    Kick,
    Facebook,
}

impl Platform {
    pub const ALL: [Platform; 4] = [
        Platform::Twitch,
        Platform::Youtube,
        Platform::Kick,
        Platform::Facebook,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Platform::Twitch => "twitch",
            Platform::Youtube => "youtube",
            Platform::Kick => "kick",
            Platform::Facebook => "facebook",
        }
    }

    pub fn default_server_url(&self) -> &'static str {
        match self {
            Platform::Twitch => "rtmp://live.twitch.tv/app",
            Platform::Youtube => "rtmp://a.rtmp.youtube.com/live2",
            // Kick shows a per-account IVS ingest URL in the creator dashboard;
            // this is the common default, but users can override it in the UI.
            Platform::Kick => "rtmps://fa723fc1b171.global-contribute.live-video.net/app",
            Platform::Facebook => "rtmps://live-api-s.facebook.com:443/rtmp",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformConfig {
    pub enabled: bool,
    pub stream_key: String,
    pub server_url: String,
}

impl PlatformConfig {
    pub fn default_for(platform: Platform) -> Self {
        Self {
            enabled: false,
            stream_key: String::new(),
            server_url: platform.default_server_url().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub ingest_port: u16,
    pub platforms: HashMap<Platform, PlatformConfig>,
}

impl AppConfig {
    /// Guarantees an entry exists for every supported platform.
    pub fn ensure_all_platforms(&mut self) {
        for platform in Platform::ALL {
            self.platforms
                .entry(platform)
                .or_insert_with(|| PlatformConfig::default_for(platform));
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut cfg = Self {
            ingest_port: 1935,
            platforms: HashMap::new(),
        };
        cfg.ensure_all_platforms();
        cfg
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlatformStatus {
    Idle,
    Connecting,
    Live,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformStatusEvent {
    pub platform: Platform,
    pub status: PlatformStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformMetricsEvent {
    pub platform: Platform,
    pub bitrate_kbps: f64,
    pub speed: f64,
    pub fps: f64,
    pub uptime_sec: u64,
    pub reconnects: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestStatusEvent {
    pub status: String,
    pub bitrate_kbps: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMetricsEvent {
    pub cpu_percent: f32,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub ts: u64,
    pub level: String,
    pub source: String,
    pub message: String,
}
