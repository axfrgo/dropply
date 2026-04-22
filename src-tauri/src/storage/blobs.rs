use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::AppResult;

pub async fn persist_blob(blobs_dir: &Path, source: &Path) -> AppResult<(String, PathBuf, u64)> {
    let bytes = tokio::fs::read(source).await?;
    let digest = Sha256::digest(&bytes);
    let hash = format!("{digest:x}");
    let extension = source.extension().and_then(|value| value.to_str()).unwrap_or("");
    let file_name = if extension.is_empty() {
        hash.clone()
    } else {
        format!("{hash}.{extension}")
    };

    let relative = PathBuf::from("blobs").join(file_name);
    let target = blobs_dir.join(relative.file_name().expect("blob file"));

    if !tokio::fs::try_exists(&target).await? {
        tokio::fs::write(&target, bytes).await?;
    }

    Ok((hash, relative, tokio::fs::metadata(&target).await?.len()))
}

