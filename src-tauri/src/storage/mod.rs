pub mod blobs;
pub mod db;
pub mod import;

use std::path::{Path, PathBuf};

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{ImportPathPayload, Item, ItemPayload, ItemType, PairingInfo, RelayItemPayload};
use crate::sync::log::LogStore;

#[derive(Clone)]
pub struct Storage {
    inner: std::sync::Arc<StorageInner>,
}

struct StorageInner {
    base_dir: PathBuf,
    blobs_dir: PathBuf,
    db: db::Database,
    pairing: PairingInfo,
    import_lock: Mutex<()>,
    log_store: LogStore,
}

impl Storage {
    pub async fn new(app_name: &str) -> AppResult<Self> {
        let base_dir = resolve_base_dir(app_name)?;
        let blobs_dir = base_dir.join("blobs");
        tokio::fs::create_dir_all(&blobs_dir).await?;

        let db_path = base_dir.join("dropply.sqlite3");
        let db = db::Database::open(&db_path)?;
        db.migrate()?;

        let pairing = db.load_or_create_pairing()?;
        let log_store = LogStore::new(db.clone());

        Ok(Self {
            inner: std::sync::Arc::new(StorageInner {
                base_dir,
                blobs_dir,
                db,
                pairing,
                import_lock: Mutex::new(()),
                log_store,
            }),
        })
    }

    pub fn base_dir(&self) -> &Path {
        &self.inner.base_dir
    }

    pub fn pairing(&self) -> PairingInfo {
        self.inner.pairing.clone()
    }

    pub fn log_store(&self) -> LogStore {
        self.inner.log_store.clone()
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
                    });
                }
                ItemType::Image | ItemType::File => {
                    let bytes = tokio::fs::read(self.resolve_asset_path(&item.content_ref)).await?;
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
                        bytes_b64: Some(BASE64.encode(bytes)),
                    });
                }
            }
        }

        Ok(output)
    }

    pub fn item_count(&self) -> AppResult<usize> {
        Ok(self.inner.db.count_items()? as usize)
    }

    pub async fn import_text(&self, text: String) -> AppResult<ItemPayload> {
        let _guard = self.inner.import_lock.lock().await;
        let item = import::persist_text(
            &self.inner.db,
            &self.inner.log_store,
            &self.inner.base_dir,
            &self.inner.pairing.device_id,
            text,
        )?;
        Ok(self.to_payload(item))
    }

    pub async fn import_paths(&self, payload: ImportPathPayload) -> AppResult<Vec<ItemPayload>> {
        let _guard = self.inner.import_lock.lock().await;
        let mut output = Vec::with_capacity(payload.paths.len());

        for raw_path in payload.paths {
            let source = PathBuf::from(raw_path);
            if !source.exists() {
                continue;
            }

            let item = import::persist_file(
                &self.inner.db,
                &self.inner.log_store,
                &self.inner.blobs_dir,
                &self.inner.pairing.device_id,
                &source,
            )
            .await
            .with_context(|| format!("Failed to import {}", source.display()))?;
            output.push(self.to_payload(item));
        }

        Ok(output)
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

    pub async fn upsert_remote_item(&self, item: Item) -> AppResult<()> {
        self.inner.db.upsert_item(&item)?;
        self.inner
            .log_store
            .append("replicated", &item.id, serde_json::to_value(&item)?)?;
        Ok(())
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
            .append("delete", item_id, serde_json::json!({ "item_id": item_id, "device_id": self.inner.pairing.device_id }))?;

        if remaining_refs <= 1 {
            let absolute = self.resolve_asset_path(&item.content_ref);
            let _ = tokio::fs::remove_file(absolute).await;
        }

        Ok(())
    }

    pub async fn export_item(&self, item_id: &str, destination_path: &str) -> AppResult<()> {
        let Some(item) = self.inner.db.get_item(item_id)? else {
            return Ok(());
        };

        let destination = PathBuf::from(destination_path);
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        match item.item_type {
            ItemType::Text => {
                let source = self.inner.base_dir.join(item.content_ref);
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
        let content_ref = match item.item_type {
            ItemType::Text => item.content_ref.clone(),
            ItemType::Image | ItemType::File => self
                .resolve_asset_path(&item.content_ref)
                .to_string_lossy()
                .to_string(),
        };

        ItemPayload {
            id: item.id,
            item_type: item.item_type,
            content_ref,
            created_at: item.created_at,
            updated_at: item.updated_at,
            device_id: item.device_id,
            name: item.name,
            mime_type: item.mime_type,
            size_bytes: item.size_bytes,
            sha256: item.sha256,
            text_preview: item.text_preview,
        }
    }
}

fn resolve_base_dir(app_name: &str) -> AppResult<PathBuf> {
    let local = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .ok_or_else(|| crate::error::AppError::Message("Unable to resolve data directory".into()))?;

    let root = local.join(app_name.to_lowercase());
    std::fs::create_dir_all(&root)?;
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
