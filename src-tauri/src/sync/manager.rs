use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::AppResult;
use crate::models::SyncStatusPayload;
use crate::storage::Storage;
use crate::sync::relay::RelayTransport;

#[derive(Clone)]
pub struct SyncManager {
    inner: Arc<RwLock<SyncInner>>,
}

struct SyncInner {
    device_id: String,
    pairing_token: String,
    pending_entries: usize,
    paired_devices: usize,
    transport: String,
    relay: RelayTransport,
}

impl SyncManager {
    pub fn new(device_id: String, pairing_token: String) -> Self {
        Self {
            inner: Arc::new(RwLock::new(SyncInner {
                device_id,
                pairing_token,
                pending_entries: 0,
                paired_devices: 0,
                transport: "offline".into(),
                relay: RelayTransport::new(),
            })),
        }
    }

    pub async fn bootstrap(&self, storage: Storage) -> AppResult<()> {
        let mut inner = self.inner.write().await;
        inner.pending_entries = storage.item_count()?;
        Ok(())
    }

    pub async fn status(&self) -> SyncStatusPayload {
        let inner = self.inner.read().await;
        SyncStatusPayload {
            device_id: inner.device_id.clone(),
            paired_devices: inner.paired_devices,
            transport: inner.transport.clone(),
            relay_connected: inner.relay.connected,
            pending_entries: inner.pending_entries,
            pairing_token: inner.pairing_token.clone(),
        }
    }

    pub async fn update_pairing_token(&self, new_token: String) {
        let mut inner = self.inner.write().await;
        inner.pairing_token = new_token;
        // Optionally reset transport state
        inner.paired_devices = 0;
        inner.transport = "offline".into();
    }

    pub async fn note_local_change(&self, storage: Storage) -> AppResult<()> {
        let mut inner = self.inner.write().await;
        inner.pending_entries = storage.item_count()?;
        Ok(())
    }
}
