use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{
    ConversationBundleSourcePayload, ImportConversationBundlePayload, ImportPathPayload,
};
use crate::storage::bundles::{
    MAX_BUNDLE_ENTRY_BYTES, MAX_BUNDLE_ENTRY_COUNT, MAX_BUNDLE_TOTAL_BYTES,
};
use crate::storage::sanitize_file_name;

const STAGING_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone)]
pub struct ImportBroker {
    staging_root: PathBuf,
}

pub struct StagedPathImports {
    pub workspace: StagingWorkspace,
    pub paths: Vec<PathBuf>,
}

pub struct StagedConversationBundle {
    pub workspace: StagingWorkspace,
    pub payload: ImportConversationBundlePayload,
}

pub struct StagedBundlePreview {
    pub workspace: StagingWorkspace,
    pub bundle_path: PathBuf,
}

pub struct StagingWorkspace {
    root: PathBuf,
    staging_root: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub enum ShareBundleOrigin {
    DesktopApp,
    Cli,
    BrowserShare,
    IdeShare,
}

enum MissingSourcePolicy {
    Skip,
    Error,
}

impl ImportBroker {
    pub fn new(staging_root: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&staging_root).with_context(|| {
            format!(
                "Unable to create Dropply staging directory at {}",
                staging_root.display()
            )
        })?;
        let _ = cleanup_stale_workspaces(&staging_root);

        Ok(Self { staging_root })
    }

    pub async fn stage_path_imports(&self, payload: ImportPathPayload) -> AppResult<StagedPathImports> {
        let workspace = StagingWorkspace::create(&self.staging_root).await?;
        let mut staged_paths = Vec::with_capacity(payload.paths.len());

        for raw_path in payload.paths {
            let source = PathBuf::from(raw_path);
            if let Some(staged_path) = workspace
                .stage_local_file(&source, "imports", MissingSourcePolicy::Skip, None)
                .await?
            {
                staged_paths.push(staged_path);
            }
        }

        Ok(StagedPathImports {
            workspace,
            paths: staged_paths,
        })
    }

    pub async fn stage_conversation_bundle(
        &self,
        payload: ImportConversationBundlePayload,
    ) -> AppResult<StagedConversationBundle> {
        self.stage_share_bundle(ShareBundleOrigin::DesktopApp, payload)
            .await
    }

    pub async fn stage_share_bundle(
        &self,
        _origin: ShareBundleOrigin,
        payload: ImportConversationBundlePayload,
    ) -> AppResult<StagedConversationBundle> {
        let total_sources = payload.files.len() + payload.attachments.len();
        if total_sources > MAX_BUNDLE_ENTRY_COUNT {
            return Err(AppError::Message(format!(
                "Conversation bundles support at most {MAX_BUNDLE_ENTRY_COUNT} attached files."
            )));
        }
        let transcript_bytes = payload.transcript_markdown.as_bytes().len() as u64;
        if transcript_bytes > MAX_BUNDLE_ENTRY_BYTES {
            return Err(AppError::Message(format!(
                "Conversation bundle transcript exceeds the {} MB per-file limit.",
                MAX_BUNDLE_ENTRY_BYTES / (1024 * 1024)
            )));
        }

        let workspace = StagingWorkspace::create(&self.staging_root).await?;
        let mut total_bytes = transcript_bytes;
        let files = workspace
            .stage_bundle_sources("files", &payload.files)
            .await?;
        total_bytes = total_bytes.saturating_add(sum_paths_size(&files)?);
        let attachments = workspace
            .stage_bundle_sources("attachments", &payload.attachments)
            .await?;
        total_bytes = total_bytes.saturating_add(sum_paths_size(&attachments)?);
        if total_bytes > MAX_BUNDLE_TOTAL_BYTES {
            return Err(AppError::Message(format!(
                "Conversation bundle exceeds the {} MB total size limit.",
                MAX_BUNDLE_TOTAL_BYTES / (1024 * 1024)
            )));
        }

        Ok(StagedConversationBundle {
            workspace,
            payload: ImportConversationBundlePayload {
                title: payload.title,
                transcript_markdown: payload.transcript_markdown,
                source_label: payload.source_label,
                source_url: payload.source_url,
                files,
                attachments,
            },
        })
    }

    pub async fn stage_bundle_preview(&self, bundle_path: &Path) -> AppResult<StagedBundlePreview> {
        let workspace = StagingWorkspace::create(&self.staging_root).await?;
        let staged_path = workspace
            .stage_local_file(
                bundle_path,
                "preview",
                MissingSourcePolicy::Error,
                Some(MAX_BUNDLE_TOTAL_BYTES),
            )
            .await?
            .expect("bundle preview staging should not skip required sources");

        Ok(StagedBundlePreview {
            workspace,
            bundle_path: staged_path,
        })
    }
}

impl StagingWorkspace {
    async fn create(staging_root: &Path) -> AppResult<Self> {
        let root = staging_root.join(format!("job-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&root)
            .await
            .with_context(|| format!("Unable to create staging workspace at {}", root.display()))?;

        Ok(Self {
            root,
            staging_root: staging_root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    async fn stage_bundle_sources(
        &self,
        section: &str,
        sources: &[ConversationBundleSourcePayload],
    ) -> AppResult<Vec<ConversationBundleSourcePayload>> {
        let mut staged_sources = Vec::with_capacity(sources.len());

        for source in sources {
            let staged_path = self.stage_bundle_source(source, section).await?;
            staged_sources.push(ConversationBundleSourcePayload {
                path: Some(staged_path.to_string_lossy().to_string()),
                archive_path: source.archive_path.clone(),
                name: source.name.clone(),
                mime_type: source.mime_type.clone(),
                text_content: None,
                bytes_b64: None,
            });
        }

        Ok(staged_sources)
    }

    async fn stage_bundle_source(
        &self,
        source: &ConversationBundleSourcePayload,
        section: &str,
    ) -> AppResult<PathBuf> {
        if let Some(path) = source.path.as_deref().filter(|value| !value.trim().is_empty()) {
            return self
                .stage_local_file(
                    Path::new(path),
                    section,
                    MissingSourcePolicy::Error,
                    Some(MAX_BUNDLE_ENTRY_BYTES),
                )
                .await?
                .ok_or_else(|| {
                    AppError::Message("Bundle staging unexpectedly skipped a source file.".into())
                });
        }

        if let Some(text_content) = source.text_content.as_deref() {
            return self
                .stage_inline_bytes(
                    section,
                    source,
                    text_content.as_bytes(),
                    Some(MAX_BUNDLE_ENTRY_BYTES),
                )
                .await;
        }

        if let Some(bytes_b64) = source.bytes_b64.as_deref() {
            let bytes = BASE64.decode(bytes_b64).map_err(|error| {
                AppError::Message(format!("Bundle source '{}' has invalid base64 bytes: {error}", source_label(source)))
            })?;
            return self
                .stage_inline_bytes(section, source, &bytes, Some(MAX_BUNDLE_ENTRY_BYTES))
                .await;
        }

        Err(AppError::Message(format!(
            "Bundle source '{}' is missing a file path or inline content.",
            source_label(source)
        )))
    }

    async fn stage_local_file(
        &self,
        source: &Path,
        section: &str,
        missing_policy: MissingSourcePolicy,
        max_bytes: Option<u64>,
    ) -> AppResult<Option<PathBuf>> {
        let Some(canonical_source) = canonicalize_source(source, missing_policy).await? else {
            return Ok(None);
        };

        if canonical_source.starts_with(&self.staging_root) {
            return Err(AppError::Message(format!(
                "Refusing to import staged source '{}' back into Dropply.",
                canonical_source.display()
            )));
        }

        let metadata = tokio::fs::metadata(&canonical_source)
            .await
            .with_context(|| format!("Unable to read metadata for {}", canonical_source.display()))?;
        if !metadata.is_file() {
            return Err(AppError::Message(format!(
                "Import source '{}' is not a file.",
                canonical_source.display()
            )));
        }
        if metadata.len() > i64::MAX as u64 {
            return Err(AppError::Message(format!(
                "Import source '{}' is too large for Dropply to track safely.",
                canonical_source.display()
            )));
        }
        if let Some(max_bytes) = max_bytes.filter(|limit| metadata.len() > *limit) {
            return Err(AppError::Message(format!(
                "Import source '{}' exceeds the {} MB sandbox limit.",
                canonical_source.display(),
                max_bytes / (1024 * 1024)
            )));
        }

        let section_dir = self.root.join(section);
        tokio::fs::create_dir_all(&section_dir).await?;

        let leaf_name = staged_leaf_name(&canonical_source);
        let staged_path = next_available_staged_path(&section_dir, &leaf_name);
        tokio::fs::copy(&canonical_source, &staged_path)
            .await
            .with_context(|| {
                format!(
                    "Unable to stage '{}' inside the Dropply import sandbox.",
                    canonical_source.display()
                )
            })?;

        Ok(Some(staged_path))
    }

    async fn stage_inline_bytes(
        &self,
        section: &str,
        source: &ConversationBundleSourcePayload,
        bytes: &[u8],
        max_bytes: Option<u64>,
    ) -> AppResult<PathBuf> {
        if bytes.len() as u64 > i64::MAX as u64 {
            return Err(AppError::Message(format!(
                "Inline bundle source '{}' is too large for Dropply to track safely.",
                source_label(source)
            )));
        }
        if let Some(max_bytes) = max_bytes.filter(|limit| bytes.len() as u64 > *limit) {
            return Err(AppError::Message(format!(
                "Inline bundle source '{}' exceeds the {} MB sandbox limit.",
                source_label(source),
                max_bytes / (1024 * 1024)
            )));
        }

        let section_dir = self.root.join(section);
        tokio::fs::create_dir_all(&section_dir).await?;

        let leaf_name = staged_bundle_source_name(source);
        let staged_path = next_available_staged_path(&section_dir, &leaf_name);
        tokio::fs::write(&staged_path, bytes).await.with_context(|| {
            format!(
                "Unable to stage inline bundle source '{}' inside the Dropply import sandbox.",
                source_label(source)
            )
        })?;
        Ok(staged_path)
    }
}

impl Drop for StagingWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn canonicalize_source(
    source: &Path,
    missing_policy: MissingSourcePolicy,
) -> AppResult<Option<PathBuf>> {
    match tokio::fs::canonicalize(source).await {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match missing_policy {
            MissingSourcePolicy::Skip => Ok(None),
            MissingSourcePolicy::Error => Err(AppError::Message(format!(
                "Import source '{}' does not exist.",
                source.display()
            ))),
        },
        Err(error) => Err(AppError::Message(format!(
            "Unable to access import source '{}': {}",
            source.display(),
            error
        ))),
    }
}

fn cleanup_stale_workspaces(staging_root: &Path) -> AppResult<()> {
    let now = SystemTime::now();
    let entries = std::fs::read_dir(staging_root)?;

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }

        let modified_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let age = now
            .duration_since(modified_at)
            .unwrap_or_else(|_| Duration::from_secs(0));
        if age >= STAGING_TTL {
            let _ = std::fs::remove_dir_all(path);
        }
    }

    Ok(())
}

fn staged_leaf_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_file_name)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "item".to_string())
}

fn next_available_staged_path(root: &Path, file_name: &str) -> PathBuf {
    let initial = root.join(file_name);
    if !initial.exists() {
        return initial;
    }

    let parsed = Path::new(file_name);
    let stem = parsed
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("item");
    let ext = parsed.extension().and_then(|value| value.to_str()).unwrap_or("");

    for index in 2.. {
        let candidate_name = if ext.is_empty() {
            format!("{stem}-{index}")
        } else {
            format!("{stem}-{index}.{ext}")
        };
        let candidate = root.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("staged path selection should always find an available name")
}

fn sum_paths_size(sources: &[ConversationBundleSourcePayload]) -> AppResult<u64> {
    let mut total = 0_u64;
    for source in sources {
        let path = source.path.as_deref().ok_or_else(|| {
            AppError::Message("Sandbox staged source is missing a filesystem path.".into())
        })?;
        let size = std::fs::metadata(path)
            .with_context(|| format!("Unable to read metadata for {}", path))?
            .len();
        total = total.saturating_add(size);
    }
    Ok(total)
}

fn staged_bundle_source_name(source: &ConversationBundleSourcePayload) -> String {
    source
        .name
        .as_deref()
        .map(sanitize_file_name)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            source
                .archive_path
                .as_deref()
                .and_then(|value| value.replace('\\', "/").rsplit('/').next().map(str::to_string))
                .map(|value| sanitize_file_name(&value))
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "attachment.txt".to_string())
}

fn source_label(source: &ConversationBundleSourcePayload) -> String {
    source
        .name
        .clone()
        .or_else(|| source.archive_path.clone())
        .or_else(|| source.path.clone())
        .unwrap_or_else(|| "bundle source".to_string())
}
