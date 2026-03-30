use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Result of a single update check.
#[derive(Debug, Clone)]
pub enum UpdateResult {
    /// Already running the latest version.
    UpToDate,
    /// A new version was downloaded and installed; restart required.
    Updated { from: String, to: String },
    /// A new version exists but manual intervention is required.
    /// Details were written to `~/.local/share/memlayer/update_pending`.
    ManualPending { version: String },
    /// No daemon component found in the server response.
    NoDaemonComponent,
    /// No artifact available for the current platform.
    NoPlatformArtifact { platform: String },
}

/// Server response from `/api/version/latest`.
#[derive(Debug, Deserialize)]
struct LatestVersionResponse {
    latest_version: String,
    #[serde(default)]
    manual_intervention: bool,
    #[serde(default)]
    components: HashMap<String, ComponentInfo>,
}

#[derive(Debug, Deserialize)]
struct ComponentInfo {
    version: String,
    #[serde(default)]
    artifacts: HashMap<String, String>,
    /// Optional SHA-256 checksums keyed by the same platform key.
    #[serde(default)]
    checksums: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

fn platform_key() -> String {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    format!("{os}-{arch}")
}

/// Return the base directory for memlayer data: `~/.local/share/memlayer`.
fn data_dir() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
        .ok_or_else(|| "cannot determine data directory".to_string())?;
    Ok(base.join("memlayer"))
}

/// Directory where old binary versions are archived.
fn versions_dir() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("versions"))
}

// ---------------------------------------------------------------------------
// One-shot check
// ---------------------------------------------------------------------------

/// Check the server for a new daemon version and, if available, perform the
/// update (or write a pending marker when manual intervention is required).
pub async fn check_once(server_url: &str, auth_token: &str) -> Result<UpdateResult, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    info!(current = %current_version, "checking for daemon updates");

    // -- Fetch latest version info ----------------------------------------
    let url = format!("{}/version/latest", server_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut req = client.get(&url);
    if !auth_token.is_empty() {
        req = req.bearer_auth(auth_token);
    }

    let resp = req.send().await.map_err(|e| format!("version check request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "version check returned HTTP {}",
            resp.status()
        ));
    }

    let body: LatestVersionResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse version response: {e}"))?;

    // -- Locate daemon component ------------------------------------------
    let daemon = match body.components.get("daemon") {
        Some(c) => c,
        None => {
            debug!("no daemon component in version response");
            return Ok(UpdateResult::NoDaemonComponent);
        }
    };

    if daemon.version == current_version {
        info!(version = %current_version, "daemon is up to date");
        return Ok(UpdateResult::UpToDate);
    }

    info!(
        current = %current_version,
        available = %daemon.version,
        manual = body.manual_intervention,
        "new daemon version available"
    );

    // -- Manual intervention path -----------------------------------------
    if body.manual_intervention {
        write_pending_marker(&daemon.version, &body.latest_version)?;
        return Ok(UpdateResult::ManualPending {
            version: daemon.version.clone(),
        });
    }

    // -- Locate platform artifact -----------------------------------------
    let platform = platform_key();
    let artifact_url = match daemon.artifacts.get(&platform) {
        Some(u) => u.clone(),
        None => {
            warn!(platform = %platform, "no artifact for this platform");
            return Ok(UpdateResult::NoPlatformArtifact { platform });
        }
    };
    let expected_checksum = daemon.checksums.get(&platform).cloned();

    // -- Download artifact ------------------------------------------------
    let artifact_bytes = download_artifact(&client, &artifact_url, auth_token).await?;

    // -- Verify checksum --------------------------------------------------
    if let Some(ref expected) = expected_checksum {
        verify_sha256(&artifact_bytes, expected)?;
        info!("SHA-256 checksum verified");
    } else {
        debug!("no checksum provided, skipping verification");
    }

    // -- Extract binary from tarball --------------------------------------
    let binary_bytes = extract_binary_from_tarball(&artifact_bytes)?;

    // -- Archive current binary -------------------------------------------
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("cannot determine current executable path: {e}"))?;
    archive_current_binary(&current_exe, &current_version)?;

    // -- Prune old archives (keep last 3) ---------------------------------
    prune_archives(3)?;

    // -- Atomically replace binary ----------------------------------------
    atomic_replace(&current_exe, &binary_bytes)?;

    info!(
        from = %current_version,
        to = %daemon.version,
        "daemon binary updated — please restart the service to use the new version"
    );

    Ok(UpdateResult::Updated {
        from: current_version,
        to: daemon.version.clone(),
    })
}

// ---------------------------------------------------------------------------
// Background loop
// ---------------------------------------------------------------------------

/// Periodically check for updates at `check_interval` and apply them
/// automatically (or write a pending marker). This function never returns
/// under normal operation — it should be spawned as a background task.
pub async fn check_and_update(
    server_url: &str,
    auth_token: &str,
    check_interval: Duration,
) {
    loop {
        match check_once(server_url, auth_token).await {
            Ok(UpdateResult::UpToDate) => {
                debug!("update check: up to date");
            }
            Ok(UpdateResult::Updated { from, to }) => {
                info!(
                    from = %from,
                    to = %to,
                    "update applied — restart the daemon to activate the new version"
                );
            }
            Ok(UpdateResult::ManualPending { version }) => {
                info!(
                    version = %version,
                    "update requires manual intervention; details in update_pending file"
                );
            }
            Ok(UpdateResult::NoDaemonComponent) => {
                debug!("server did not advertise a daemon component");
            }
            Ok(UpdateResult::NoPlatformArtifact { platform }) => {
                warn!(platform = %platform, "no artifact available for this platform");
            }
            Err(e) => {
                warn!(error = %e, "update check failed");
            }
        }

        tokio::time::sleep(check_interval).await;
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Download an artifact from the given URL.
async fn download_artifact(
    client: &reqwest::Client,
    url: &str,
    auth_token: &str,
) -> Result<Vec<u8>, String> {
    info!(url = %url, "downloading update artifact");

    let mut req = client.get(url);
    // Only send auth for same-origin requests; GitHub release URLs are public.
    if !auth_token.is_empty() && !url.contains("github.com") {
        req = req.bearer_auth(auth_token);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("artifact download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "artifact download returned HTTP {}",
            resp.status()
        ));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("failed to read artifact body: {e}"))
}

/// Verify that `data` matches the expected hex-encoded SHA-256 digest.
fn verify_sha256(data: &[u8], expected_hex: &str) -> Result<(), String> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let actual = hex::encode(hasher.finalize());
    if actual != expected_hex.to_lowercase() {
        return Err(format!(
            "checksum mismatch: expected {expected_hex}, got {actual}"
        ));
    }
    Ok(())
}

/// Extract the daemon binary from a `.tar.gz` archive. Expects exactly one
/// regular file whose name contains "memlayer-daemon" (ignoring directory
/// entries). Falls back to the first regular file if no name matches.
fn extract_binary_from_tarball(tarball: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let gz = flate2::read::GzDecoder::new(tarball);
    let mut archive = tar::Archive::new(gz);

    let mut best: Option<Vec<u8>> = None;
    let mut first_file: Option<Vec<u8>> = None;

    let entries = archive
        .entries()
        .map_err(|e| format!("failed to read tarball entries: {e}"))?;

    for entry_result in entries {
        let mut entry = entry_result.map_err(|e| format!("bad tarball entry: {e}"))?;
        if entry.header().entry_type().is_file() {
            let path_str = entry
                .path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("failed to read entry {path_str}: {e}"))?;

            if path_str.contains("memlayer-daemon") {
                best = Some(buf);
                break;
            }
            if first_file.is_none() {
                first_file = Some(buf);
            }
        }
    }

    best.or(first_file)
        .ok_or_else(|| "tarball contained no regular files".to_string())
}

/// Archive the current binary to `~/.local/share/memlayer/versions/`.
fn archive_current_binary(current_exe: &Path, version: &str) -> Result<(), String> {
    let dir = versions_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create versions dir: {e}"))?;

    let dest = dir.join(format!("memlayer-daemon-{version}"));
    if dest.exists() {
        debug!(path = %dest.display(), "archive already exists, skipping");
        return Ok(());
    }

    std::fs::copy(current_exe, &dest)
        .map_err(|e| format!("failed to archive current binary: {e}"))?;

    info!(path = %dest.display(), "archived current binary");
    Ok(())
}

/// Keep only the newest `keep` archives in the versions directory.
fn prune_archives(keep: usize) -> Result<(), String> {
    let dir = versions_dir()?;
    if !dir.exists() {
        return Ok(());
    }

    let mut entries: Vec<(PathBuf, std::time::SystemTime)> = std::fs::read_dir(&dir)
        .map_err(|e| format!("cannot read versions dir: {e}"))?
        .filter_map(|r| r.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("memlayer-daemon-")
        })
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((e.path(), modified))
        })
        .collect();

    // Sort newest first.
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    for (path, _) in entries.iter().skip(keep) {
        if let Err(e) = std::fs::remove_file(path) {
            warn!(path = %path.display(), error = %e, "failed to prune old archive");
        } else {
            info!(path = %path.display(), "pruned old archive");
        }
    }

    Ok(())
}

/// Atomically replace the binary at `target` with `new_bytes`.
///
/// Strategy: write to a temp file next to the target, set executable
/// permissions, then rename (atomic on the same filesystem on Unix).
fn atomic_replace(target: &Path, new_bytes: &[u8]) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| "target has no parent directory".to_string())?;

    let tmp_path = parent.join(".memlayer-daemon.update.tmp");

    // Write new binary to temp file.
    std::fs::write(&tmp_path, new_bytes)
        .map_err(|e| format!("failed to write temp file: {e}"))?;

    // Set executable permissions (rwxr-xr-x).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to set permissions on temp file: {e}"))?;
    }

    // Atomic rename.
    std::fs::rename(&tmp_path, target)
        .map_err(|e| format!("failed to rename temp file to target: {e}"))?;

    Ok(())
}

/// Write a marker file so an external tool (or human) knows an update is
/// pending and requires manual action.
fn write_pending_marker(daemon_version: &str, latest_version: &str) -> Result<(), String> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot create data dir: {e}"))?;

    let path = dir.join("update_pending");
    let content = format!(
        "pending_daemon_version={daemon_version}\n\
         latest_version={latest_version}\n\
         current_version={current}\n\
         platform={platform}\n\
         timestamp={ts}\n",
        current = env!("CARGO_PKG_VERSION"),
        platform = platform_key(),
        ts = chrono::Utc::now().to_rfc3339(),
    );

    std::fs::write(&path, content)
        .map_err(|e| format!("failed to write update_pending marker: {e}"))?;

    info!(path = %path.display(), "wrote update_pending marker");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_key_format() {
        let key = platform_key();
        // Should be "os-arch" with no empty segments.
        let parts: Vec<&str> = key.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
    }

    #[test]
    fn test_verify_sha256_valid() {
        let data = b"hello world";
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_sha256(data, expected).is_ok());
    }

    #[test]
    fn test_verify_sha256_invalid() {
        let data = b"hello world";
        let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(verify_sha256(data, wrong).is_err());
    }

    #[test]
    fn test_verify_sha256_case_insensitive() {
        let data = b"hello world";
        let upper = "B94D27B9934D3E08A52E52D7DA7DABFAC484EFE37A5380EE9088F7ACE2EFCDE9";
        assert!(verify_sha256(data, upper).is_ok());
    }

    #[test]
    fn test_data_dir_is_under_memlayer() {
        let dir = data_dir().unwrap();
        assert!(dir.ends_with("memlayer"));
    }

    #[test]
    fn test_versions_dir_is_under_data() {
        let dir = versions_dir().unwrap();
        assert!(dir.ends_with("memlayer/versions"));
    }

    #[test]
    fn test_prune_archives_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("versions");
        std::fs::create_dir_all(&dir).unwrap();
        // Should not error on empty directory.
        // (We cannot easily test prune_archives directly because it uses
        // versions_dir(), but we can test the logic indirectly via the
        // archive/prune integration in a future integration test.)
    }

    #[test]
    fn test_extract_binary_from_tarball() {
        // Build a small .tar.gz with one file named "memlayer-daemon".
        let binary_content = b"FAKE_ELF_BINARY";
        let buf = Vec::new();
        let enc = flate2::write::GzEncoder::new(buf, flate2::Compression::fast());
        let mut builder = tar::Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_size(binary_content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        builder
            .append_data(&mut header, "memlayer-daemon", &binary_content[..])
            .unwrap();

        let enc = builder.into_inner().unwrap();
        let tarball = enc.finish().unwrap();

        let extracted = extract_binary_from_tarball(&tarball).unwrap();
        assert_eq!(extracted, binary_content);
    }

    #[test]
    fn test_extract_tarball_fallback_to_first_file() {
        let content = b"SOME_OTHER_BINARY";
        let buf = Vec::new();
        let enc = flate2::write::GzEncoder::new(buf, flate2::Compression::fast());
        let mut builder = tar::Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        builder
            .append_data(&mut header, "some-other-name", &content[..])
            .unwrap();

        let enc = builder.into_inner().unwrap();
        let tarball = enc.finish().unwrap();

        let extracted = extract_binary_from_tarball(&tarball).unwrap();
        assert_eq!(extracted, content);
    }

    #[test]
    fn test_extract_tarball_empty_errors() {
        let buf = Vec::new();
        let enc = flate2::write::GzEncoder::new(buf, flate2::Compression::fast());
        let builder = tar::Builder::new(enc);
        let enc = builder.into_inner().unwrap();
        let tarball = enc.finish().unwrap();

        assert!(extract_binary_from_tarball(&tarball).is_err());
    }

    #[test]
    fn test_atomic_replace() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("test-binary");
        std::fs::write(&target, b"old content").unwrap();

        atomic_replace(&target, b"new content").unwrap();

        let contents = std::fs::read(&target).unwrap();
        assert_eq!(contents, b"new content");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }
}
