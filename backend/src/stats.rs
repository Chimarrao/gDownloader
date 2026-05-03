use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RealtimeStats {
    pub timestamp: u64,
    pub total_speed_bps: u64,
    pub per_host_speed: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct StatsManager {
    ring_buffer: Arc<RwLock<Vec<RealtimeStats>>>,
    buffer_size: usize,
}

impl StatsManager {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            ring_buffer: Arc::new(RwLock::new(Vec::with_capacity(buffer_size))),
            buffer_size,
        }
    }

    pub async fn record_stats(&self, total_speed_bps: u64, per_host_speed: HashMap<String, u64>) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let stats = RealtimeStats {
            timestamp,
            total_speed_bps,
            per_host_speed,
        };

        let mut buffer = self.ring_buffer.write().await;
        buffer.push(stats);
        if buffer.len() > self.buffer_size {
            buffer.remove(0);
        }
    }

    pub async fn get_realtime_stats(&self) -> Vec<RealtimeStats> {
        self.ring_buffer.read().await.clone()
    }

    pub async fn get_current_stats(&self) -> Option<RealtimeStats> {
        self.ring_buffer.read().await.last().cloned()
    }
}
