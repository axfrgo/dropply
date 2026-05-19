pub mod blobs;
pub mod bundles;
pub mod db;
pub mod import;
pub mod sandbox;
pub mod smart_drops;

use std::path::{Path, PathBuf};

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

#[cfg(feature = "zenith")]
use zenith_core::data_plane::DataPlane;

use crate::error::{AppError, AppResult};
use crate::models::{
    ConversationBundleDetailsPayload, ConversationBundleTextEntryPayload,
    ImportConversationBundlePayload, ImportPathPayload, IntentState, Item, ItemPayload, ItemType, LogEntry,
    PairingInfo, RelayBlobPayload, RelayItemPayload, SourceKind, TrustProvenance, ZenithMetadataPayload,
};
use crate::storage::smart_drops::SmartDropSeed;
use crate::sync::log::LogStore;

#[derive(Clone)]
pub struct Storage {
    inner: std::sync::Arc<StorageInner>,
}

struct StorageInner {
    base_dir: PathBuf,
    blobs_dir: PathBuf,
    db: db::Database,
    import_lock: Mutex<()>,
    import_broker: sandbox::ImportBroker,
    log_store: LogStore,
}

fn compute_zenith_sidecar(bytes: &[u8]) -> (Option<serde_json::Value>, Option<ZenithMetadataPayload>) {
    #[cfg(feature = "zenith")]
    {
        use zenith_core::entropy::EntropyHeuristic;

        let entropy = EntropyHeuristic::calculate(bytes);
        let mut dp = DataPlane::new();
        match dp.ingest(bytes) {
            Ok(Some(equation)) => {
                let metadata = ZenithMetadataPayload {
                    enabled: true,
                    eligible: true,
                    bypassed: false,
                    entropy: Some(entropy),
                    equation_weight_bytes: Some(equation.weight_bytes as i64),
                    verification: Some("pending".into()),
                };
                (serde_json::to_value(&equation).ok(), Some(metadata))
            }
            Ok(None) => {
                let metadata = ZenithMetadataPayload {
                    enabled: true,
                    eligible: false,
                    bypassed: true,
                    entropy: Some(entropy),
                    equation_weight_bytes: None,
                    verification: Some("not_applicable".into()),
                };
                (None, Some(metadata))
            }
            Err(_) => {
                let metadata = ZenithMetadataPayload {
                    enabled: true,
                    eligible: false,
                    bypassed: true,
                    entropy: Some(entropy),
                    equation_weight_bytes: None,
                    verification: Some("error".into()),
                };
                (None, Some(metadata))
            }
        }
    }
    #[cfg(not(feature = "zenith"))]
    {
        let _ = bytes;
        (None, None)
    }
}

impl Storage {
    pub async fn new(app_name: &str) -> AppResult<Self> {
        let base_dir = match resolve_base_dir(app_name) {
            Ok(path) => path,
            Err(error) => {
                return Err(AppError::Message(format!(
                    "failed to resolve Dropply base directory for {app_name}: {error}"
                )));
            }
        };
        let blobs_dir = base_dir.join("blobs");
        let staging_dir = base_dir.join("staging");
        match std::fs::create_dir_all(&blobs_dir) {
            Ok(()) => {}
            // On Windows, create_dir_all can report AlreadyExists when another Dropply
            // surface has already materialized the shared blobs directory.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                let error_kind = error.kind();
                let raw_os_error = error.raw_os_error();
                return Err(AppError::Message(format!(
                    "failed to create blobs directory at {}: {} (kind: {:?}, raw_os_error: {:?})",
                    blobs_dir.display(),
                    error,
                    error_kind,
                    raw_os_error
                )));
            }
        }

        let db_path = base_dir.join("dropply.sqlite3");
        let db = match db::Database::open(&db_path) {
            Ok(db) => db,
            Err(error) => {
                return Err(AppError::Message(format!(
                    "failed to open Dropply database at {}: {}",
                    db_path.display(),
                    error
                )));
            }
        };
        if let Err(error) = db.migrate() {
            return Err(AppError::Message(format!(
                "failed to migrate Dropply database at {}: {}",
                db_path.display(),
                error
            )));
        }

        if let Err(error) = db.load_or_create_pairing() {
            return Err(AppError::Message(format!(
                "failed to load or create Dropply pairing record in {}: {}",
                db_path.display(),
                error
            )));
        }
        let log_store = LogStore::new(db.clone());
        let import_broker = sandbox::ImportBroker::new(staging_dir)?;

        Ok(Self {
            inner: std::sync::Arc::new(StorageInner {
                base_dir,
                blobs_dir,
                db,
                import_lock: Mutex::new(()),
                import_broker,
                log_store,
            }),
        })
    }

    pub fn base_dir(&self) -> &Path {
        &self.inner.base_dir
    }

    pub fn pairing(&self) -> AppResult<PairingInfo> {
        self.inner.db.load_or_create_pairing()
    }

    pub fn update_pairing_token(&self, new_token: String) -> AppResult<()> {
        self.inner.db.update_pairing_token(&new_token)?;
        Ok(())
    }

    pub fn reset_pairing_token(&self) -> AppResult<String> {
        self.inner.db.reset_pairing_token()
    }

    pub fn clear_pairing(&self) -> AppResult<()> {
        self.inner.db.clear_pairing()
    }

    pub async fn list_items(&self) -> AppResult<Vec<ItemPayload>> {
        let items = self.inner.db.list_items()?;
        Ok(items.into_iter().map(|item| self.to_payload(item)).collect())
    }

    pub async fn export_relay_items(&self) -> AppResult<Vec<RelayItemPayload>> {
        let items = self.inner.db.list_items()?;
        let mut output = Vec::with_capacity(items.len());

        for item in items {
            match item.item_type {
                ItemType::Text => {
                    let text = tokio::fs::read_to_string(self.inner.base_dir.join(&item.content_ref)).await?;
                    output.push(RelayItemPayload {
                        id: item.id,
                        item_type: item.item_type,
                        name: item.name,
                        mime_type: item.mime_type,
                        size_bytes: item.size_bytes,
                        sha256: item.sha256,
                        updated_at: item.updated_at,
                        device_id: item.device_id,
                        text_content: Some(text),
                        bytes_b64: None,
                        deleted: Some(false),
                        zenith_equation: None,
                        zenith_metadata: None,
                        source_context: item.source_context,
                        semantic_context: item.semantic_context,
                        suggested_actions: item.suggested_actions,
                        intent_state: item.intent_state,
                        trust_context: item.trust_context,
                    });
                }
                ItemType::Image | ItemType::File => {
                    let bytes = tokio::fs::read(self.resolve_asset_path(&item.content_ref)).await?;
                    let (zenith_equation, zenith_metadata) = compute_zenith_sidecar(&bytes);
                    output.push(RelayItemPayload {
                        id: item.id,
                        item_type: item.item_type,
                        name: item.name,
                        mime_type: item.mime_type,
                        size_bytes: item.size_bytes,
                        sha256: item.sha256,
                        updated_at: item.updated_at,
                        device_id: item.device_id,
                        text_content: None,
                        bytes_b64: Some(BASE64.encode(&bytes)),
                        deleted: Some(false),
                        zenith_equation,
                        zenith_metadata,
                        source_context: item.source_context,
                        semantic_context: item.semantic_context,
                        suggested_actions: item.suggested_actions,
                        intent_state: item.intent_state,
                        trust_context: item.trust_context,
                    });
                }
            }
        }

        let deleted_logs = self.inner.db.list_deleted_log_entries()?;
        for log in deleted_logs {
            output.push(RelayItemPayload {
                id: log.item_id,
                item_type: ItemType::Text, // Dummy type for deletion
                name: None,
                mime_type: None,
                size_bytes: None,
                sha256: None,
                updated_at: log.updated_at,
                device_id: log.device_id,
                text_content: None,
                bytes_b64: None,
                deleted: Some(true),
                zenith_equation: None,
                zenith_metadata: None,
                source_context: None,
                semantic_context: None,
                suggested_actions: Vec::new(),
                intent_state: IntentState::Revoked,
                trust_context: None,
            });
        }

        Ok(output)
    }

    pub async fn export_pair_manifest(&self) -> AppResult<Vec<RelayItemPayload>> {
        let items = self.inner.db.list_items()?;
        let mut output = Vec::with_capacity(items.len());

        for item in items {
            match item.item_type {
                ItemType::Text => {
                    let text = tokio::fs::read_to_string(self.inner.base_dir.join(&item.content_ref)).await?;
                    output.push(RelayItemPayload {
                        id: item.id,
                        item_type: item.item_type,
                        name: item.name,
                        mime_type: item.mime_type,
                        size_bytes: item.size_bytes,
                        sha256: item.sha256,
                        updated_at: item.updated_at,
                        device_id: item.device_id,
                        text_content: Some(text),
                        bytes_b64: None,
                        deleted: Some(false),
                        zenith_equation: None,
                        zenith_metadata: None,
                        source_context: item.source_context,
                        semantic_context: item.semantic_context,
                        suggested_actions: item.suggested_actions,
                        intent_state: item.intent_state,
                        trust_context: item.trust_context,
                    });
                }
                ItemType::Image | ItemType::File => {
                    let mut bytes_b64 = None;
                    let can_inline = matches!(item.item_type, ItemType::Image)
                        && item
                            .mime_type
                            .as_deref()
                            .map(|mime| mime.starts_with("image/"))
                            .unwrap_or(true);
                    if can_inline {
                        if let Some(size) = item.size_bytes {
                            if size > 0 && size <= 128 * 1024 {
                                if let Ok(bytes) = tokio::fs::read(self.inner.base_dir.join(&item.content_ref)).await {
                                    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
                                    bytes_b64 = Some(BASE64.encode(bytes));
                                }
                            }
                        }
                    }
                    output.push(RelayItemPayload {
                        id: item.id,
                        item_type: item.item_type,
                        name: item.name,
                        mime_type: item.mime_type,
                        size_bytes: item.size_bytes,
                        sha256: item.sha256,
                        updated_at: item.updated_at,
                        device_id: item.device_id,
                        text_content: None,
                        bytes_b64,
                        deleted: Some(false),
                        zenith_equation: None,
                        zenith_metadata: None,
                        source_context: item.source_context,
                        semantic_context: item.semantic_context,
                        suggested_actions: item.suggested_actions,
                        intent_state: item.intent_state,
                        trust_context: item.trust_context,
                    });
                }
            }
        }

        let deleted_logs = self.inner.db.list_deleted_log_entries()?;
        for log in deleted_logs {
            output.push(RelayItemPayload {
                id: log.item_id,
                item_type: ItemType::Text,
                name: None,
                mime_type: None,
                size_bytes: None,
                sha256: None,
                updated_at: log.updated_at,
                device_id: log.device_id,
                text_content: None,
                bytes_b64: None,
                deleted: Some(true),
                zenith_equation: None,
                zenith_metadata: None,
                source_context: None,
                semantic_context: None,
                suggested_actions: Vec::new(),
                intent_state: IntentState::Revoked,
                trust_context: None,
            });
        }

        Ok(output)
    }

    pub async fn export_relay_blob(&self, item_id: &str, chunk_bytes: usize) -> AppResult<RelayBlobPayload> {
        let Some(item) = self.inner.db.get_item(item_id)? else {
            return Err(crate::error::AppError::Message(format!("Item {item_id} was not found.")));
        };

        match item.item_type {
            ItemType::Text => {
                Err(crate::error::AppError::Message(format!(
                    "Item {item_id} does not have relay blob bytes."
                )))
            }
            ItemType::Image | ItemType::File => {
                let chunk_bytes = chunk_bytes.max(1);
                let bytes = tokio::fs::read(self.resolve_asset_path(&item.content_ref)).await?;
                let chunks = bytes
                    .chunks(chunk_bytes)
                    .map(|chunk| BASE64.encode(chunk))
                    .collect::<Vec<_>>();

                Ok(RelayBlobPayload {
                    item_id: item.id,
                    mime_type: item.mime_type,
                    size_bytes: item.size_bytes.unwrap_or(bytes.len() as i64),
                    sha256: item.sha256,
                    updated_at: item.updated_at,
                    chunks,
                })
            }
        }
    }

    pub fn item_count(&self) -> AppResult<usize> {
        Ok(self.inner.db.count_items()? as usize)
    }

    pub fn list_recent_logs(&self, limit: usize) -> AppResult<Vec<LogEntry>> {
        self.inner.db.list_recent_logs(limit)
    }

    pub async fn import_text(&self, text: String, provided_id: Option<String>) -> AppResult<ItemPayload> {
        self.import_text_with_source(text, provided_id, SourceKind::Composer).await
    }

    pub async fn import_text_with_source(
        &self,
        text: String,
        provided_id: Option<String>,
        source_kind: SourceKind,
    ) -> AppResult<ItemPayload> {
        let _guard = self.inner.import_lock.lock().await;
        let item = import::persist_text(
            &self.inner.db,
            &self.inner.log_store,
            &self.inner.base_dir,
            &self.pairing()?.device_id,
            text,
            provided_id,
            source_kind,
        )?;
        Ok(self.to_payload(item))
    }

    pub async fn import_relay_item(&self, payload: RelayItemPayload) -> AppResult<ItemPayload> {
        let _guard = self.inner.import_lock.lock().await;
        let item = import::persist_relay_item(
            &self.inner.db,
            &self.inner.log_store,
            &self.inner.base_dir,
            &self.inner.blobs_dir,
            payload,
        ).await?;
        Ok(self.to_payload(item))
    }

    pub async fn import_staged_relay_item(
        &self,
        payload: RelayItemPayload,
        staged_path: &Path,
    ) -> AppResult<ItemPayload> {
        let _guard = self.inner.import_lock.lock().await;
        let item = import::persist_staged_relay_item(
            &self.inner.db,
            &self.inner.log_store,
            &self.inner.base_dir,
            &self.inner.blobs_dir,
            payload,
            staged_path,
        )
        .await?;
        Ok(self.to_payload(item))
    }

    pub async fn import_paths(&self, payload: ImportPathPayload) -> AppResult<Vec<ItemPayload>> {
        let _guard = self.inner.import_lock.lock().await;
        let source_kind = payload.source_kind.unwrap_or(SourceKind::FilePicker);
        let sandbox::StagedPathImports { workspace: _workspace, paths } =
            self.inner.import_broker.stage_path_imports(payload).await?;
        let device_id = self.pairing()?.device_id;
        let mut output = Vec::with_capacity(paths.len());

        for staged_path in paths {
            let item = import::persist_staged_file(
                &self.inner.db,
                &self.inner.log_store,
                &self.inner.blobs_dir,
                &device_id,
                &staged_path,
                source_kind,
            )
            .await
            .with_context(|| format!("Failed to import {}", staged_path.display()))?;
            output.push(self.to_payload(item));
        }

        Ok(output)
    }

    pub async fn import_conversation_bundle(
        &self,
        payload: ImportConversationBundlePayload,
    ) -> AppResult<ItemPayload> {
        self.import_shared_conversation_bundle(sandbox::ShareBundleOrigin::DesktopApp, payload)
            .await
    }

    pub async fn import_shared_conversation_bundle(
        &self,
        origin: sandbox::ShareBundleOrigin,
        payload: ImportConversationBundlePayload,
    ) -> AppResult<ItemPayload> {
        let _guard = self.inner.import_lock.lock().await;
        let sandbox::StagedConversationBundle { workspace, payload } =
            self.inner.import_broker.stage_share_bundle(origin, payload).await?;
        let device_id = self.pairing()?.device_id;
        let source_kind = match origin {
            sandbox::ShareBundleOrigin::BrowserShare => SourceKind::BrowserShare,
            _ => SourceKind::Composer,
        };
        let provenance = match origin {
            sandbox::ShareBundleOrigin::BrowserShare => TrustProvenance::BrowserExtension,
            _ => TrustProvenance::Local,
        };
        let source_app = payload.source_label.clone();
        let source_url = payload.source_url.clone();
        let source_title = payload.title.clone();
        let item = bundles::persist_conversation_bundle(
            &self.inner.db,
            &self.inner.log_store,
            &self.inner.blobs_dir,
            workspace.root(),
            &device_id,
            payload,
            SmartDropSeed {
                source_kind,
                provenance,
                source_app,
                source_url,
                source_title,
            },
        )
        .await?;
        Ok(self.to_payload(item))
    }

    pub async fn inspect_conversation_bundle(
        &self,
        item_id: &str,
    ) -> AppResult<ConversationBundleDetailsPayload> {
        let Some(item) = self.inner.db.get_item(item_id)? else {
            return Err(crate::error::AppError::Message(format!("Item {item_id} was not found.")));
        };
        if !bundles::is_conversation_bundle_item(&item) {
            return Err(crate::error::AppError::Message(format!(
                "Item {item_id} is not a conversation bundle."
            )));
        }

        let sandbox::StagedBundlePreview {
            workspace: _workspace,
            bundle_path,
        } = self
            .inner
            .import_broker
            .stage_bundle_preview(&self.resolve_asset_path(&item.content_ref))
            .await?;

        bundles::inspect_bundle_archive(&bundle_path)
    }

    pub async fn read_conversation_bundle_entry(
        &self,
        item_id: &str,
        entry_path: &str,
    ) -> AppResult<ConversationBundleTextEntryPayload> {
        let Some(item) = self.inner.db.get_item(item_id)? else {
            return Err(crate::error::AppError::Message(format!("Item {item_id} was not found.")));
        };
        if !bundles::is_conversation_bundle_item(&item) {
            return Err(crate::error::AppError::Message(format!(
                "Item {item_id} is not a conversation bundle."
            )));
        }

        let sandbox::StagedBundlePreview {
            workspace: _workspace,
            bundle_path,
        } = self
            .inner
            .import_broker
            .stage_bundle_preview(&self.resolve_asset_path(&item.content_ref))
            .await?;

        bundles::read_bundle_entry_text(&bundle_path, entry_path)
    }

    pub async fn item_text(&self, item_id: &str) -> AppResult<Option<String>> {
        let item = self.inner.db.get_item(item_id)?;
        let Some(item) = item else {
            return Ok(None);
        };

        match item.item_type {
            crate::models::ItemType::Text => {
                let text_path = self.inner.base_dir.join(item.content_ref);
                let text = tokio::fs::read_to_string(text_path).await?;
                Ok(Some(text))
            }
            _ => Ok(None),
        }
    }

    pub async fn delete_item(&self, item_id: &str) -> AppResult<()> {
        let _guard = self.inner.import_lock.lock().await;
        let Some(item) = self.inner.db.get_item(item_id)? else {
            return Ok(());
        };

        let remaining_refs = self.inner.db.count_items_with_content_ref(&item.content_ref)?;
        self.inner.db.delete_item(item_id)?;
        self.inner
            .log_store
            .append("delete", item_id, serde_json::json!({ "item_id": item_id, "device_id": self.pairing()?.device_id }))?;

        if remaining_refs <= 1 {
            let absolute = self.resolve_asset_path(&item.content_ref);
            let _ = tokio::fs::remove_file(absolute).await;
        }

        Ok(())
    }

    pub async fn update_item_intent_state(
        &self,
        item_id: &str,
        intent_state: IntentState,
    ) -> AppResult<Option<ItemPayload>> {
        let _guard = self.inner.import_lock.lock().await;
        let Some(mut item) = self.inner.db.get_item(item_id)? else {
            return Ok(None);
        };

        smart_drops::apply_intent_state(&mut item, intent_state, Utc::now());
        self.inner.db.upsert_item(&item)?;
        self.inner
            .log_store
            .append("upsert", item_id, serde_json::to_value(&item)?)?;

        Ok(Some(self.to_payload(item)))
    }

    pub async fn export_item(&self, item_id: &str, destination_path: &str) -> AppResult<()> {
        let Some(item) = self.inner.db.get_item(item_id)? else {
            return Ok(());
        };

        let destination = PathBuf::from(destination_path);
        self.write_item_to_destination(&item, &destination).await
    }

    pub async fn export_item_to_downloads(&self, item_id: &str) -> AppResult<String> {
        let Some(item) = self.inner.db.get_item(item_id)? else {
            return Ok(String::new());
        };

        let downloads_dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .ok_or_else(|| crate::error::AppError::Message("Unable to resolve Downloads directory".into()))?;
        let destination = next_available_download_path(&downloads_dir, &preferred_download_name(&item));
        self.write_item_to_destination(&item, &destination).await?;
        Ok(destination.to_string_lossy().to_string())
    }

    pub async fn open_item(&self, item_id: &str) -> AppResult<()> {
        let Some(item) = self.inner.db.get_item(item_id)? else {
            return Ok(());
        };

        let path = self.resolve_asset_path(&item.content_ref);
        open_path(&path)
    }

    async fn write_item_to_destination(&self, item: &Item, destination: &Path) -> AppResult<()> {
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        match item.item_type {
            ItemType::Text => {
                let source = self.inner.base_dir.join(&item.content_ref);
                let bytes = tokio::fs::read(source).await?;
                tokio::fs::write(destination, bytes).await?;
            }
            ItemType::Image | ItemType::File => {
                let source = self.resolve_asset_path(&item.content_ref);
                tokio::fs::copy(source, destination).await?;
            }
        }

        Ok(())
    }

    pub fn resolve_asset_path(&self, relative: &str) -> PathBuf {
        self.inner.base_dir.join(relative)
    }

    fn to_payload(&self, item: Item) -> ItemPayload {
        let item_type = item.item_type.clone();
        let text_content = match item_type {
            ItemType::Text => std::fs::read_to_string(self.inner.base_dir.join(&item.content_ref)).ok(),
            ItemType::Image | ItemType::File => None,
        };
        let content_ref = match item.item_type {
            ItemType::Text => item.content_ref.clone(),
            ItemType::Image | ItemType::File => self
                .resolve_asset_path(&item.content_ref)
                .to_string_lossy()
                .to_string(),
        };

        ItemPayload {
            id: item.id,
            item_type: item_type.clone(),
            content_ref,
            storage_path: match item_type {
                ItemType::Text => None,
                ItemType::Image | ItemType::File => Some(item.content_ref),
            },
            created_at: item.created_at,
            updated_at: item.updated_at,
            device_id: item.device_id,
            name: item.name,
            mime_type: item.mime_type,
            size_bytes: item.size_bytes,
            sha256: item.sha256,
            text_preview: item.text_preview,
            text_content,
            source_context: item.source_context,
            semantic_context: item.semantic_context,
            suggested_actions: item.suggested_actions,
            intent_state: item.intent_state,
            trust_context: item.trust_context,
        }
    }
}

fn resolve_base_dir(app_name: &str) -> AppResult<PathBuf> {
    let local = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .ok_or_else(|| crate::error::AppError::Message("Unable to resolve data directory".into()))?;

    let root = local.join(app_name.to_lowercase());
    if let Err(error) = std::fs::create_dir_all(&root) {
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(AppError::Message(format!(
                "failed to create Dropply base directory at {}: {} (kind: {:?}, raw_os_error: {:?})",
                root.display(),
                error,
                error.kind(),
                error.raw_os_error()
            )));
        }
    }
    Ok(root)
}

pub fn next_relative_path(prefix: &str, extension: Option<&str>) -> String {
    let timestamp = Utc::now().timestamp_millis();
    let id = Uuid::new_v4();
    match extension {
        Some(ext) if !ext.is_empty() => format!("{prefix}/{timestamp}-{id}.{ext}"),
        _ => format!("{prefix}/{timestamp}-{id}"),
    }
}

fn preferred_download_name(item: &Item) -> String {
    if let Some(name) = item.name.as_deref() {
        let sanitized = sanitize_file_name(name);
        if !sanitized.is_empty() {
            return sanitized;
        }
    }

    if bundles::is_conversation_bundle_item(item) {
        return format!("conversation-bundle.{}", bundles::CONVERSATION_BUNDLE_EXTENSION);
    }

    match item.item_type {
        ItemType::Text => "dropply-note.txt".to_string(),
        ItemType::Image => "dropply-image".to_string(),
        ItemType::File => "dropply-file".to_string(),
    }
}

fn open_path(path: &Path) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }
}

pub fn sanitize_file_name(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => ch,
        })
        .collect::<String>();

    sanitized.trim().trim_matches('.').to_string()
}

fn next_available_download_path(root: &Path, file_name: &str) -> PathBuf {
    let initial = root.join(file_name);
    if !initial.exists() {
        return initial;
    }

    let parsed = Path::new(file_name);
    let stem = parsed
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("dropply-download");
    let ext = parsed.extension().and_then(|value| value.to_str()).unwrap_or("");

    for index in 2.. {
        let candidate_name = if ext.is_empty() {
            format!("{stem} ({index})")
        } else {
            format!("{stem} ({index}).{ext}")
        };
        let candidate = root.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("download path search should always find an available name")
}
