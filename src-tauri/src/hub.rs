//! In-process fan-out hub between the native RTMP ingest server and the
//! per-platform forwarders.
//!
//! The ingest server pushes FLV media tags into a broadcast channel; each
//! forwarder subscribes and feeds them to its FFmpeg process. AVC/AAC sequence
//! headers are cached so forwarders that (re)connect mid-stream can prime
//! their decoder configuration before receiving live tags.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, watch};

pub const TAG_AUDIO: u8 = 8;
pub const TAG_VIDEO: u8 = 9;

#[derive(Clone)]
pub struct FlvTag {
    pub tag_type: u8,
    pub timestamp: u32,
    pub payload: Arc<Vec<u8>>,
}

#[derive(Clone)]
pub enum HubMessage {
    Tag(FlvTag),
    /// The OBS publish session ended; forwarders should finish their current
    /// FFmpeg run and wait for the next session (timestamps restart at zero).
    SessionEnded,
}

#[derive(Default, Clone)]
pub struct SessionCache {
    pub video_seq: Option<FlvTag>,
    pub audio_seq: Option<FlvTag>,
}

pub struct StreamHub {
    tx: broadcast::Sender<HubMessage>,
    cache: Mutex<SessionCache>,
    /// Current publish session id; 0 means no active session.
    session_tx: watch::Sender<u64>,
    next_session: AtomicU64,
}

impl StreamHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(8192);
        let (session_tx, _) = watch::channel(0u64);
        Self {
            tx,
            cache: Mutex::new(SessionCache::default()),
            session_tx,
            next_session: AtomicU64::new(1),
        }
    }

    pub fn begin_session(&self) -> u64 {
        let id = self.next_session.fetch_add(1, Ordering::Relaxed);
        *self.cache.lock().unwrap() = SessionCache::default();
        self.session_tx.send_replace(id);
        id
    }

    pub fn end_session(&self) {
        self.session_tx.send_replace(0);
        let _ = self.tx.send(HubMessage::SessionEnded);
    }

    pub fn session_watch(&self) -> watch::Receiver<u64> {
        self.session_tx.subscribe()
    }

    /// Subscribe to live tags and get a consistent snapshot of the cached
    /// sequence headers. Subscribing before snapshotting guarantees no tag is
    /// lost in between (duplicated sequence headers are harmless).
    pub fn subscribe(&self) -> (broadcast::Receiver<HubMessage>, SessionCache) {
        let rx = self.tx.subscribe();
        let cache = self.cache.lock().unwrap().clone();
        (rx, cache)
    }

    pub fn publish_media(&self, tag_type: u8, timestamp: u32, payload: Vec<u8>) {
        let tag = FlvTag {
            tag_type,
            timestamp,
            payload: Arc::new(payload),
        };
        let is_sequence_header = match tag_type {
            // AVC: frame type/codec id 0x_7, packet type 0.
            TAG_VIDEO => {
                tag.payload.len() >= 2 && (tag.payload[0] & 0x0f) == 7 && tag.payload[1] == 0
            }
            // AAC: sound format 10, packet type 0.
            TAG_AUDIO => {
                tag.payload.len() >= 2 && (tag.payload[0] >> 4) == 10 && tag.payload[1] == 0
            }
            _ => false,
        };
        if is_sequence_header {
            let mut cache = self.cache.lock().unwrap();
            if tag_type == TAG_VIDEO {
                cache.video_seq = Some(tag.clone());
            } else {
                cache.audio_seq = Some(tag.clone());
            }
        }
        let _ = self.tx.send(HubMessage::Tag(tag));
    }
}
