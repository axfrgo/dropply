use serde::{Deserialize, Serialize};

use crate::models::LogEntry;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RelayMessage {
    Hello { device_id: String, pairing_token: String },
    LogBatch { device_id: String, entries: Vec<LogEntry> },
    BlobRequest { item_id: String, sha256: String },
    BlobChunk {
        item_id: String,
        sha256: String,
        chunk_index: u32,
        total_chunks: u32,
        bytes_b64: String,
    },
}

#[derive(Clone, Debug, Default)]
pub struct RelayTransport {
    pub connected: bool,
}

impl RelayTransport {
    pub fn new() -> Self {
        Self { connected: false }
    }

    pub async fn connect(&mut self, _endpoint: &str) -> anyhow::Result<()> {
        self.connected = true;
        Ok(())
    }
}

