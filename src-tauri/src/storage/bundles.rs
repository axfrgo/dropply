use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{AppError, AppResult};
use crate::models::{
    ConversationBundleDetailsPayload, ConversationBundleEntryPayload, ConversationBundleEntryRole,
    ConversationBundleManifestPayload, ConversationBundleSourcePayload,
    ConversationBundleTextEntryPayload, ImportConversationBundlePayload, Item, ItemType,
};
use crate::storage::{blobs, db::Database, sanitize_file_name, smart_drops};
use crate::storage::smart_drops::SmartDropSeed;
use crate::sync::log::LogStore;

pub const CONVERSATION_BUNDLE_MIME_TYPE: &str = "application/vnd.dropply.conversation-bundle+zip";
pub const CONVERSATION_BUNDLE_EXTENSION: &str = "dropplybundle";

const BUNDLE_VERSION: &str = "1";
const MANIFEST_PATH: &str = "manifest.json";
const TRANSCRIPT_PATH: &str = "conversation.md";
pub const MAX_PREVIEW_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_BUNDLE_ENTRY_COUNT: usize = 128;
pub const MAX_BUNDLE_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_BUNDLE_TOTAL_BYTES: u64 = 128 * 1024 * 1024;

pub async fn persist_conversation_bundle(
    db: &Database,
    log_store: &LogStore,
    blobs_dir: &Path,
    temp_root: &Path,
    device_id: &str,
    payload: ImportConversationBundlePayload,
    seed: SmartDropSeed,
) -> AppResult<Item> {
    if payload.transcript_markdown.trim().is_empty() {
        return Err(AppError::Message(
            "Conversation bundles require a transcript.".into(),
        ));
    }

    let now = Utc::now();
    let title = payload
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            payload
                .source_label
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| format!("{value} conversation"))
    })
        .unwrap_or_else(|| "Conversation bundle".to_string());
    let file_name = default_bundle_file_name(&title, now);
    let item_id = Uuid::new_v4().to_string();
    std::fs::create_dir_all(temp_root).with_context(|| {
        format!(
            "Unable to create conversation bundle staging directory at {}",
            temp_root.display()
        )
    })?;
    let temp_path = temp_root.join(format!(".bundle-{}.tmp", Uuid::new_v4()));

    build_bundle_archive(&temp_path, &title, now, &payload)?;
    verify_bundle_archive(&temp_path)?;

    let (hash, relative_path, size) =
        blobs::persist_owned_blob(blobs_dir, &temp_path, Some(CONVERSATION_BUNDLE_EXTENSION)).await?;
    let relative_ref = relative_path.to_string_lossy().replace('\\', "/");

    let mut item = Item {
        id: item_id.clone(),
        item_type: ItemType::File,
        content_ref: relative_ref,
        created_at: now,
        updated_at: now,
        device_id: device_id.to_string(),
        name: Some(file_name),
        mime_type: Some(CONVERSATION_BUNDLE_MIME_TYPE.to_string()),
        size_bytes: Some(size as i64),
        sha256: Some(hash),
        text_preview: Some(payload.transcript_markdown.chars().take(512).collect()),
        source_context: None,
        semantic_context: None,
        suggested_actions: Vec::new(),
        intent_state: Default::default(),
        trust_context: None,
    };
    smart_drops::apply_new_item_metadata(&mut item, seed);

    db.upsert_item(&item)?;
    log_store.append("upsert", &item_id, serde_json::to_value(&item)?)?;
    Ok(item)
}

pub fn verify_bundle_archive(path: &Path) -> AppResult<()> {
    let _ = inspect_archive(path)?;
    Ok(())
}

pub fn inspect_bundle_archive(path: &Path) -> AppResult<ConversationBundleDetailsPayload> {
    let (mut archive, manifest) = inspect_archive(path)?;
    let transcript_markdown = read_text_from_archive(&mut archive, &manifest.transcript_path)?;
    Ok(ConversationBundleDetailsPayload {
        manifest,
        transcript_markdown,
    })
}

pub fn read_bundle_entry_text(path: &Path, entry_path: &str) -> AppResult<ConversationBundleTextEntryPayload> {
    let (mut archive, manifest) = inspect_archive(path)?;
    let entry = manifest
        .entries
        .iter()
        .find(|entry| entry.path == entry_path)
        .cloned()
        .ok_or_else(|| AppError::Message(format!("Bundle entry '{entry_path}' was not found.")))?;
    let content = read_text_from_archive(&mut archive, &entry.path)?;

    Ok(ConversationBundleTextEntryPayload {
        path: entry.path,
        mime_type: entry.mime_type,
        content,
    })
}

pub fn is_conversation_bundle_item(item: &Item) -> bool {
    matches!(item.item_type, ItemType::File)
        && (item.mime_type.as_deref() == Some(CONVERSATION_BUNDLE_MIME_TYPE)
            || item
                .name
                .as_deref()
                .map(is_conversation_bundle_name)
                .unwrap_or(false))
}

pub fn is_conversation_bundle_name(name: &str) -> bool {
    name.to_ascii_lowercase()
        .ends_with(&format!(".{CONVERSATION_BUNDLE_EXTENSION}"))
}

fn build_bundle_archive(
    destination: &Path,
    title: &str,
    created_at: DateTime<Utc>,
    payload: &ImportConversationBundlePayload,
) -> AppResult<()> {
    let file = File::create(destination)
        .with_context(|| format!("Unable to create bundle archive at {}", destination.display()))?;
    let mut archive = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let mut used_paths = HashSet::new();

    let transcript_bytes = payload.transcript_markdown.as_bytes();
    archive.start_file(TRANSCRIPT_PATH, options)?;
    archive.write_all(transcript_bytes)?;
    used_paths.insert(TRANSCRIPT_PATH.to_string());

    let mut entries = Vec::new();
    entries.extend(add_sources_to_archive(
        &mut archive,
        options,
        &mut used_paths,
        "files",
        ConversationBundleEntryRole::Reference,
        &payload.files,
    )?);
    entries.extend(add_sources_to_archive(
        &mut archive,
        options,
        &mut used_paths,
        "attachments",
        ConversationBundleEntryRole::Attachment,
        &payload.attachments,
    )?);

    let manifest = ConversationBundleManifestPayload {
        bundle_version: BUNDLE_VERSION.to_string(),
        title: title.to_string(),
        source_label: payload.source_label.clone(),
        source_url: payload.source_url.clone(),
        created_at,
        transcript_path: TRANSCRIPT_PATH.to_string(),
        transcript_sha256: sha256_hex(transcript_bytes),
        entries,
    };

    archive.start_file(MANIFEST_PATH, options)?;
    archive.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
    archive.finish()?;
    Ok(())
}

fn add_sources_to_archive(
    archive: &mut ZipWriter<File>,
    options: FileOptions,
    used_paths: &mut HashSet<String>,
    section_root: &str,
    role: ConversationBundleEntryRole,
    sources: &[ConversationBundleSourcePayload],
) -> AppResult<Vec<ConversationBundleEntryPayload>> {
    let mut entries = Vec::with_capacity(sources.len());

    for source in sources {
        let source_path = PathBuf::from(
            source
                .path
                .as_deref()
                .ok_or_else(|| AppError::Message("Conversation bundle source is missing a staged file path.".into()))?,
        );
        if !source_path.exists() {
            return Err(AppError::Message(format!(
                "Bundle source '{}' does not exist.",
                source.path.clone().unwrap_or_else(|| "unknown source".to_string())
            )));
        }
        if !source_path.is_file() {
            return Err(AppError::Message(format!(
                "Bundle source '{}' is not a file.",
                source.path.clone().unwrap_or_else(|| "unknown source".to_string())
            )));
        }

        let requested_path = source
            .archive_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(sanitize_archive_fragment)
            .unwrap_or_else(|| default_archive_leaf(&source_path));
        let archive_path = ensure_unique_archive_path(
            &format!("{section_root}/{requested_path}"),
            used_paths,
        );
        let mime_type = source
            .mime_type
            .clone()
            .or_else(|| guess_mime_from_path(&source_path).map(str::to_string));
        let size_bytes = std::fs::metadata(&source_path)
            .with_context(|| format!("Unable to read metadata for {}", source_path.display()))?
            .len() as i64;
        let name = source
            .name
            .clone()
            .or_else(|| {
                source_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "attachment".to_string());

        archive.start_file(&archive_path, options)?;
        let mut file = File::open(&source_path)
            .with_context(|| format!("Unable to open {}", source_path.display()))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).with_context(|| {
            format!(
                "Unable to read '{}' while creating the conversation bundle.",
                source_path.display()
            )
        })?;
        archive.write_all(&bytes).with_context(|| {
            format!(
                "Unable to write '{}' into bundle archive.",
                source_path.display()
            )
        })?;

        entries.push(ConversationBundleEntryPayload {
            path: archive_path,
            role: role.clone(),
            name,
            mime_type,
            size_bytes,
            sha256: sha256_hex(&bytes),
        });
    }

    Ok(entries)
}

fn open_archive(path: &Path) -> AppResult<ZipArchive<File>> {
    let file = File::open(path)
        .with_context(|| format!("Unable to open conversation bundle at {}", path.display()))?;
    ZipArchive::new(file).context("Unable to read conversation bundle archive").map_err(Into::into)
}

fn inspect_archive(path: &Path) -> AppResult<(ZipArchive<File>, ConversationBundleManifestPayload)> {
    let archive_file_size = std::fs::metadata(path)
        .with_context(|| format!("Unable to stat conversation bundle at {}", path.display()))?
        .len();
    if archive_file_size > MAX_BUNDLE_TOTAL_BYTES {
        return Err(AppError::Message(format!(
            "Conversation bundle exceeds the {} MB sandbox limit.",
            MAX_BUNDLE_TOTAL_BYTES / (1024 * 1024)
        )));
    }

    let mut archive = open_archive(path)?;
    let manifest = read_manifest(&mut archive)?;
    verify_archive_against_manifest(&mut archive, &manifest)?;
    Ok((archive, manifest))
}

fn read_manifest(archive: &mut ZipArchive<File>) -> AppResult<ConversationBundleManifestPayload> {
    let mut manifest_file = archive
        .by_name(MANIFEST_PATH)
        .context("Conversation bundle is missing manifest.json")?;
    if manifest_file.size() > MAX_PREVIEW_TEXT_BYTES as u64 {
        return Err(AppError::Message(
            "Conversation bundle manifest is too large to inspect safely.".into(),
        ));
    }
    let mut manifest_json = String::new();
    manifest_file.read_to_string(&mut manifest_json)?;
    let manifest: ConversationBundleManifestPayload = serde_json::from_str(&manifest_json)
        .context("Conversation bundle manifest could not be parsed")?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn read_text_from_archive(archive: &mut ZipArchive<File>, entry_path: &str) -> AppResult<String> {
    let bytes = read_archive_bytes(archive, entry_path, None)?;
    if bytes.len() > MAX_PREVIEW_TEXT_BYTES {
        return Err(AppError::Message(format!(
            "Bundle entry '{entry_path}' is too large to preview in-app."
        )));
    }
    String::from_utf8(bytes)
        .with_context(|| format!("Bundle entry '{entry_path}' is not valid UTF-8"))
        .map_err(Into::into)
}

fn validate_manifest(manifest: &ConversationBundleManifestPayload) -> AppResult<()> {
    if manifest.bundle_version.trim().is_empty() {
        return Err(AppError::Message(
            "Conversation bundle manifest is missing bundle_version.".into(),
        ));
    }

    if manifest.title.trim().is_empty() {
        return Err(AppError::Message(
            "Conversation bundle manifest is missing a title.".into(),
        ));
    }

    if manifest.transcript_path != sanitize_archive_fragment(&manifest.transcript_path)
        || manifest.transcript_path != TRANSCRIPT_PATH
    {
        return Err(AppError::Message(
            "Conversation bundle transcript_path is invalid.".into(),
        ));
    }
    validate_sha256_hex(
        &manifest.transcript_sha256,
        "Conversation bundle transcript hash is invalid.",
    )?;
    if manifest.entries.len() > MAX_BUNDLE_ENTRY_COUNT {
        return Err(AppError::Message(format!(
            "Conversation bundle exceeds the {} file entry limit.",
            MAX_BUNDLE_ENTRY_COUNT
        )));
    }

    let mut seen_paths = HashSet::from([
        MANIFEST_PATH.to_string(),
        manifest.transcript_path.clone(),
    ]);

    for entry in &manifest.entries {
        validate_manifest_entry(entry, &mut seen_paths)?;
    }

    Ok(())
}

fn validate_manifest_entry(
    entry: &ConversationBundleEntryPayload,
    seen_paths: &mut HashSet<String>,
) -> AppResult<()> {
    if entry.name.trim().is_empty() {
        return Err(AppError::Message(format!(
            "Conversation bundle entry '{}' is missing a display name.",
            entry.path
        )));
    }

    if entry.size_bytes < 0 {
        return Err(AppError::Message(format!(
            "Conversation bundle entry '{}' has an invalid size.",
            entry.path
        )));
    }
    if entry.size_bytes as u64 > MAX_BUNDLE_ENTRY_BYTES {
        return Err(AppError::Message(format!(
            "Conversation bundle entry '{}' exceeds the {} MB per-file limit.",
            entry.path,
            MAX_BUNDLE_ENTRY_BYTES / (1024 * 1024)
        )));
    }
    validate_sha256_hex(
        &entry.sha256,
        &format!("Conversation bundle entry '{}' has an invalid hash.", entry.path),
    )?;

    let normalized_path = sanitize_archive_fragment(&entry.path);
    if normalized_path != entry.path {
        return Err(AppError::Message(format!(
            "Conversation bundle entry path '{}' is invalid.",
            entry.path
        )));
    }

    let required_prefix = match entry.role {
        ConversationBundleEntryRole::Reference => "files/",
        ConversationBundleEntryRole::Attachment => "attachments/",
    };
    if !entry.path.starts_with(required_prefix) {
        return Err(AppError::Message(format!(
            "Conversation bundle entry '{}' is not inside '{}'.",
            entry.path, required_prefix
        )));
    }

    if !seen_paths.insert(entry.path.clone()) {
        return Err(AppError::Message(format!(
            "Conversation bundle contains duplicate entry path '{}'.",
            entry.path
        )));
    }

    Ok(())
}

fn verify_archive_against_manifest(
    archive: &mut ZipArchive<File>,
    manifest: &ConversationBundleManifestPayload,
) -> AppResult<()> {
    let expected_entry_count = manifest.entries.len() + 2;
    if archive.len() != expected_entry_count {
        return Err(AppError::Message(format!(
            "Conversation bundle manifest expects {expected_entry_count} archive entries, but found {}.",
            archive.len()
        )));
    }

    let transcript_bytes = read_archive_bytes(archive, &manifest.transcript_path, None)?;
    if sha256_hex(&transcript_bytes) != manifest.transcript_sha256 {
        return Err(AppError::Message(
            "Conversation bundle transcript failed sandbox hash verification.".into(),
        ));
    }

    let manifest_size = archive
        .by_name(MANIFEST_PATH)
        .context("Conversation bundle is missing manifest.json")?
        .size();
    let mut total_bytes = manifest_size + transcript_bytes.len() as u64;

    for entry in &manifest.entries {
        let bytes = read_archive_bytes(archive, &entry.path, Some(entry.size_bytes as u64))?;
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        if total_bytes > MAX_BUNDLE_TOTAL_BYTES {
            return Err(AppError::Message(format!(
                "Conversation bundle exceeds the {} MB total size limit.",
                MAX_BUNDLE_TOTAL_BYTES / (1024 * 1024)
            )));
        }
        if sha256_hex(&bytes) != entry.sha256 {
            return Err(AppError::Message(format!(
                "Conversation bundle entry '{}' failed sandbox hash verification.",
                entry.path
            )));
        }
    }

    Ok(())
}

fn read_archive_bytes(
    archive: &mut ZipArchive<File>,
    entry_path: &str,
    expected_size: Option<u64>,
) -> AppResult<Vec<u8>> {
    let mut entry = archive
        .by_name(entry_path)
        .with_context(|| format!("Conversation bundle is missing '{entry_path}'"))?;
    if entry.size() > MAX_BUNDLE_ENTRY_BYTES {
        return Err(AppError::Message(format!(
            "Conversation bundle entry '{entry_path}' exceeds the {} MB per-file limit.",
            MAX_BUNDLE_ENTRY_BYTES / (1024 * 1024)
        )));
    }
    if let Some(expected_size) = expected_size {
        if entry.size() != expected_size {
            return Err(AppError::Message(format!(
                "Conversation bundle entry '{entry_path}' size does not match its manifest."
            )));
        }
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn validate_sha256_hex(value: &str, message: &str) -> AppResult<()> {
    let is_valid = value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    if is_valid {
        Ok(())
    } else {
        Err(AppError::Message(message.to_string()))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sanitize_archive_fragment(value: &str) -> String {
    let normalized = value.replace('\\', "/");
    let mut parts = Vec::new();

    for raw_part in normalized.split('/') {
        let part = raw_part.trim();
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        let clean = sanitize_file_name(part);
        if !clean.is_empty() {
            parts.push(clean);
        }
    }

    if parts.is_empty() {
        "item".to_string()
    } else {
        parts.join("/")
    }
}

fn default_archive_leaf(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_file_name)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "item".to_string())
}

fn ensure_unique_archive_path(path: &str, used_paths: &mut HashSet<String>) -> String {
    if used_paths.insert(path.to_string()) {
        return path.to_string();
    }

    let (prefix, file_name) = path
        .rsplit_once('/')
        .map(|(dir, name)| (format!("{dir}/"), name.to_string()))
        .unwrap_or_else(|| (String::new(), path.to_string()));
    let (stem, ext) = split_name_and_extension(&file_name);

    for index in 2.. {
        let candidate_name = if let Some(ext) = ext.as_deref() {
            format!("{stem}-{index}.{ext}")
        } else {
            format!("{stem}-{index}")
        };
        let candidate = format!("{prefix}{candidate_name}");
        if used_paths.insert(candidate.clone()) {
            return candidate;
        }
    }

    unreachable!("archive path collision resolution should always terminate")
}

fn split_name_and_extension(name: &str) -> (String, Option<String>) {
    if let Some((stem, ext)) = name.rsplit_once('.') {
        if !stem.is_empty() && !ext.is_empty() {
            return (stem.to_string(), Some(ext.to_string()));
        }
    }

    (name.to_string(), None)
}

fn default_bundle_file_name(title: &str, created_at: DateTime<Utc>) -> String {
    let sanitized = sanitize_file_name(title);
    let prefix = if sanitized.is_empty() {
        "conversation-bundle".to_string()
    } else {
        sanitized.replace(' ', "-")
    };
    format!(
        "{prefix}-{}.{}",
        created_at.format("%Y%m%d-%H%M%S"),
        CONVERSATION_BUNDLE_EXTENSION
    )
}

fn guess_mime_from_path(path: &Path) -> Option<&'static str> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())?;

    Some(match extension.as_str() {
        "md" => "text/markdown",
        "txt" => "text/plain",
        "json" => "application/json",
        "ts" => "text/plain",
        "tsx" => "text/plain",
        "js" => "text/plain",
        "jsx" => "text/plain",
        "rs" => "text/plain",
        "toml" => "text/plain",
        "yml" | "yaml" => "text/plain",
        "css" => "text/css",
        "html" => "text/html",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        CONVERSATION_BUNDLE_EXTENSION => CONVERSATION_BUNDLE_MIME_TYPE,
        _ => "application/octet-stream",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn sanitize_archive_fragment_removes_traversal_segments() {
        assert_eq!(
            sanitize_archive_fragment("../files/../secrets/./notes.md"),
            "files/secrets/notes.md"
        );
    }

    #[test]
    fn validate_manifest_rejects_duplicate_entry_paths() {
        let manifest = ConversationBundleManifestPayload {
            bundle_version: "1".into(),
            title: "Chat export".into(),
            source_label: None,
            source_url: None,
            created_at: Utc::now(),
            transcript_path: TRANSCRIPT_PATH.into(),
            transcript_sha256: sha256_hex(b"transcript"),
            entries: vec![
                ConversationBundleEntryPayload {
                    path: "files/code.ts".into(),
                    role: ConversationBundleEntryRole::Reference,
                    name: "code.ts".into(),
                    mime_type: Some("text/plain".into()),
                    size_bytes: 12,
                    sha256: sha256_hex(b"hello world!"),
                },
                ConversationBundleEntryPayload {
                    path: "files/code.ts".into(),
                    role: ConversationBundleEntryRole::Reference,
                    name: "duplicate.ts".into(),
                    mime_type: Some("text/plain".into()),
                    size_bytes: 12,
                    sha256: sha256_hex(b"hello world!"),
                },
            ],
        };

        let error = validate_manifest(&manifest).expect_err("manifest should be rejected");
        assert!(
            error
                .to_string()
                .contains("duplicate entry path"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn validate_manifest_rejects_role_prefix_mismatches() {
        let manifest = ConversationBundleManifestPayload {
            bundle_version: "1".into(),
            title: "Chat export".into(),
            source_label: None,
            source_url: None,
            created_at: Utc::now(),
            transcript_path: TRANSCRIPT_PATH.into(),
            transcript_sha256: sha256_hex(b"transcript"),
            entries: vec![ConversationBundleEntryPayload {
                path: "attachments/code.ts".into(),
                role: ConversationBundleEntryRole::Reference,
                name: "code.ts".into(),
                mime_type: Some("text/plain".into()),
                size_bytes: 12,
                sha256: sha256_hex(b"hello world!"),
            }],
        };

        let error = validate_manifest(&manifest).expect_err("manifest should be rejected");
        assert!(
            error.to_string().contains("is not inside"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn inspect_bundle_archive_rejects_missing_manifest() {
        let path = test_temp_path("missing-manifest.dropplybundle");
        let file = File::create(&path).expect("create zip");
        let mut archive = ZipWriter::new(file);
        let options = FileOptions::default().compression_method(CompressionMethod::Deflated);
        archive
            .start_file(TRANSCRIPT_PATH, options)
            .expect("start transcript");
        archive.write_all(b"# transcript").expect("write transcript");
        archive.finish().expect("finish zip");

        let error = inspect_bundle_archive(&path).expect_err("bundle should be rejected");
        assert!(
            error.to_string().contains("manifest.json"),
            "unexpected error: {error}"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn inspect_bundle_archive_rejects_non_zip_bytes() {
        let path = test_temp_path("corrupt-bundle.dropplybundle");
        fs::write(&path, b"this is not a zip archive").expect("write corrupt bytes");

        let error = inspect_bundle_archive(&path).expect_err("bundle should be rejected");
        assert!(
            error.to_string().contains("Unable to read conversation bundle archive"),
            "unexpected error: {error}"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn inspect_bundle_archive_rejects_hash_mismatch() {
        let path = test_temp_path("hash-mismatch.dropplybundle");
        let file = File::create(&path).expect("create zip");
        let mut archive = ZipWriter::new(file);
        let options = FileOptions::default().compression_method(CompressionMethod::Deflated);

        archive
            .start_file(TRANSCRIPT_PATH, options)
            .expect("start transcript");
        archive.write_all(b"# transcript").expect("write transcript");

        archive
            .start_file("files/code.ts", options)
            .expect("start file entry");
        archive.write_all(b"console.log('ok');").expect("write file entry");

        let manifest = ConversationBundleManifestPayload {
            bundle_version: "1".into(),
            title: "Hash mismatch".into(),
            source_label: None,
            source_url: None,
            created_at: Utc::now(),
            transcript_path: TRANSCRIPT_PATH.into(),
            transcript_sha256: sha256_hex(b"# transcript"),
            entries: vec![ConversationBundleEntryPayload {
                path: "files/code.ts".into(),
                role: ConversationBundleEntryRole::Reference,
                name: "code.ts".into(),
                mime_type: Some("text/plain".into()),
                size_bytes: 18,
                sha256: sha256_hex(b"something else"),
            }],
        };

        archive
            .start_file(MANIFEST_PATH, options)
            .expect("start manifest");
        archive
            .write_all(
                serde_json::to_string_pretty(&manifest)
                    .expect("serialize manifest")
                    .as_bytes(),
            )
            .expect("write manifest");
        archive.finish().expect("finish zip");

        let error = inspect_bundle_archive(&path).expect_err("bundle should be rejected");
        assert!(
            error.to_string().contains("hash verification"),
            "unexpected error: {error}"
        );

        let _ = fs::remove_file(path);
    }

    fn test_temp_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dropply-test-{}-{file_name}", Uuid::new_v4()))
    }
}
