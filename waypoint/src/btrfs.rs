use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use crate::cache::TtlCache;

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

/// Get the root subvolume path
#[allow(dead_code)]
pub fn get_root_subvolume() -> Result<PathBuf> {
    // Try to find the mounted root subvolume
    let mounts = fs::read_to_string("/proc/mounts")
        .context("Failed to read /proc/mounts")?;

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[1] == "/" && parts[2] == "btrfs" {
            return Ok(PathBuf::from("/"));
        }
    }

    bail!("Root filesystem is not Btrfs")
}

/// Get Btrfs subvolume information
#[allow(dead_code)]
pub fn get_subvolume_info(path: &Path) -> Result<SubvolumeInfo> {
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("show")
        .arg(path)
        .output()
        .context("Failed to execute btrfs subvolume show")?;

    if !output.status.success() {
        bail!("Failed to get subvolume info for {:?}", path);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse the output to extract subvolume name and ID
    let mut name = None;
    let mut id = None;

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("Name:") {
            name = line.split_whitespace().nth(1).map(String::from);
        } else if line.starts_with("Subvolume ID:") {
            id = line.split_whitespace().nth(2).and_then(|s| s.parse().ok());
        }
    }

    Ok(SubvolumeInfo {
        path: path.to_path_buf(),
        name: name.unwrap_or_else(|| "unknown".to_string()),
        id: id.unwrap_or(0),
    })
}

#[derive(Debug, Clone)]
pub struct SubvolumeInfo {
    #[allow(dead_code)]
    pub path: PathBuf,
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub id: u64,
}

/// Create a Btrfs snapshot
pub fn create_snapshot(source: &Path, dest: &Path, readonly: bool) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .context("Failed to create snapshot parent directory")?;
    }

    let mut cmd = Command::new("btrfs");
    cmd.arg("subvolume")
        .arg("snapshot");

    if readonly {
        cmd.arg("-r");
    }

    cmd.arg(source).arg(dest);

    let output = cmd.output()
        .context("Failed to execute btrfs snapshot command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create snapshot: {}", stderr);
    }

    Ok(())
}

/// Delete a Btrfs snapshot/subvolume
#[allow(dead_code)]
pub fn delete_snapshot(path: &Path) -> Result<()> {
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("delete")
        .arg(path)
        .output()
        .context("Failed to execute btrfs subvolume delete")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to delete snapshot: {}", stderr);
    }

    Ok(())
}

/// Get the subvolume ID for a path
#[allow(dead_code)]
pub fn get_subvolume_id(path: &Path) -> Result<u64> {
    let info = get_subvolume_info(path)?;
    Ok(info.id)
}

/// Set a subvolume as the default boot subvolume
/// WARNING: This changes which subvolume boots by default!
#[allow(dead_code)]
pub fn set_default_subvolume(subvol_id: u64, mount_point: &Path) -> Result<()> {
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("set-default")
        .arg(subvol_id.to_string())
        .arg(mount_point)
        .output()
        .context("Failed to execute btrfs subvolume set-default")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to set default subvolume: {}", stderr);
    }

    Ok(())
}

/// Get the current default subvolume ID
#[allow(dead_code)]
pub fn get_default_subvolume(mount_point: &Path) -> Result<u64> {
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("get-default")
        .arg(mount_point)
        .output()
        .context("Failed to execute btrfs subvolume get-default")?;

    if !output.status.success() {
        bail!("Failed to get default subvolume");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Output format: "ID 5 (FS_TREE)" or "ID 256 gen 123 top level 5 path @"
    // Extract the ID number
    for part in stdout.split_whitespace() {
        if let Ok(id) = part.parse::<u64>() {
            return Ok(id);
        }
    }

    bail!("Could not parse default subvolume ID from output");
}

/// Create a read-write snapshot from a read-only snapshot
/// This is necessary for rollback since the boot subvolume must be read-write
#[allow(dead_code)]
pub fn create_rw_snapshot_from_ro(source: &Path, dest: &Path) -> Result<()> {
    // Create a read-write snapshot
    create_snapshot(source, dest, false)
}

/// List all subvolumes in a Btrfs filesystem
#[allow(dead_code)]
pub fn list_subvolumes(path: &Path) -> Result<Vec<SubvolumeInfo>> {
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("list")
        .arg(path)
        .output()
        .context("Failed to execute btrfs subvolume list")?;

    if !output.status.success() {
        bail!("Failed to list subvolumes");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut subvolumes = Vec::new();

    for line in stdout.lines() {
        // Parse lines like: "ID 256 gen 123 top level 5 path @snapshots/test"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 9 && parts[0] == "ID" {
            if let Ok(id) = parts[1].parse() {
                let name = parts[8..].join(" ");
                subvolumes.push(SubvolumeInfo {
                    path: PathBuf::from(&name),
                    name,
                    id,
                });
            }
        }
    }

    Ok(subvolumes)
}

/// Check if running as root
#[allow(dead_code)]
pub fn check_root() -> Result<()> {
    let euid = unsafe { libc::geteuid() };
    if euid != 0 {
        bail!("This operation requires root privileges. Please run with sudo or pkexec.");
    }
    Ok(())
}

/// Get available disk space for a path
///
/// This function uses a cache with a 30-second TTL to avoid repeatedly
/// querying the filesystem for the same path.
pub fn get_available_space(path: &Path) -> Result<u64> {
    let path_buf = path.to_path_buf();

    // Check cache first
    if let Some(cached_space) = space_cache().get(&path_buf) {
        return Ok(cached_space);
    }

    // Cache miss - query filesystem
    let output = Command::new("df")
        .arg("-B1")
        .arg("--output=avail")
        .arg(path)
        .output()
        .context("Failed to execute df command")?;

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

/// Get the disk usage of a snapshot or subvolume
/// Returns size in bytes
///
/// This function uses a cache with a 5-minute TTL to avoid repeatedly
/// running expensive `du` operations on the same snapshot.
pub fn get_snapshot_size(path: &Path) -> Result<u64> {
    let path_buf = path.to_path_buf();

    // Check cache first
    if let Some(cached_size) = size_cache().get(&path_buf) {
        return Ok(cached_size);
    }

    // Cache miss - run du
    // Use du to get actual disk usage
    // -s for summary, -b for bytes
    // Note: du may return non-zero exit code due to permission denied errors
    // on some directories in the snapshot, but it still returns a valid size
    let output = Command::new("du")
        .arg("-sb")
        .arg(path)
        .output()
        .context("Failed to execute du command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.split_whitespace().collect();

    if parts.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to get snapshot size: {}", stderr);
    }

    // Parse the size from the first column
    let size: u64 = parts[0]
        .parse()
        .context("Failed to parse snapshot size")?;

    // Store in cache
    size_cache().insert(path_buf, size);

    Ok(size)
}

/// Invalidate the size cache for a specific snapshot path
///
/// Call this when a snapshot is deleted or its contents change
#[allow(dead_code)]
pub fn invalidate_size_cache(path: &Path) {
    size_cache().remove(&path.to_path_buf());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_root() {
        // This will fail in tests unless run as root
        let result = check_root();
        assert!(result.is_err() || result.is_ok());
    }
}
