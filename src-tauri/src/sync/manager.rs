use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;

use crate::error::AppResult;
use crate::models::{Item, SyncStatusPayload};
use crate::storage::Storage;
use crate::sync::{relay::RelayTransport, webrtc::WebRtcTransport};

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
    webrtc: WebRtcTransport,
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
                webrtc: WebRtcTransport::new(),
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

    pub async fn note_local_change(&self, storage: Storage) -> AppResult<()> {
        let mut inner = self.inner.write().await;
        inner.pending_entries = storage.item_count()?;
        Ok(())
    }

    pub async fn ingest_remote_batch(&self, storage: Storage, batch: Vec<Value>) -> AppResult<()> {
        for payload in batch {
            let item: Item = serde_json::from_value(payload)?;
            storage.upsert_remote_item(item).await?;
        }

        self.note_local_change(storage).await?;
        Ok(())
    }

    pub async fn prepare_pairing_offer(&self) -> AppResult<String> {
        let inner = self.inner.read().await;
        let device_id = inner.device_id.clone();
        let webrtc = inner.webrtc.clone();
        drop(inner);

        let offer = webrtc.create_offer(&device_id).await?;
        Ok(serde_json::to_string(&offer)?)
    }
}
