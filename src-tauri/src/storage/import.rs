use std::path::Path;

use anyhow::Context;
use chrono::Utc;
use uuid::Uuid;

#[cfg(feature = "zenith")]
use zenith_core::equation::{EquationEngine, CausalEquation};

use crate::error::AppResult;
use crate::models::{Item, ItemType, SourceKind};
use crate::storage::{blobs, bundles, db::Database, next_relative_path, smart_drops};
use crate::storage::smart_drops::SmartDropSeed;
use crate::sync::log::LogStore;

pub fn persist_text(
    db: &Database,
    log_store: &LogStore,
    base_dir: &Path,
    device_id: &str,
    text: String,
    provided_id: Option<String>,
    source_kind: SourceKind,
) -> AppResult<Item> {
    let now = Utc::now();
    let id = provided_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let relative = next_relative_path("notes", Some("txt"));
    std::fs::create_dir_all(base_dir.join("notes"))?;
    std::fs::write(base_dir.join(&relative), &text)?;

    let mut item = Item {
        id: id.clone(),
        item_type: ItemType::Text,
        content_ref: relative,
        created_at: now,
        updated_at: now,
        device_id: device_id.to_string(),
        name: Some("Pasted text".into()),
        mime_type: Some("text/plain".into()),
        size_bytes: Some(text.len() as i64),
        sha256: None,
        text_preview: Some(text.chars().take(512).collect()),
        source_context: None,
        semantic_context: None,
        suggested_actions: Vec::new(),
        intent_state: Default::default(),
        trust_context: None,
    };
    smart_drops::apply_new_item_metadata(&mut item, SmartDropSeed::local(source_kind));

    db.upsert_item(&item)?;
    log_store.append("upsert", &id, serde_json::to_value(&item)?)?;
    Ok(item)
}

pub async fn persist_relay_item(
    db: &Database,
    log_store: &LogStore,
    base_dir: &Path,
    blobs_dir: &Path,
    payload: crate::models::RelayItemPayload,
) -> AppResult<Item> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    let item_id = payload.id.clone();
    let expected_size = payload.size_bytes;
    let content_ref = match payload.item_type {
        ItemType::Text => {
            let relative = next_relative_path("notes", Some("txt"));
            std::fs::create_dir_all(base_dir.join("notes"))?;
            std::fs::write(base_dir.join(&relative), payload.text_content.as_deref().unwrap_or(""))?;
            relative
        }
        ItemType::Image | ItemType::File => {
            if let Some(b64) = payload.bytes_b64 {
                let bytes = BASE64
                    .decode(&b64)
                    .with_context(|| format!("Invalid relay payload encoding for item {item_id}"))?;

                if let Some(expected_size) = expected_size {
                    if expected_size >= 0 && expected_size as usize != bytes.len() {
                        return Err(crate::error::AppError::Message(format!(
                            "Relay payload size mismatch for item {item_id}"
                        )));
                    }
                }
                
                #[cfg(feature = "zenith")]
                if let Some(equation_val) = payload.zenith_equation {
                    if let Ok(equation) = serde_json::from_value::<CausalEquation>(equation_val) {
                        let engine = EquationEngine::new();
                        if !engine.verify(&equation, &bytes) {
                            return Err(crate::error::AppError::Message("ZENITH AUDIT FAILED: Cryptographic tampering detected".into()));
                        }
                        println!("ZENITH AUDIT PASSED: Causal synthesis verified.");
                    }
                }

                let hash = payload.sha256.clone().unwrap_or_else(|| {
                    use sha2::{Digest, Sha256};
                    let mut hasher = Sha256::new();
                    hasher.update(&bytes);
                    format!("{:x}", hasher.finalize())
                });
                
                let extension = payload
                    .name
                    .as_deref()
                    .and_then(|name| Path::new(name).extension())
                    .and_then(|value| value.to_str())
                    .filter(|ext| !ext.is_empty());
                let file_name = extension
                    .map(|ext| format!("{hash}.{ext}"))
                    .unwrap_or_else(|| hash.clone());
                let file_path = blobs_dir.join(&file_name);
                if !file_path.exists() {
                    tokio::fs::write(&file_path, bytes).await?;
                }
                format!("blobs/{}", file_name)
            } else {
                return Err(crate::error::AppError::Message(format!(
                    "Relay item {item_id} is missing file bytes"
                )));
            }
        }
    };

    let mut item = Item {
        id: payload.id.clone(),
        item_type: payload.item_type,
        content_ref,
        created_at: payload.updated_at, // Use updated_at as proxy for created_at if missing
        updated_at: payload.updated_at,
        device_id: payload.device_id,
        name: payload.name,
        mime_type: payload.mime_type,
        size_bytes: payload.size_bytes,
        sha256: payload.sha256,
        text_preview: payload.text_content.map(|t| t.chars().take(512).collect()),
        source_context: payload.source_context,
        semantic_context: payload.semantic_context,
        suggested_actions: payload.suggested_actions,
        intent_state: payload.intent_state,
        trust_context: payload.trust_context,
    };
    smart_drops::ensure_item_metadata(&mut item, SmartDropSeed::paired(SourceKind::Relay));

    db.upsert_item(&item)?;
    log_store.append("upsert", &item.id, serde_json::to_value(&item)?)?;
    Ok(item)
}

pub async fn persist_file(
    db: &Database,
    log_store: &LogStore,
    blobs_dir: &Path,
    device_id: &str,
    source: &Path,
) -> AppResult<Item> {
    let name = source.file_name().and_then(|value| value.to_str()).map(str::to_string);
    let extension = source.extension().and_then(|value| value.to_str()).unwrap_or("");
    let mime = guess_mime(extension);
    let item_type = if mime.starts_with("image/") {
        ItemType::Image
    } else {
        ItemType::File
    };
    let (hash, relative_path, size) = blobs::persist_blob(blobs_dir, source)
        .await
        .with_context(|| format!("Unable to persist {}", source.display()))?;
    let relative_ref = relative_path.to_string_lossy().replace('\\', "/");

    upsert_persisted_file(
        db,
        log_store,
        device_id,
        relative_ref,
        item_type,
        name,
        mime,
        size,
        hash,
        SourceKind::FilePicker,
    )
}

pub async fn persist_staged_file(
    db: &Database,
    log_store: &LogStore,
    blobs_dir: &Path,
    device_id: &str,
    staged_path: &Path,
    source_kind: SourceKind,
) -> AppResult<Item> {
    let name = staged_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_string);
    let extension = staged_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mime = guess_mime(extension);
    let item_type = if mime.starts_with("image/") {
        ItemType::Image
    } else {
        ItemType::File
    };
    let (hash, relative_path, size) = blobs::persist_owned_blob(
        blobs_dir,
        staged_path,
        staged_path.extension().and_then(|value| value.to_str()),
    )
    .await
    .with_context(|| format!("Unable to persist {}", staged_path.display()))?;
    let relative_ref = relative_path.to_string_lossy().replace('\\', "/");

    upsert_persisted_file(
        db,
        log_store,
        device_id,
        relative_ref,
        item_type,
        name,
        mime,
        size,
        hash,
        source_kind,
    )
}

fn upsert_persisted_file(
    db: &Database,
    log_store: &LogStore,
    device_id: &str,
    relative_ref: String,
    item_type: ItemType,
    name: Option<String>,
    mime: &str,
    size: u64,
    hash: String,
    source_kind: SourceKind,
) -> AppResult<Item> {
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();

    if let Some(existing) = db.find_latest_item_by_content_ref_and_device(&relative_ref, device_id)? {
        let mut item = Item {
            id: existing.id.clone(),
            item_type,
            content_ref: relative_ref,
            created_at: existing.created_at,
            updated_at: now,
            device_id: device_id.to_string(),
            name,
            mime_type: Some(mime.into()),
            size_bytes: Some(size as i64),
            sha256: Some(hash),
            text_preview: None,
            source_context: existing.source_context,
            semantic_context: existing.semantic_context,
            suggested_actions: existing.suggested_actions,
            intent_state: existing.intent_state,
            trust_context: existing.trust_context,
        };
        smart_drops::ensure_item_metadata(&mut item, SmartDropSeed::local(source_kind));

        db.upsert_item(&item)?;
        log_store.append("upsert", &item.id, serde_json::to_value(&item)?)?;
        return Ok(item);
    }

    let mut item = Item {
        id: id.clone(),
        item_type,
        content_ref: relative_ref,
        created_at: now,
        updated_at: now,
        device_id: device_id.to_string(),
        name,
        mime_type: Some(mime.into()),
        size_bytes: Some(size as i64),
        sha256: Some(hash),
        text_preview: None,
        source_context: None,
        semantic_context: None,
        suggested_actions: Vec::new(),
        intent_state: Default::default(),
        trust_context: None,
    };
    smart_drops::apply_new_item_metadata(&mut item, SmartDropSeed::local(source_kind));

    db.upsert_item(&item)?;
    log_store.append("upsert", &id, serde_json::to_value(&item)?)?;
    Ok(item)
}

pub async fn persist_staged_relay_item(
    db: &Database,
    log_store: &LogStore,
    base_dir: &Path,
    blobs_dir: &Path,
    payload: crate::models::RelayItemPayload,
    staged_path: &Path,
) -> AppResult<Item> {
    let item_id = payload.id.clone();
    let now = payload.updated_at;

    let (content_ref, size_bytes, sha256, text_preview) = match payload.item_type.clone() {
        ItemType::Text => {
            let relative = next_relative_path("notes", Some("txt"));
            std::fs::create_dir_all(base_dir.join("notes"))?;
            let text = payload.text_content.as_deref().unwrap_or("");
            std::fs::write(base_dir.join(&relative), text)?;
            (
                relative,
                Some(text.len() as i64),
                None,
                Some(text.chars().take(512).collect()),
            )
        }
        ItemType::Image | ItemType::File => {
            if !tokio::fs::try_exists(staged_path).await? {
                return Err(crate::error::AppError::Message(format!(
                    "Staged transfer file missing for item {item_id}"
                )));
            }

            let preferred_extension = payload
                .name
                .as_deref()
                .and_then(|name| Path::new(name).extension())
                .and_then(|value| value.to_str())
                .filter(|ext| !ext.is_empty());

            let (hash, relative_path, size) =
                blobs::persist_owned_blob(blobs_dir, staged_path, preferred_extension).await?;

            if let Some(expected_size) = payload.size_bytes {
                if expected_size >= 0 && expected_size as u64 != size {
                    return Err(crate::error::AppError::Message(format!(
                        "Direct transfer size mismatch for item {item_id}"
                    )));
                }
            }

            if let Some(expected_hash) = payload.sha256.as_deref() {
                if !expected_hash.eq_ignore_ascii_case(&hash) {
                    return Err(crate::error::AppError::Message(format!(
                        "Direct transfer checksum mismatch for item {item_id}"
                    )));
                }
            }

            (
                relative_path.to_string_lossy().replace('\\', "/"),
                Some(size as i64),
                Some(hash),
                None,
            )
        }
    };

    let mut item = Item {
        id: payload.id.clone(),
        item_type: payload.item_type,
        content_ref,
        created_at: now,
        updated_at: now,
        device_id: payload.device_id,
        name: payload.name,
        mime_type: payload.mime_type,
        size_bytes: payload.size_bytes.or(size_bytes),
        sha256: payload.sha256.or(sha256),
        text_preview,
        source_context: payload.source_context,
        semantic_context: payload.semantic_context,
        suggested_actions: payload.suggested_actions,
        intent_state: payload.intent_state,
        trust_context: payload.trust_context,
    };
    smart_drops::ensure_item_metadata(&mut item, SmartDropSeed::paired(SourceKind::Direct));

    db.upsert_item(&item)?;
    log_store.append("upsert", &item.id, serde_json::to_value(&item)?)?;
    Ok(item)
}

fn guess_mime(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "m4v" => "video/x-m4v",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        "txt" | "md" | "json" | "rs" | "ts" | "tsx" | "js" => "text/plain",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "dropplybundle" => bundles::CONVERSATION_BUNDLE_MIME_TYPE,
        _ => "application/octet-stream",
    }
}
