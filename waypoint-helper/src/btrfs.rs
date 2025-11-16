// Btrfs operations for waypoint-helper

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use version_compare::{Cmp, compare};
use waypoint_common::{Package, SnapshotInfo, WaypointConfig};

/// Global configuration instance
static CONFIG: OnceLock<WaypointConfig> = OnceLock::new();

/// Initialize the global configuration (called once at startup)
pub fn init_config() {
    CONFIG.get_or_init(|| WaypointConfig::new());
}

/// Get the snapshot directory path
fn snapshot_dir() -> &'static Path {
    CONFIG
        .get_or_init(|| WaypointConfig::new())
        .snapshot_dir
        .as_path()
}

/// Get the metadata file path
fn metadata_file() -> &'static Path {
    CONFIG
        .get_or_init(|| WaypointConfig::new())
        .metadata_file
        .as_path()
}

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
    ensure_snapshot_name(name)?;

    // Default to root if no subvolumes specified
    let subvolumes_to_snapshot = if subvolumes.is_empty() {
        vec![PathBuf::from("/")]
    } else {
        subvolumes
    };

    // Load exclude patterns
    let exclude_config = waypoint_common::ExcludeConfig::load().unwrap_or_default();
    let enabled_patterns = exclude_config.enabled_patterns();

    // Ensure snapshot directory exists
    let snap_dir = snapshot_dir();
    fs::create_dir_all(snap_dir).context("Failed to create snapshot directory")?;

    // Create a directory for this snapshot group
    let snapshot_base_path = snap_dir.join(name);
    fs::create_dir_all(&snapshot_base_path).context("Failed to create snapshot base directory")?;

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

        // Use the mount point directly as the source
        let source_path = subvol_mount;

        log::info!(
            "Creating snapshot: {} -> {}",
            source_path.display(),
            snapshot_path.display()
        );

        // Create the btrfs snapshot as WRITABLE (no -r flag) so we can apply exclusions
        let output = Command::new("btrfs")
            .arg("subvolume")
            .arg("snapshot")
            .arg(&source_path)
            .arg(&snapshot_path)
            .output()
            .context(format!(
                "Failed to create snapshot of {}",
                source_path.display()
            ))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Clean up partial snapshots
            let _ = cleanup_failed_snapshot(&snapshot_base_path);
            bail!(
                "Failed to create snapshot of {}: {}\n{}",
                source_path.display(),
                stderr,
                stdout
            );
        }

        // Apply exclude patterns by deleting matching files
        if !enabled_patterns.is_empty() {
            log::info!(
                "Applying {} exclude patterns to {}",
                enabled_patterns.len(),
                snapshot_path.display()
            );
            if let Err(e) = apply_exclusions(&snapshot_path, &enabled_patterns) {
                log::error!(
                    "Failed to apply exclusions to {}: {}",
                    snapshot_path.display(),
                    e
                );
                // Don't fail the whole snapshot, just log the error
            }
        }

        // Now make the snapshot read-only
        let output = Command::new("btrfs")
            .arg("property")
            .arg("set")
            .arg("-ts")
            .arg(&snapshot_path)
            .arg("ro")
            .arg("true")
            .output()
            .context("Failed to make snapshot read-only")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::warn!("Failed to make snapshot read-only: {}", stderr);
            // Continue anyway - writable snapshots still work
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

/// Apply exclude patterns to a snapshot by deleting matching files
fn apply_exclusions(
    snapshot_path: &Path,
    patterns: &[&waypoint_common::ExcludePattern],
) -> Result<()> {
    use walkdir::WalkDir;

    let mut deleted_count = 0;
    let mut failed_count = 0;

    // Walk the snapshot directory and find files matching patterns
    for entry in WalkDir::new(snapshot_path)
        .follow_links(false) // Don't follow symlinks
        .into_iter()
        .filter_entry(|e| {
            // Filter at directory level for efficiency
            // If a directory matches a prefix pattern, we can skip its entire contents
            let path = e.path();
            let relative_path = path.strip_prefix(snapshot_path).ok();

            if let Some(rel_path) = relative_path {
                // Convert to absolute path within the original filesystem for pattern matching
                let absolute_path = Path::new("/").join(rel_path);

                // Check if any pattern matches this path
                for pattern in patterns {
                    if pattern.matches(&absolute_path) {
                        return false; // Don't descend into this directory
                    }
                }
            }

            true // Continue descending
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Error walking directory: {}", e);
                continue;
            }
        };

        let path = entry.path();

        // Get relative path for pattern matching
        let relative_path = match path.strip_prefix(snapshot_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Convert to absolute path for pattern matching
        let absolute_path = Path::new("/").join(relative_path);

        // Check if this path matches any pattern
        let matches = patterns
            .iter()
            .any(|pattern| pattern.matches(&absolute_path));

        if matches {
            // Delete this file or directory
            if path.is_dir() {
                match fs::remove_dir_all(path) {
                    Ok(_) => {
                        log::debug!("Excluded directory: {}", absolute_path.display());
                        deleted_count += 1;
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to delete excluded directory {}: {}",
                            absolute_path.display(),
                            e
                        );
                        failed_count += 1;
                    }
                }
            } else if path.is_file() {
                match fs::remove_file(path) {
                    Ok(_) => {
                        log::debug!("Excluded file: {}", absolute_path.display());
                        deleted_count += 1;
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to delete excluded file {}: {}",
                            absolute_path.display(),
                            e
                        );
                        failed_count += 1;
                    }
                }
            }
        }
    }

    log::info!(
        "Exclusion complete: {} items excluded, {} failures",
        deleted_count,
        failed_count
    );

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
    ensure_snapshot_name(name)?;
    let snapshot_path = snapshot_dir().join(name);
    ensure_within_snapshot_dir(&snapshot_path)?;

    if !snapshot_path.exists() {
        bail!("Snapshot not found: {}", name);
    }

    // Check if it's a directory (new multi-subvolume format) or a single subvolume (old format)
    if snapshot_path.is_dir() {
        // New format: directory containing subvolume snapshots
        // Delete all subvolume snapshots within this directory
        let entries = fs::read_dir(&snapshot_path).context("Failed to read snapshot directory")?;

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
                    log::warn!("Failed to delete {}: {}", subvol_path.display(), stderr);
                }
            }
        }

        // Remove the parent directory
        fs::remove_dir(&snapshot_path).context("Failed to remove snapshot directory")?;
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
    let snapshot_base_path = snapshot_dir().join(name);
    ensure_within_snapshot_dir(&snapshot_base_path)?;

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
            log::warn!(
                "/etc/fstab not found in snapshot, multi-subvolume restore may not work correctly"
            );
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

/// Get sizes for multiple snapshots efficiently
/// Returns a HashMap mapping snapshot names to sizes in bytes
///
/// This runs as root via D-Bus, so it can access snapshot directories
/// without permission issues. Uses parallel processing for speed.
pub fn get_snapshot_sizes(snapshot_names: Vec<String>) -> Result<std::collections::HashMap<String, u64>> {
    use rayon::prelude::*;
    use std::collections::HashMap;

    // Get all snapshots to map names to paths
    let snapshots = load_snapshot_metadata()?;
    let name_to_path: HashMap<String, PathBuf> = snapshots
        .iter()
        .map(|s| (s.name.clone(), s.path.clone()))
        .collect();

    // Calculate sizes in parallel
    let results: HashMap<String, u64> = snapshot_names
        .par_iter()
        .filter_map(|name| {
            let path = name_to_path.get(name)?;
            let size = get_snapshot_size_impl(path).ok()?;
            Some((name.clone(), size))
        })
        .collect();

    Ok(results)
}

/// Internal implementation to get a single snapshot's size
fn get_snapshot_size_impl(path: &Path) -> Result<u64> {
    let output = Command::new("du")
        .arg("-sb")
        .arg(path)
        .output()
        .context("Failed to execute du command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.split_whitespace().collect();

    if parts.is_empty() {
        bail!("Failed to get snapshot size");
    }

    let size: u64 = parts[0].parse().context("Failed to parse snapshot size")?;
    Ok(size)
}

/// Verification result for a snapshot
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VerificationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Verify snapshot integrity
///
/// Checks:
/// - Snapshot directory exists
/// - All expected subvolumes exist
/// - Each subvolume is a valid btrfs subvolume
///
/// Returns a VerificationResult with any errors or warnings found
pub fn verify_snapshot(name: &str) -> Result<VerificationResult> {
    ensure_snapshot_name(name)?;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Check snapshot base directory exists first
    let snapshot_base_path = snapshot_dir().join(name);
    if !snapshot_base_path.exists() {
        errors.push(format!(
            "Snapshot directory does not exist: {}",
            snapshot_base_path.display()
        ));
        return Ok(VerificationResult {
            is_valid: false,
            errors,
            warnings,
        });
    }

    // Try to get snapshot metadata - warn if missing but continue verification
    let snapshot_meta_opt = match get_snapshot_metadata(name) {
        Ok(meta) => Some(meta),
        Err(e) => {
            warnings.push(format!(
                "Snapshot metadata not found (this is normal for older snapshots): {}",
                e
            ));
            None
        }
    };

    // Check if snapshot is a directory (multi-subvolume) or single subvolume
    if snapshot_base_path.is_dir() {
        // Directory format - could be multi-subvolume or old single-subvolume in directory
        if let Some(snapshot_meta) = snapshot_meta_opt {
            // We have metadata - verify expected subvolumes
            for subvol_mount in &snapshot_meta.subvolumes {
                let subvol_name = if subvol_mount == &PathBuf::from("/") {
                    "root".to_string()
                } else {
                    subvol_mount
                        .to_string_lossy()
                        .trim_start_matches('/')
                        .replace('/', "_")
                };

                let subvol_path = snapshot_base_path.join(&subvol_name);

                // Check if subvolume exists
                if !subvol_path.exists() {
                    errors.push(format!(
                        "Subvolume snapshot missing: {} (expected at {})",
                        subvol_name,
                        subvol_path.display()
                    ));
                    continue;
                }

                // Verify it's a valid btrfs subvolume
                let path = snapshot_base_path.join(&subvol_name);
                ensure_within_snapshot_dir(&path)?;
                match Command::new("btrfs")
                    .arg("subvolume")
                    .arg("show")
                    .arg(&path)
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        // Subvolume is valid
                    }
                    Ok(_) => {
                        errors.push(format!(
                            "Path exists but is not a valid btrfs subvolume: {}",
                            subvol_path.display()
                        ));
                    }
                    Err(e) => {
                        warnings.push(format!(
                            "Could not verify subvolume {}: {}",
                            subvol_path.display(),
                            e
                        ));
                    }
                }
            }
        } else {
            // No metadata - just verify the directory contains at least one valid subvolume
            let mut found_valid_subvol = false;
            if let Ok(entries) = fs::read_dir(&snapshot_base_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Ok(output) = Command::new("btrfs")
                        .arg("subvolume")
                        .arg("show")
                        .arg(&path)
                        .output()
                    {
                        if output.status.success() {
                            found_valid_subvol = true;
                            break;
                        }
                    }
                }
            }

            if !found_valid_subvol {
                errors.push(format!(
                    "No valid btrfs subvolumes found in {}",
                    snapshot_base_path.display()
                ));
            }
        }
    } else {
        // Single subvolume (old format or direct subvolume)
        // Verify it's a valid btrfs subvolume
        match Command::new("btrfs")
            .arg("subvolume")
            .arg("show")
            .arg(&snapshot_base_path)
            .output()
        {
            Ok(output) if output.status.success() => {
                // Subvolume is valid
            }
            Ok(_) => {
                errors.push(format!(
                    "Path exists but is not a valid btrfs subvolume: {}",
                    snapshot_base_path.display()
                ));
            }
            Err(e) => {
                warnings.push(format!(
                    "Could not verify subvolume {}: {}",
                    snapshot_base_path.display(),
                    e
                ));
            }
        }
    }

    Ok(VerificationResult {
        is_valid: errors.is_empty(),
        errors,
        warnings,
    })
}

/// Package change information for restore preview
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageChange {
    pub name: String,
    pub current_version: Option<String>,
    pub snapshot_version: Option<String>,
    pub change_type: String, // "add", "remove", "upgrade", "downgrade", "unchanged"
}

/// Preview of what will happen if a snapshot is restored
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RestorePreview {
    pub snapshot_name: String,
    pub snapshot_timestamp: String,
    pub snapshot_description: Option<String>,
    pub current_kernel: Option<String>,
    pub snapshot_kernel: Option<String>,
    pub affected_subvolumes: Vec<String>,
    pub packages_to_add: Vec<PackageChange>,
    pub packages_to_remove: Vec<PackageChange>,
    pub packages_to_upgrade: Vec<PackageChange>,
    pub packages_to_downgrade: Vec<PackageChange>,
    pub total_package_changes: usize,
}

/// Preview what will happen if a snapshot is restored
///
/// Compares the snapshot's state with the current system state to show:
/// - Which packages will be added, removed, upgraded, or downgraded
/// - Kernel version changes
/// - Which subvolumes will be affected
pub fn preview_restore(name: &str) -> Result<RestorePreview> {
    use crate::packages::get_installed_packages;
    use std::collections::HashMap;

    ensure_snapshot_name(name)?;

    // Get snapshot metadata
    let snapshot_meta = get_snapshot_metadata(name).context("Failed to load snapshot metadata")?;

    // Get current packages
    let current_packages =
        get_installed_packages().context("Failed to get current installed packages")?;

    // Build maps for easy lookup
    let current_pkg_map: HashMap<String, String> = current_packages
        .iter()
        .map(|p| (p.name.clone(), p.version.clone()))
        .collect();

    let snapshot_pkg_map: HashMap<String, String> = snapshot_meta
        .packages
        .iter()
        .map(|p| (p.name.clone(), p.version.clone()))
        .collect();

    // Categorize package changes
    let mut packages_to_add = Vec::new();
    let mut packages_to_remove = Vec::new();
    let mut packages_to_upgrade = Vec::new();
    let mut packages_to_downgrade = Vec::new();

    // Check packages in snapshot
    for (snap_name, snap_version) in &snapshot_pkg_map {
        match current_pkg_map.get(snap_name) {
            None => {
                // Package is in snapshot but not currently installed - will be added
                packages_to_add.push(PackageChange {
                    name: snap_name.clone(),
                    current_version: None,
                    snapshot_version: Some(snap_version.clone()),
                    change_type: "add".to_string(),
                });
            }
            Some(current_version) => {
                if current_version != snap_version {
                    // Version mismatch - determine if upgrade or downgrade
                    let ordering = compare(current_version, snap_version)
                        .map(|cmp| match cmp {
                            Cmp::Lt => std::cmp::Ordering::Less,
                            Cmp::Eq => std::cmp::Ordering::Equal,
                            Cmp::Gt => std::cmp::Ordering::Greater,
                            _ => std::cmp::Ordering::Equal,
                        })
                        .unwrap_or(std::cmp::Ordering::Equal);
                    let change = if ordering == std::cmp::Ordering::Greater {
                        PackageChange {
                            name: snap_name.clone(),
                            current_version: Some(current_version.clone()),
                            snapshot_version: Some(snap_version.clone()),
                            change_type: "downgrade".to_string(),
                        }
                    } else {
                        PackageChange {
                            name: snap_name.clone(),
                            current_version: Some(current_version.clone()),
                            snapshot_version: Some(snap_version.clone()),
                            change_type: "upgrade".to_string(),
                        }
                    };

                    if ordering == std::cmp::Ordering::Greater {
                        packages_to_downgrade.push(change);
                    } else if ordering == std::cmp::Ordering::Less {
                        packages_to_upgrade.push(change);
                    }
                }
            }
        }
    }

    // Check for packages currently installed but not in snapshot (will be removed)
    for (current_name, current_version) in &current_pkg_map {
        if !snapshot_pkg_map.contains_key(current_name) {
            packages_to_remove.push(PackageChange {
                name: current_name.clone(),
                current_version: Some(current_version.clone()),
                snapshot_version: None,
                change_type: "remove".to_string(),
            });
        }
    }

    // Get current kernel version
    let current_kernel = get_current_kernel_version().ok();

    // Format affected subvolumes
    let affected_subvolumes: Vec<String> = snapshot_meta
        .subvolumes
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let total_package_changes = packages_to_add.len()
        + packages_to_remove.len()
        + packages_to_upgrade.len()
        + packages_to_downgrade.len();

    Ok(RestorePreview {
        snapshot_name: snapshot_meta.name.clone(),
        snapshot_timestamp: snapshot_meta
            .timestamp
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string(),
        snapshot_description: snapshot_meta.description.clone(),
        current_kernel,
        snapshot_kernel: snapshot_meta.kernel_version.clone(),
        affected_subvolumes,
        packages_to_add,
        packages_to_remove,
        packages_to_upgrade,
        packages_to_downgrade,
        total_package_changes,
    })
}

/// Get current kernel version
fn get_current_kernel_version() -> Result<String> {
    let output = Command::new("uname")
        .arg("-r")
        .output()
        .context("Failed to execute uname")?;

    if !output.status.success() {
        bail!("Failed to get kernel version");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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

pub fn ensure_snapshot_name(name: &str) -> Result<()> {
    waypoint_common::validate_snapshot_name(name)
        .map_err(|e| anyhow!("Invalid snapshot name '{}': {}", name, e))
}

fn ensure_within_snapshot_dir(path: &Path) -> Result<()> {
    let base = snapshot_dir();
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !canonical.starts_with(base) {
        bail!(
            "Snapshot path {} is outside of snapshot directory {}",
            canonical.display(),
            base.display()
        );
    }
    Ok(())
}

/// Load snapshot metadata from file
fn load_snapshot_metadata() -> Result<Vec<Snapshot>> {
    let path = metadata_file();

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path).context("Failed to read snapshots metadata")?;

    let parsed: Vec<Snapshot> =
        serde_json::from_str(&content).context("Failed to parse snapshots metadata")?;

    let base_dir = snapshot_dir();
    let mut sanitized = Vec::with_capacity(parsed.len());

    for mut snapshot in parsed {
        if let Err(e) = ensure_snapshot_name(&snapshot.name) {
            log::warn!(
                "Ignoring snapshot metadata entry with invalid name '{}': {}",
                snapshot.name,
                e
            );
            continue;
        }

        let resolved_path = base_dir.join(&snapshot.name);
        if !resolved_path.starts_with(base_dir) {
            log::warn!(
                "Ignoring snapshot metadata entry '{}' with unexpected path {}",
                snapshot.name,
                resolved_path.display()
            );
            continue;
        }

        snapshot.path = resolved_path;
        sanitized.push(snapshot);
    }

    Ok(sanitized)
}

/// Save snapshot metadata to file
fn save_snapshot_metadata(snapshots: &[Snapshot]) -> Result<()> {
    let path = metadata_file();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create metadata directory")?;
    }

    let content =
        serde_json::to_string_pretty(snapshots).context("Failed to serialize snapshots")?;

    fs::write(path, content).context("Failed to write snapshots metadata")?;

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
    ensure_snapshot_name(name)?;
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

/// Create a backup of /etc/fstab before modification
///
/// # Arguments
/// * `fstab_path` - Path to the fstab file to backup
///
/// # Returns
/// Path to the created backup file
///
/// # Backup Strategy
/// - Creates /etc/fstab.bak if it doesn't exist
/// - If backup already exists, creates timestamped backup /etc/fstab.bak.TIMESTAMP
fn backup_fstab(fstab_path: &Path) -> Result<PathBuf> {
    let mut backup_path = fstab_path.with_extension("bak");

    if backup_path.exists() {
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let base_name = fstab_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("fstab");
        backup_path = fstab_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{}.bak.{}", base_name, timestamp));
    }

    // Copy fstab to backup
    fs::copy(fstab_path, &backup_path).context(format!(
        "Failed to create fstab backup at {}",
        backup_path.display()
    ))?;

    log::info!("Created fstab backup: {}", backup_path.display());

    Ok(backup_path)
}

/// Update fstab in a snapshot to mount the correct subvolume snapshots
fn update_fstab_for_snapshot(
    fstab_path: &Path,
    snapshot_name: &str,
    subvolumes: &[PathBuf],
) -> Result<()> {
    // Create backup before modifying fstab
    backup_fstab(fstab_path)?;

    // Read current fstab
    let fstab_content = fs::read_to_string(fstab_path).context("Failed to read fstab")?;

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
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                new_parts[0], new_parts[1], new_parts[2], new_parts[3], new_parts[4], new_parts[5]
            )
        } else {
            new_parts.join("\t")
        };

        updated_lines.push(new_line);
        updated = true;
    }

    if updated {
        // Write updated fstab
        let new_content = updated_lines.join("\n") + "\n";
        fs::write(fstab_path, new_content).context("Failed to write updated fstab")?;
    }

    Ok(())
}

/// Update the subvol option in mount options string
fn update_subvol_option(options: &str, snapshot_name: &str, mount_point: &Path) -> Result<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_subvol_option_root_filesystem() {
        // Test updating subvol option for root filesystem
        let options = "rw,relatime,ssd,space_cache,subvol=/@";
        let snapshot_name = "snapshot-20251111-120000";
        let mount_point = PathBuf::from("/");

        let result = update_subvol_option(options, snapshot_name, &mount_point).unwrap();

        // Should replace subvol=@ with subvol=@snapshots/snapshot-20251111-120000/root
        assert!(result.contains("subvol=@snapshots/snapshot-20251111-120000/root"));
        assert!(result.contains("rw,relatime,ssd,space_cache"));
        assert!(!result.contains("subvol=/@"));
    }

    #[test]
    fn test_update_subvol_option_home_filesystem() {
        // Test updating subvol option for /home
        let options = "rw,relatime,ssd,subvol=/@home";
        let snapshot_name = "backup-2025";
        let mount_point = PathBuf::from("/home");

        let result = update_subvol_option(options, snapshot_name, &mount_point).unwrap();

        // Should use "home" as subvolume name
        assert!(result.contains("subvol=@snapshots/backup-2025/home"));
        assert!(!result.contains("subvol=/@home"));
    }

    #[test]
    fn test_update_subvol_option_with_subvolid() {
        // Test that subvolid= is also replaced
        let options = "rw,subvolid=256";
        let snapshot_name = "test-snapshot";
        let mount_point = PathBuf::from("/");

        let result = update_subvol_option(options, snapshot_name, &mount_point).unwrap();

        // subvolid should be replaced with subvol
        assert!(result.contains("subvol=@snapshots/test-snapshot/root"));
        assert!(!result.contains("subvolid=256"));
    }

    #[test]
    fn test_update_subvol_option_no_existing_subvol() {
        // Test adding subvol option when it doesn't exist
        let options = "rw,relatime,ssd";
        let snapshot_name = "new-snapshot";
        let mount_point = PathBuf::from("/");

        let result = update_subvol_option(options, snapshot_name, &mount_point).unwrap();

        // Should add subvol option
        assert!(result.contains("subvol=@snapshots/new-snapshot/root"));
        assert!(result.contains("rw,relatime,ssd"));
    }

    #[test]
    fn test_update_subvol_option_complex_mount_point() {
        // Test with nested mount point like /var/lib
        let options = "rw,subvol=/@var_lib";
        let snapshot_name = "snapshot-1";
        let mount_point = PathBuf::from("/var/lib");

        let result = update_subvol_option(options, snapshot_name, &mount_point).unwrap();

        // Should convert /var/lib to var_lib
        assert!(result.contains("subvol=@snapshots/snapshot-1/var_lib"));
    }

    #[test]
    fn test_update_subvol_option_preserves_other_options() {
        // Test that all other mount options are preserved
        let options = "rw,noatime,compress=zstd,space_cache=v2,subvol=/@,autodefrag";
        let snapshot_name = "test";
        let mount_point = PathBuf::from("/");

        let result = update_subvol_option(options, snapshot_name, &mount_point).unwrap();

        // All options except subvol should be preserved
        assert!(result.contains("rw"));
        assert!(result.contains("noatime"));
        assert!(result.contains("compress=zstd"));
        assert!(result.contains("space_cache=v2"));
        assert!(result.contains("autodefrag"));
        assert!(result.contains("subvol=@snapshots/test/root"));
    }

    #[test]
    fn test_update_subvol_option_snapshot_name_with_special_chars() {
        // Test snapshot names with hyphens and underscores
        let options = "rw,subvol=/@";
        let snapshot_name = "pre-upgrade_2025-01-11";
        let mount_point = PathBuf::from("/");

        let result = update_subvol_option(options, snapshot_name, &mount_point).unwrap();

        assert!(result.contains("subvol=@snapshots/pre-upgrade_2025-01-11/root"));
    }

    #[test]
    fn test_update_subvol_option_empty_options() {
        // Test with empty options string
        let options = "";
        let snapshot_name = "test";
        let mount_point = PathBuf::from("/");

        let result = update_subvol_option(options, snapshot_name, &mount_point).unwrap();

        // Should still add subvol option
        assert!(result.contains("subvol=@snapshots/test/root"));
    }
}
