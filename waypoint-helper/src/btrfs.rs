// Btrfs operations for waypoint-helper

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use waypoint_common::{Package, SnapshotInfo};

const SNAPSHOT_DIR: &str = "/@snapshots";
const METADATA_FILE: &str = "/var/lib/waypoint/snapshots.json";

/// Internal snapshot representation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub path: PathBuf,
    pub description: Option<String>,
    pub kernel_version: Option<String>,
    pub package_count: Option<usize>,
    #[serde(default)]
    pub packages: Vec<Package>,
    /// List of subvolumes included in this snapshot (mount points)
    #[serde(default)]
    pub subvolumes: Vec<PathBuf>,
}

impl From<Snapshot> for SnapshotInfo {
    fn from(s: Snapshot) -> Self {
        SnapshotInfo {
            name: s.name,
            timestamp: s.timestamp,
            description: s.description,
            package_count: s.package_count,
            packages: s.packages,
            subvolumes: s.subvolumes,
        }
    }
}

/// Create a new snapshot of multiple subvolumes
pub fn create_snapshot(
    name: &str,
    description: Option<&str>,
    packages: Vec<Package>,
    subvolumes: Vec<PathBuf>,
) -> Result<()> {
    // Default to root if no subvolumes specified
    let subvolumes_to_snapshot = if subvolumes.is_empty() {
        vec![PathBuf::from("/")]
    } else {
        subvolumes
    };

    // Ensure snapshot directory exists
    let snapshot_dir = Path::new(SNAPSHOT_DIR);
    fs::create_dir_all(snapshot_dir)
        .context("Failed to create snapshot directory")?;

    // Create a directory for this snapshot group
    let snapshot_base_path = snapshot_dir.join(name);
    fs::create_dir_all(&snapshot_base_path)
        .context("Failed to create snapshot base directory")?;

    // Create snapshots for each subvolume
    for subvol_mount in &subvolumes_to_snapshot {
        let subvol_name = if subvol_mount == &PathBuf::from("/") {
            "root".to_string()
        } else {
            // Convert /home to "home", /var to "var", etc.
            subvol_mount
                .to_string_lossy()
                .trim_start_matches('/')
                .replace('/', "_")
        };

        let snapshot_path = snapshot_base_path.join(&subvol_name);

        // Create the btrfs snapshot
        let output = Command::new("btrfs")
            .arg("subvolume")
            .arg("snapshot")
            .arg("-r") // Read-only
            .arg(subvol_mount)
            .arg(&snapshot_path)
            .output()
            .context(format!(
                "Failed to create snapshot of {}",
                subvol_mount.display()
            ))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up partial snapshots
            let _ = cleanup_failed_snapshot(&snapshot_base_path);
            bail!(
                "Failed to create snapshot of {}: {}",
                subvol_mount.display(),
                stderr
            );
        }
    }

    // Save metadata
    let snapshot = Snapshot {
        id: format!("snapshot-{}", Utc::now().format("%Y%m%d-%H%M%S")),
        name: name.to_string(),
        timestamp: Utc::now(),
        path: snapshot_base_path,
        description: description.map(String::from),
        kernel_version: get_kernel_version(),
        package_count: Some(packages.len()),
        packages,
        subvolumes: subvolumes_to_snapshot,
    };

    add_snapshot_metadata(snapshot)?;

    Ok(())
}

/// Clean up a failed snapshot creation
fn cleanup_failed_snapshot(snapshot_path: &Path) -> Result<()> {
    if snapshot_path.exists() {
        // Delete all subvolume snapshots in the directory
        if let Ok(entries) = fs::read_dir(snapshot_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let _ = Command::new("btrfs")
                        .arg("subvolume")
                        .arg("delete")
                        .arg(&path)
                        .output();
                }
            }
        }
        // Remove the directory
        let _ = fs::remove_dir(snapshot_path);
    }
    Ok(())
}

/// Delete a snapshot (and all its subvolumes)
pub fn delete_snapshot(name: &str) -> Result<()> {
    let snapshot_path = Path::new(SNAPSHOT_DIR).join(name);

    if !snapshot_path.exists() {
        bail!("Snapshot not found: {}", name);
    }

    // Check if it's a directory (new multi-subvolume format) or a single subvolume (old format)
    if snapshot_path.is_dir() {
        // New format: directory containing subvolume snapshots
        // Delete all subvolume snapshots within this directory
        let entries = fs::read_dir(&snapshot_path)
            .context("Failed to read snapshot directory")?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let subvol_path = entry.path();

            if subvol_path.is_dir() {
                let output = Command::new("btrfs")
                    .arg("subvolume")
                    .arg("delete")
                    .arg(&subvol_path)
                    .output()
                    .context("Failed to execute btrfs subvolume delete")?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("Warning: Failed to delete {}: {}", subvol_path.display(), stderr);
                }
            }
        }

        // Remove the parent directory
        fs::remove_dir(&snapshot_path)
            .context("Failed to remove snapshot directory")?;
    } else {
        // Old format: single subvolume snapshot
        let output = Command::new("btrfs")
            .arg("subvolume")
            .arg("delete")
            .arg(&snapshot_path)
            .output()
            .context("Failed to execute btrfs subvolume delete")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to delete snapshot: {}", stderr);
        }
    }

    // Remove from metadata
    remove_snapshot_metadata(name)?;

    Ok(())
}

/// Restore a snapshot (set as default boot subvolume)
pub fn restore_snapshot(name: &str) -> Result<()> {
    let snapshot_base_path = Path::new(SNAPSHOT_DIR).join(name);

    if !snapshot_base_path.exists() {
        bail!("Snapshot not found: {}", name);
    }

    // Load snapshot metadata to check which subvolumes were included
    let snapshot_meta = get_snapshot_metadata(name)?;

    // Determine the path to the root snapshot
    let root_snapshot_path = if snapshot_base_path.is_dir() {
        // New format: directory with subvolumes
        snapshot_base_path.join("root")
    } else {
        // Old format: single subvolume
        snapshot_base_path.clone()
    };

    if !root_snapshot_path.exists() {
        bail!("Root snapshot not found in snapshot {}", name);
    }

    // Check if this is a multi-subvolume snapshot
    let has_multiple_subvolumes = snapshot_meta.subvolumes.len() > 1;

    let target_root = if has_multiple_subvolumes {
        // For multi-subvolume snapshots, we need to update fstab
        // Create a writable copy of the root snapshot
        let writable_root = snapshot_base_path.join("root-writable");

        // Clean up any existing writable snapshot from previous attempts
        if writable_root.exists() {
            let _ = Command::new("btrfs")
                .arg("subvolume")
                .arg("delete")
                .arg(&writable_root)
                .output();
        }

        create_writable_snapshot(&root_snapshot_path, &writable_root)
            .context("Failed to create writable snapshot for fstab update")?;

        // Update fstab in the writable snapshot
        let fstab_path = writable_root.join("etc/fstab");
        if fstab_path.exists() {
            update_fstab_for_snapshot(&fstab_path, name, &snapshot_meta.subvolumes)
                .context("Failed to update fstab")?;
        } else {
            eprintln!("Warning: /etc/fstab not found in snapshot, multi-subvolume restore may not work correctly");
        }

        writable_root
    } else {
        // Single subvolume snapshot, use it directly
        root_snapshot_path
    };

    // Get subvolume ID of the target root
    let subvol_id = get_subvolume_id(&target_root)?;

    // Set as default boot subvolume
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("set-default")
        .arg(subvol_id.to_string())
        .arg("/")
        .output()
        .context("Failed to execute btrfs subvolume set-default")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to set default subvolume: {}", stderr);
    }

    Ok(())
}

/// List all snapshots
pub fn list_snapshots() -> Result<Vec<Snapshot>> {
    load_snapshot_metadata()
}

/// Get subvolume ID for a path
fn get_subvolume_id(path: &Path) -> Result<u64> {
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

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("Subvolume ID:") {
            if let Some(id_str) = line.split_whitespace().nth(2) {
                if let Ok(id) = id_str.parse() {
                    return Ok(id);
                }
            }
        }
    }

    bail!("Could not parse subvolume ID from output");
}

/// Get current kernel version
fn get_kernel_version() -> Option<String> {
    fs::read_to_string("/proc/version")
        .ok()
        .and_then(|v| v.split_whitespace().nth(2).map(String::from))
}

/// Load snapshot metadata from file
fn load_snapshot_metadata() -> Result<Vec<Snapshot>> {
    let path = Path::new(METADATA_FILE);

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)
        .context("Failed to read snapshots metadata")?;

    let snapshots: Vec<Snapshot> = serde_json::from_str(&content)
        .context("Failed to parse snapshots metadata")?;

    Ok(snapshots)
}

/// Save snapshot metadata to file
fn save_snapshot_metadata(snapshots: &[Snapshot]) -> Result<()> {
    let path = Path::new(METADATA_FILE);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .context("Failed to create metadata directory")?;
    }

    let content = serde_json::to_string_pretty(snapshots)
        .context("Failed to serialize snapshots")?;

    fs::write(path, content)
        .context("Failed to write snapshots metadata")?;

    Ok(())
}

/// Add snapshot to metadata
fn add_snapshot_metadata(snapshot: Snapshot) -> Result<()> {
    let mut snapshots = load_snapshot_metadata()?;
    snapshots.push(snapshot);
    save_snapshot_metadata(&snapshots)
}

/// Remove snapshot from metadata
fn remove_snapshot_metadata(name: &str) -> Result<()> {
    let mut snapshots = load_snapshot_metadata()?;
    snapshots.retain(|s| s.name != name);
    save_snapshot_metadata(&snapshots)
}

/// Get snapshot metadata by name
fn get_snapshot_metadata(name: &str) -> Result<Snapshot> {
    let snapshots = load_snapshot_metadata()?;
    snapshots
        .into_iter()
        .find(|s| s.name == name)
        .context(format!("Snapshot metadata not found: {}", name))
}

/// Get filesystem UUID for a mount point
#[allow(dead_code)]
fn get_filesystem_uuid(mount_point: &Path) -> Result<String> {
    let output = Command::new("findmnt")
        .arg("-n")
        .arg("-o")
        .arg("UUID")
        .arg(mount_point)
        .output()
        .context("Failed to execute findmnt")?;

    if !output.status.success() {
        bail!("Failed to get UUID for {:?}", mount_point);
    }

    let uuid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if uuid.is_empty() {
        bail!("No UUID found for {:?}", mount_point);
    }

    Ok(uuid)
}

/// Update fstab in a snapshot to mount the correct subvolume snapshots
fn update_fstab_for_snapshot(
    fstab_path: &Path,
    snapshot_name: &str,
    subvolumes: &[PathBuf],
) -> Result<()> {
    // Read current fstab
    let fstab_content = fs::read_to_string(fstab_path)
        .context("Failed to read fstab")?;

    let mut updated_lines = Vec::new();
    let mut updated = false;

    for line in fstab_content.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            updated_lines.push(line.to_string());
            continue;
        }

        // Parse fstab entry: device mountpoint fstype options dump pass
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 4 {
            updated_lines.push(line.to_string());
            continue;
        }

        let mount_point = parts[1];
        let fs_type = parts[2];
        let options = parts[3];

        // Only process btrfs entries for subvolumes we snapshotted
        if fs_type != "btrfs" {
            updated_lines.push(line.to_string());
            continue;
        }

        // Check if this mount point is in our snapshot
        let mount_path = PathBuf::from(mount_point);
        if !subvolumes.contains(&mount_path) {
            updated_lines.push(line.to_string());
            continue;
        }

        // Update the subvol option to point to the snapshot
        let new_options = update_subvol_option(options, snapshot_name, &mount_path)?;

        // Reconstruct the fstab line with updated options
        let mut new_parts = parts.clone();
        new_parts[3] = &new_options;

        // Preserve original spacing/formatting as much as possible
        let new_line = if parts.len() == 6 {
            format!("{}\t{}\t{}\t{}\t{}\t{}",
                new_parts[0], new_parts[1], new_parts[2],
                new_parts[3], new_parts[4], new_parts[5])
        } else {
            new_parts.join("\t")
        };

        updated_lines.push(new_line);
        updated = true;
    }

    if updated {
        // Write updated fstab
        let new_content = updated_lines.join("\n") + "\n";
        fs::write(fstab_path, new_content)
            .context("Failed to write updated fstab")?;
    }

    Ok(())
}

/// Update the subvol option in mount options string
fn update_subvol_option(
    options: &str,
    snapshot_name: &str,
    mount_point: &Path,
) -> Result<String> {
    let opts: Vec<&str> = options.split(',').collect();
    let mut new_opts = Vec::new();
    let mut found_subvol = false;

    // Determine the subvolume name in the snapshot
    let subvol_name = if mount_point == &PathBuf::from("/") {
        "root".to_string()
    } else {
        mount_point
            .to_string_lossy()
            .trim_start_matches('/')
            .replace('/', "_")
    };

    // The new subvol path in the snapshot
    let new_subvol = format!("@snapshots/{}/{}", snapshot_name, subvol_name);

    for opt in opts {
        if opt.starts_with("subvol=") || opt.starts_with("subvolid=") {
            // Replace with new subvol path
            new_opts.push(format!("subvol={}", new_subvol));
            found_subvol = true;
        } else {
            new_opts.push(opt.to_string());
        }
    }

    // If no subvol option was found, add it
    if !found_subvol {
        new_opts.push(format!("subvol={}", new_subvol));
    }

    Ok(new_opts.join(","))
}

/// Create a writable snapshot from a read-only snapshot
fn create_writable_snapshot(source: &Path, dest: &Path) -> Result<()> {
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("snapshot")
        // Note: no -r flag, so it's writable
        .arg(source)
        .arg(dest)
        .output()
        .context("Failed to create writable snapshot")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create writable snapshot: {}", stderr);
    }

    Ok(())
}
