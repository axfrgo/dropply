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
    pub source_context: Option<SourceContextPayload>,
    pub semantic_context: Option<SemanticContextPayload>,
    #[serde(default)]
    pub suggested_actions: Vec<SuggestedActionPayload>,
    #[serde(default)]
    pub intent_state: IntentState,
    pub trust_context: Option<TrustContextPayload>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
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
    pub storage_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub device_id: String,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub text_preview: Option<String>,
    pub text_content: Option<String>,
    pub source_context: Option<SourceContextPayload>,
    pub semantic_context: Option<SemanticContextPayload>,
    #[serde(default)]
    pub suggested_actions: Vec<SuggestedActionPayload>,
    #[serde(default)]
    pub intent_state: IntentState,
    pub trust_context: Option<TrustContextPayload>,
}

impl From<Item> for ItemPayload {
    fn from(value: Item) -> Self {
        Self {
            id: value.id,
            item_type: value.item_type,
            content_ref: value.content_ref,
            storage_path: None,
            created_at: value.created_at,
            updated_at: value.updated_at,
            device_id: value.device_id,
            name: value.name,
            mime_type: value.mime_type,
            size_bytes: value.size_bytes,
            sha256: value.sha256,
            text_preview: value.text_preview,
            text_content: None,
            source_context: value.source_context,
            semantic_context: value.semantic_context,
            suggested_actions: value.suggested_actions,
            intent_state: value.intent_state,
            trust_context: value.trust_context,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentState {
    Captured,
    Pending,
    Sent,
    Resumed,
    Completed,
    Revoked,
}

impl Default for IntentState {
    fn default() -> Self {
        Self::Captured
    }
}

impl IntentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Resumed => "resumed",
            Self::Completed => "completed",
            Self::Revoked => "revoked",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "pending" => Self::Pending,
            "sent" => Self::Sent,
            "resumed" => Self::Resumed,
            "completed" => Self::Completed,
            "revoked" => Self::Revoked,
            _ => Self::Captured,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Composer,
    Paste,
    DragDrop,
    FilePicker,
    BrowserShare,
    Relay,
    Direct,
}

impl Default for SourceKind {
    fn default() -> Self {
        Self::Composer
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceContextPayload {
    pub source_kind: SourceKind,
    pub source_app: Option<String>,
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub source_device_id: String,
    pub captured_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticContextPayload {
    pub primary_label: String,
    pub summary: Option<String>,
    pub extracted_text_preview: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedActionId {
    Copy,
    Open,
    Download,
    OpenBundle,
    SendToDevice,
    ResumeLater,
    SummarizeLater,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuggestedActionPayload {
    pub id: SuggestedActionId,
    pub label: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustProvenance {
    Local,
    PairedDevice,
    BrowserExtension,
}

impl Default for TrustProvenance {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustContextPayload {
    pub local_first: bool,
    pub provenance: TrustProvenance,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
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
    pub id: Option<String>,
    #[serde(default)]
    pub source_kind: Option<SourceKind>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ImportPathPayload {
    pub paths: Vec<String>,
    #[serde(default)]
    pub source_kind: Option<SourceKind>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationBundleSourcePayload {
    pub path: Option<String>,
    pub archive_path: Option<String>,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub text_content: Option<String>,
    pub bytes_b64: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ImportConversationBundlePayload {
    pub title: Option<String>,
    pub transcript_markdown: String,
    pub source_label: Option<String>,
    pub source_url: Option<String>,
    #[serde(default)]
    pub files: Vec<ConversationBundleSourcePayload>,
    #[serde(default)]
    pub attachments: Vec<ConversationBundleSourcePayload>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationBundleEntryRole {
    Reference,
    Attachment,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationBundleEntryPayload {
    pub path: String,
    pub role: ConversationBundleEntryRole,
    pub name: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationBundleManifestPayload {
    pub bundle_version: String,
    pub title: String,
    pub source_label: Option<String>,
    pub source_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub transcript_path: String,
    pub transcript_sha256: String,
    pub entries: Vec<ConversationBundleEntryPayload>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationBundleDetailsPayload {
    pub manifest: ConversationBundleManifestPayload,
    pub transcript_markdown: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationBundleTextEntryPayload {
    pub path: String,
    pub mime_type: Option<String>,
    pub content: String,
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
pub struct ZenithMetadataPayload {
    pub enabled: bool,
    pub eligible: bool,
    pub bypassed: bool,
    pub entropy: Option<f64>,
    pub equation_weight_bytes: Option<i64>,
    pub verification: Option<String>,
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
    pub deleted: Option<bool>,
    pub zenith_equation: Option<serde_json::Value>,
    pub zenith_metadata: Option<ZenithMetadataPayload>,
    pub source_context: Option<SourceContextPayload>,
    pub semantic_context: Option<SemanticContextPayload>,
    #[serde(default)]
    pub suggested_actions: Vec<SuggestedActionPayload>,
    #[serde(default)]
    pub intent_state: IntentState,
    pub trust_context: Option<TrustContextPayload>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayBlobPayload {
    pub item_id: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub chunks: Vec<String>,
}
