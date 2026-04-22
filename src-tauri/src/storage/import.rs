use std::path::Path;

use anyhow::Context;
use chrono::Utc;
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{Item, ItemType};
use crate::storage::{blobs, db::Database, next_relative_path};
use crate::sync::log::LogStore;

pub fn persist_text(
    db: &Database,
    log_store: &LogStore,
    base_dir: &Path,
    device_id: &str,
    text: String,
) -> AppResult<Item> {
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let relative = next_relative_path("notes", Some("txt"));
    std::fs::create_dir_all(base_dir.join("notes"))?;
    std::fs::write(base_dir.join(&relative), &text)?;

    let item = Item {
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
    };

    db.upsert_item(&item)?;
    log_store.append("upsert", &id, serde_json::to_value(&item)?)?;
    Ok(item)
}

pub async fn persist_file(
    db: &Database,
    log_store: &LogStore,
    blobs_dir: &Path,
    device_id: &str,
    source: &Path,
) -> AppResult<Item> {
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
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

    let item = Item {
        id: id.clone(),
        item_type,
        content_ref: relative_path.to_string_lossy().replace('\\', "/"),
        created_at: now,
        updated_at: now,
        device_id: device_id.to_string(),
        name,
        mime_type: Some(mime.into()),
        size_bytes: Some(size as i64),
        sha256: Some(hash),
        text_preview: None,
    };

    db.upsert_item(&item)?;
    log_store.append("upsert", &id, serde_json::to_value(&item)?)?;
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
        "txt" | "md" | "json" | "rs" | "ts" | "tsx" | "js" => "text/plain",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}
