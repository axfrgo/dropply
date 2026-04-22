use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerOffer {
    pub device_id: String,
    pub sdp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlobChunk {
    pub item_id: String,
    pub sha256: String,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub bytes_b64: String,
}

#[derive(Clone, Debug, Default)]
pub struct WebRtcTransport;

impl WebRtcTransport {
    pub fn new() -> Self {
        Self
    }

    pub async fn create_offer(&self, device_id: &str) -> anyhow::Result<PeerOffer> {
        Ok(PeerOffer {
            device_id: device_id.to_string(),
            sdp: format!("stub-offer-for-{device_id}"),
        })
    }

    pub async fn apply_remote_offer(&self, _offer: PeerOffer) -> anyhow::Result<()> {
        Ok(())
    }
}

