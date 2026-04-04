use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonMetadata {
    pub pid: u32,
    pub socket_path: PathBuf,
}

pub fn metadata_path(socket_path: &Path) -> PathBuf {
    let file_name = socket_path
        .file_name()
        .map(|name| format!("{}.meta.json", name.to_string_lossy()))
        .unwrap_or_else(|| "caterm.meta.json".to_string());

    socket_path.with_file_name(file_name)
}

pub async fn write_metadata(socket_path: &Path) -> Result<()> {
    let metadata = DaemonMetadata {
        pid: std::process::id(),
        socket_path: socket_path.to_path_buf(),
    };
    let metadata_path = metadata_path(socket_path);
    let payload = serde_json::to_vec_pretty(&metadata)?;
    tokio::fs::write(&metadata_path, payload)
        .await
        .with_context(|| format!("failed to write {}", metadata_path.display()))
}

pub async fn read_metadata(socket_path: &Path) -> Result<Option<DaemonMetadata>> {
    let metadata_path = metadata_path(socket_path);
    let bytes = match tokio::fs::read(&metadata_path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read {}", metadata_path.display()));
        }
    };

    let metadata = serde_json::from_slice::<DaemonMetadata>(&bytes)
        .with_context(|| format!("failed to decode {}", metadata_path.display()))?;
    Ok(Some(metadata))
}

pub async fn remove_metadata(socket_path: &Path) -> Result<()> {
    let metadata_path = metadata_path(socket_path);
    match tokio::fs::remove_file(&metadata_path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to remove {}", metadata_path.display()))
        }
    }
}

pub fn pid_is_alive(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}
