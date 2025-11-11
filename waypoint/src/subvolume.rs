use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Information about a Btrfs subvolume
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubvolumeInfo {
    /// Mount point (e.g., "/", "/home")
    pub mount_point: PathBuf,
    /// Subvolume path relative to btrfs root (e.g., "@", "@home")
    pub subvol_path: String,
    /// Subvolume ID
    pub id: u64,
    /// User-friendly name for display
    pub display_name: String,
}

impl SubvolumeInfo {
    pub fn new(mount_point: PathBuf, subvol_path: String, id: u64) -> Self {
        let display_name = if mount_point == Path::new("/") {
            "Root filesystem (/)".to_string()
        } else {
            format!("{} ({})",
                mount_point.display(),
                mount_point.display()
            )
        };

        Self {
            mount_point,
            subvol_path,
            id,
            display_name,
        }
    }
}

/// Detect all Btrfs subvolumes mounted on the system
pub fn detect_mounted_subvolumes() -> Result<Vec<SubvolumeInfo>> {
    let mut subvolumes = Vec::new();

    // Read /proc/mounts to find btrfs mounts
    let mounts = std::fs::read_to_string("/proc/mounts")
        .context("Failed to read /proc/mounts")?;

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let mount_point = parts[1];
        let fs_type = parts[2];
        let options = parts[3];

        // Only process btrfs filesystems
        if fs_type != "btrfs" {
            continue;
        }

        // Extract subvolume path and ID from mount options
        let mut subvol_path = None;
        let mut subvol_id = None;

        for opt in options.split(',') {
            if let Some(path) = opt.strip_prefix("subvol=") {
                subvol_path = Some(path.to_string());
            } else if let Some(id_str) = opt.strip_prefix("subvolid=") {
                if let Ok(id) = id_str.parse::<u64>() {
                    subvol_id = Some(id);
                }
            }
        }

        let subvol_path = subvol_path.unwrap_or_else(|| "/".to_string());

        // Use subvolid from mount options, or fall back to btrfs command
        let id = if let Some(id) = subvol_id {
            id
        } else if let Ok(id) = get_subvolume_id(Path::new(mount_point)) {
            id
        } else {
            // Skip this subvolume if we can't get its ID
            log::warn!("Could not determine subvolume ID for {}", mount_point);
            continue;
        };

        let subvol_info = SubvolumeInfo::new(
            PathBuf::from(mount_point),
            subvol_path,
            id,
        );
        subvolumes.push(subvol_info);
    }

    // Sort by mount point for consistent ordering
    subvolumes.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));

    Ok(subvolumes)
}

/// Get the subvolume ID for a given path
fn get_subvolume_id(path: &Path) -> Result<u64> {
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("show")
        .arg(path)
        .output()
        .context("Failed to run btrfs subvolume show")?;

    if !output.status.success() {
        anyhow::bail!("btrfs subvolume show failed");
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Look for line like "Subvolume ID: 256"
    for line in output_str.lines() {
        if line.trim().starts_with("Subvolume ID:") {
            if let Some(id_str) = line.split(':').nth(1) {
                let id: u64 = id_str.trim().parse()
                    .context("Failed to parse subvolume ID")?;
                return Ok(id);
            }
        }
    }

    anyhow::bail!("Could not find subvolume ID in btrfs output")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subvolume_display_name() {
        let root = SubvolumeInfo::new(PathBuf::from("/"), "@".to_string(), 256);
        assert_eq!(root.display_name, "Root filesystem (/)");

        let home = SubvolumeInfo::new(PathBuf::from("/home"), "@home".to_string(), 257);
        assert!(home.display_name.contains("/home"));
    }
}
