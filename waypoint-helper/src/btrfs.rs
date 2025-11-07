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

    // Get subvolume ID of the root snapshot
    let subvol_id = get_subvolume_id(&root_snapshot_path)?;

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

    // TODO: For multi-subvolume snapshots, we would also need to:
    // 1. Check if /home, /var, etc. were included in the snapshot
    // 2. Update /etc/fstab in the restored root to mount the correct subvolume snapshots
    // 3. Or provide a warning to the user about what was/wasn't restored

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
