use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::error::AppResult;

pub async fn persist_blob(blobs_dir: &Path, source: &Path) -> AppResult<(String, PathBuf, u64)> {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_string);
    let temp_name = format!(".incoming-{}", Uuid::new_v4());
    let temp_path = blobs_dir.join(temp_name);

    let source_file = tokio::fs::File::open(source).await?;
    let temp_file = tokio::fs::File::create(&temp_path).await?;
    let (hash, size) = hash_and_copy_file(source_file, temp_file).await?;
    finalize_blob_path(blobs_dir, &temp_path, &hash, extension.as_deref(), size).await
}

pub async fn persist_owned_blob(
    blobs_dir: &Path,
    staged_path: &Path,
    preferred_extension: Option<&str>,
) -> AppResult<(String, PathBuf, u64)> {
    let extension = preferred_extension
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| staged_path.extension().and_then(|value| value.to_str()).map(str::to_string));

    let (hash, size) = hash_existing_file(staged_path).await?;
    finalize_blob_path(blobs_dir, staged_path, &hash, extension.as_deref(), size).await
}

async fn hash_existing_file(path: &Path) -> AppResult<(String, u64)> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut size = 0_u64;

    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }

    Ok((format!("{:x}", hasher.finalize()), size))
}

async fn hash_and_copy_file(
    mut source_file: tokio::fs::File,
    mut target_file: tokio::fs::File,
) -> AppResult<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut size = 0_u64;

    loop {
        let read = source_file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let slice = &buffer[..read];
        hasher.update(slice);
        target_file.write_all(slice).await?;
        size += read as u64;
    }

    target_file.flush().await?;
    Ok((format!("{:x}", hasher.finalize()), size))
}

async fn finalize_blob_path(
    blobs_dir: &Path,
    source_path: &Path,
    hash: &str,
    extension: Option<&str>,
    size: u64,
) -> AppResult<(String, PathBuf, u64)> {
    let file_name = match extension {
        Some(ext) if !ext.is_empty() => format!("{hash}.{ext}"),
        _ => hash.to_string(),
    };
    let relative = PathBuf::from("blobs").join(file_name);
    let target = blobs_dir.join(relative.file_name().expect("blob file"));

    if tokio::fs::try_exists(&target).await? {
        if source_path != target {
            let _ = tokio::fs::remove_file(source_path).await;
        }
    } else if source_path != target {
        tokio::fs::rename(source_path, &target).await?;
    }

    Ok((hash.to_string(), relative, size))
}
