use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use crate::cache::TtlCache;
use crate::performance;

/// Global cache for snapshot sizes (5-minute TTL)
static SIZE_CACHE: OnceLock<TtlCache<PathBuf, u64>> = OnceLock::new();

/// Global cache for available disk space (30-second TTL)
static SPACE_CACHE: OnceLock<TtlCache<PathBuf, u64>> = OnceLock::new();

/// Initialize caches (call once at startup)
pub fn init_cache() {
    SIZE_CACHE.get_or_init(|| TtlCache::new(Duration::from_secs(300))); // 5 minutes
    SPACE_CACHE.get_or_init(|| TtlCache::new(Duration::from_secs(30))); // 30 seconds
}

/// Get the size cache
fn size_cache() -> &'static TtlCache<PathBuf, u64> {
    SIZE_CACHE.get_or_init(|| TtlCache::new(Duration::from_secs(300)))
}

/// Get the space cache
fn space_cache() -> &'static TtlCache<PathBuf, u64> {
    SPACE_CACHE.get_or_init(|| TtlCache::new(Duration::from_secs(30)))
}

/// Check if a path is on a Btrfs filesystem
pub fn is_btrfs(path: &Path) -> Result<bool> {
    let output = Command::new("stat")
        .arg("-f")
        .arg("-c")
        .arg("%T")
        .arg(path)
        .output()
        .context("Failed to execute stat command")?;

    if !output.status.success() {
        bail!("stat command failed");
    }

    let fs_type = String::from_utf8_lossy(&output.stdout);
    Ok(fs_type.trim() == "btrfs")
}

/// Get available disk space for a path
///
/// This function uses a cache with a 30-second TTL to avoid repeatedly
/// querying the filesystem for the same path.
pub fn get_available_space(path: &Path) -> Result<u64> {
    let _timer = performance::tracker().start("get_available_space");
    let path_buf = path.to_path_buf();

    // Check cache first
    if let Some(cached_space) = space_cache().get(&path_buf) {
        let _cache_timer = performance::tracker().start("get_available_space_cache_hit");
        return Ok(cached_space);
    }

    // Cache miss - query filesystem
    let _df_timer = performance::tracker().start("df_command");
    let output = Command::new("df")
        .arg("-B1")
        .arg("--output=avail")
        .arg(path)
        .output()
        .context("Failed to execute df command")?;
    drop(_df_timer);

    if !output.status.success() {
        bail!("Failed to get available space");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    if lines.len() < 2 {
        bail!("Unexpected df output");
    }

    let space: u64 = lines[1]
        .trim()
        .parse()
        .context("Failed to parse available space")?;

    // Store in cache
    space_cache().insert(path_buf, space);

    Ok(space)
}

/// Get all snapshot sizes efficiently via D-Bus helper
/// Returns a HashMap mapping snapshot paths to sizes in bytes
///
/// This uses the privileged D-Bus helper which:
/// 1. Runs with root permissions (no password prompts)
/// 2. Uses parallel processing for speed
pub fn get_all_snapshot_sizes(paths: &[std::path::PathBuf]) -> std::collections::HashMap<std::path::PathBuf, u64> {
    use std::collections::HashMap;

    let _timer = performance::tracker().start("get_all_snapshot_sizes");

    // Try to use D-Bus helper to get sizes (runs with privileges, no password prompt)
    if let Ok(client) = crate::dbus_client::WaypointHelperClient::new() {
        // Extract snapshot names from paths
        let snapshot_names: Vec<String> = paths
            .iter()
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|s| s.to_string())
            })
            .collect();

        if !snapshot_names.is_empty() {
            if let Ok(sizes_by_name) = client.get_snapshot_sizes(snapshot_names) {
                // Convert back from name->size to path->size
                let mut result = HashMap::new();
                for path in paths {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if let Some(&size) = sizes_by_name.get(name) {
                            result.insert(path.clone(), size);
                        }
                    }
                }
                return result;
            } else {
                log::warn!("D-Bus call to get snapshot sizes failed, falling back to local du");
            }
        }
    } else {
        log::warn!("Could not connect to D-Bus helper, falling back to local du");
    }

    // Fallback: use local du calls (may prompt for password)
    use rayon::prelude::*;
    let _parallel_timer = performance::tracker().start("parallel_du_calls_fallback");
    paths
        .par_iter()
        .filter_map(|path| {
            let size = get_snapshot_size(path).ok()?;
            Some((path.clone(), size))
        })
        .collect()
}

/// Get the disk usage of a snapshot or subvolume
/// Returns size in bytes
///
/// This function uses a cache with a 5-minute TTL to avoid repeatedly
/// running expensive `du` operations on the same snapshot.
pub fn get_snapshot_size(path: &Path) -> Result<u64> {
    let _timer = performance::tracker().start("get_snapshot_size");
    let path_buf = path.to_path_buf();

    // Check cache first
    if let Some(cached_size) = size_cache().get(&path_buf) {
        let _cache_timer = performance::tracker().start("get_snapshot_size_cache_hit");
        return Ok(cached_size);
    }

    // Cache miss - run du
    // Use du to get actual disk usage
    // -s for summary, -b for bytes
    // Note: du may return non-zero exit code due to permission denied errors
    // on some directories in the snapshot, but it still returns a valid size
    let _du_timer = performance::tracker().start("du_command");
    let output = Command::new("du")
        .arg("-sb")
        .arg(path)
        .output()
        .context("Failed to execute du command")?;
    drop(_du_timer);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.split_whitespace().collect();

    if parts.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to get snapshot size: {stderr}");
    }

    // Parse the size from the first column
    let size: u64 = parts[0].parse().context("Failed to parse snapshot size")?;

    // Store in cache
    size_cache().insert(path_buf, size);

    Ok(size)
}
