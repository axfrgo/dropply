use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub item_type: ItemType,
    pub content_ref: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub device_id: String,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub text_preview: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Text,
    Image,
    File,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: ItemType,
    pub content_ref: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub device_id: String,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub text_preview: Option<String>,
}

impl From<Item> for ItemPayload {
    fn from(value: Item) -> Self {
        Self {
            id: value.id,
            item_type: value.item_type,
            content_ref: value.content_ref,
            created_at: value.created_at,
            updated_at: value.updated_at,
            device_id: value.device_id,
            name: value.name,
            mime_type: value.mime_type,
            size_bytes: value.size_bytes,
            sha256: value.sha256,
            text_preview: value.text_preview,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BootstrapPayload {
    pub items: Vec<ItemPayload>,
    pub sync_status: SyncStatusPayload,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncStatusPayload {
    pub device_id: String,
    pub paired_devices: usize,
    pub transport: String,
    pub relay_connected: bool,
    pub pending_entries: usize,
    pub pairing_token: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ImportTextPayload {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ImportPathPayload {
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub device_id: String,
    pub item_id: String,
    pub op: String,
    pub updated_at: DateTime<Utc>,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairingInfo {
    pub device_id: String,
    pub pairing_token: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayItemPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: ItemType,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub device_id: String,
    pub text_content: Option<String>,
    pub bytes_b64: Option<String>,
}
