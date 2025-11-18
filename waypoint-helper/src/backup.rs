//! Backup operations for waypoint-helper
//! This module handles btrfs send/receive operations with root privileges

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::SyncSender;
use waypoint_common::WaypointConfig;

/// Progress update message for backup operations
#[derive(Debug, Clone)]
pub struct BackupProgress {
    pub snapshot_id: String,
    /// UUID is filled in by the D-Bus layer, not directly read in this module
    #[allow(dead_code)]
    pub destination_uuid: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: u64,
    pub stage: String,
}

/// Drive type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DriveType {
    Removable, // USB, SD cards, etc.
    Network,   // NFS, CIFS, SSHFS
    Internal,  // Internal drives, eSATA
}

/// Represents a backup destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDestination {
    pub mount_point: String,
    pub label: String,
    pub drive_type: DriveType,
    pub uuid: Option<String>,
    pub fstype: String, // Filesystem type (btrfs, ntfs, exfat, etc.)
}

/// Scan for available backup destinations (mounted filesystems on external drives)
/// Supports btrfs (native), ntfs, exfat, vfat, cifs, nfs
pub fn scan_backup_destinations() -> Result<Vec<BackupDestination>> {
    // Get mount point, label, source device, filesystem type, and UUID
    // Support common backup filesystem types
    // Use JSON output to properly handle spaces in mount points and labels
    let output = Command::new("findmnt")
        .arg("-t")
        .arg("btrfs,ntfs,exfat,vfat,cifs,nfs,nfs4")
        .arg("-n") // No headings
        .arg("-l") // List format (not tree)
        .arg("-o")
        .arg("TARGET,LABEL,SOURCE,FSTYPE,UUID")
        .arg("-J") // JSON output - properly handles spaces and special chars
        .output()
        .context("Failed to list mounted filesystems")?;

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    #[derive(serde::Deserialize)]
    struct FindmntOutput {
        filesystems: Vec<FindmntEntry>,
    }

    #[derive(serde::Deserialize)]
    struct FindmntEntry {
        target: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        source: Option<String>,
        fstype: String,
        #[serde(default)]
        uuid: Option<String>,
    }

    let findmnt_result: FindmntOutput = serde_json::from_str(&output_str)
        .context("Failed to parse findmnt JSON output")?;

    let mut destinations = Vec::new();
    let mut seen_uuids = std::collections::HashSet::new();

    for entry in findmnt_result.filesystems {
        let mount_point = entry.target;

        // Only include external drives (not system partitions)
        // Exclude: root, home, system dirs, boot, swap, snapshots
        if mount_point == "/"
            || mount_point == "/home"
            || mount_point == "/boot"
            || mount_point == "/swap"
            || mount_point == "/.snapshots"
            || mount_point.starts_with("/var")
            || mount_point.starts_with("/tmp")
            || mount_point.starts_with("/sys")
            || mount_point.starts_with("/proc")
            || mount_point.starts_with("/dev")
        {
            continue;
        }

        // Exclude auto-mounted waypoint backup subvolumes
        // These are btrfs subvolumes within waypoint-backups/ that get auto-mounted
        // Check mount point path first
        if mount_point.contains("/waypoint-backups/")
        {
            log::debug!("Skipping waypoint backup subvolume at {}", mount_point);
            continue;
        }

        // Use label if available, otherwise use last component of mount path
        let label = entry.label
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| {
                PathBuf::from(&mount_point)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unnamed")
                    .to_string()
            });

        // Also exclude if label looks like a snapshot name
        let label_lower = label.to_lowercase();
        if label_lower.starts_with("snapshot-")
        {
            log::debug!("Skipping snapshot-like label: {}", label);
            continue;
        }

        // Detect drive type and filesystem
        let source = entry.source.as_deref().unwrap_or("");
        let fstype = entry.fstype;
        let drive_type = detect_drive_type(&mount_point, source, &fstype);

        // Skip duplicates by UUID (same drive mounted multiple times)
        if let Some(ref uuid) = entry.uuid {
            if !uuid.is_empty() {
                if seen_uuids.contains(uuid) {
                    log::info!("Skipping duplicate mount of {} at {}", label, mount_point);
                    continue;
                }
                seen_uuids.insert(uuid.clone());
            }
        }

        destinations.push(BackupDestination {
            mount_point,
            label,
            drive_type,
            uuid: entry.uuid,
            fstype,
        });
    }

    Ok(destinations)
}

/// Validate that a destination mount is a legitimate backup destination
/// Returns the canonical path if valid, error otherwise
fn validate_backup_destination(destination_mount: &str) -> Result<std::path::PathBuf> {
    let dest_path = Path::new(destination_mount);

    // Must exist
    if !dest_path.exists() {
        anyhow::bail!("Destination does not exist: {}", destination_mount);
    }

    // Canonicalize to resolve symlinks and get absolute path
    let canonical = dest_path.canonicalize()
        .with_context(|| format!("Failed to canonicalize destination path: {}", destination_mount))?;

    // Get list of valid backup destinations
    let valid_destinations = scan_backup_destinations()
        .context("Failed to scan valid backup destinations")?;

    // Check if the canonical path matches any valid destination
    let canonical_str = canonical.to_str()
        .ok_or_else(|| anyhow::anyhow!("Destination path contains invalid UTF-8: {}", canonical.display()))?;

    for dest in valid_destinations {
        // Canonicalize the valid destination for comparison
        if let Ok(valid_canonical) = Path::new(&dest.mount_point).canonicalize() {
            if canonical == valid_canonical {
                log::info!("Validated backup destination: {} ({})", dest.label, canonical_str);
                return Ok(canonical);
            }
        }
    }

    // If we get here, the destination is not in the valid list
    anyhow::bail!(
        "Security: Destination '{}' is not a valid backup destination. \
         Only removable drives and network shares returned by scan_backup_destinations are allowed. \
         This prevents writing to system directories.",
        canonical_str
    )
}

/// Validate that a backup path resides under a legitimate destination's waypoint-backups directory
fn validate_backup_path(backup_path: &Path) -> Result<PathBuf> {
    let canonical = backup_path.canonicalize()
        .with_context(|| format!("Failed to canonicalize backup path {}", backup_path.display()))?;

    let destinations = scan_backup_destinations()
        .context("Failed to enumerate backup destinations for validation")?;

    for dest in destinations {
        if let Ok(dest_canonical) = Path::new(&dest.mount_point).canonicalize() {
            let backups_root = dest_canonical.join("waypoint-backups");
            if canonical.starts_with(&backups_root) {
                return Ok(canonical);
            }
        }
    }

    anyhow::bail!(
        "Security: Backup path '{}' is not within a waypoint-backups directory on a trusted destination",
        canonical.display()
    )
}

/// Detect the type of drive based on mount point, source device, and filesystem type
fn detect_drive_type(_mount_point: &str, source: &str, fstype: &str) -> DriveType {
    // Check for network filesystems
    if fstype.contains("nfs")
        || fstype.contains("cifs")
        || fstype.contains("sshfs")
        || fstype.contains("fuse")
        || source.contains("://")
        || source.contains(":") && !source.starts_with("/dev/")
    {
        return DriveType::Network;
    }

    // Check if it's a removable device by examining /sys/block
    if let Some(device_name) = extract_device_name(source) {
        if is_removable_device(&device_name) {
            return DriveType::Removable;
        }
    }

    // Default to internal
    DriveType::Internal
}

/// Extract the block device name from a source path like /dev/sda1 -> sda
fn extract_device_name(source: &str) -> Option<String> {
    if !source.starts_with("/dev/") {
        return None;
    }

    let device = source.strip_prefix("/dev/")?;

    // Remove partition number (sda1 -> sda, nvme0n1p1 -> nvme0n1)
    if device.contains("nvme") || device.contains("mmcblk") {
        // NVMe and MMC devices have 'p' before partition number
        device.split('p').next().map(|s| s.to_string())
    } else {
        // Regular drives (sda1 -> sda)
        device
            .chars()
            .take_while(|c| !c.is_numeric())
            .collect::<String>()
            .into()
    }
}

/// Check if a block device is removable by reading /sys/block/*/removable
fn is_removable_device(device: &str) -> bool {
    let removable_path = format!("/sys/block/{}/removable", device);
    std::fs::read_to_string(&removable_path)
        .ok()
        .and_then(|content| content.trim().parse::<u8>().ok())
        .map(|val| val == 1)
        .unwrap_or(false)
}

/// Calculate the disk usage of a directory using du command
fn calculate_directory_size(path: &Path) -> Result<u64> {
    let output = Command::new("du")
        .arg("-sb") // -s for summary, -b for bytes
        .arg(path)
        .output()
        .context("Failed to run du command")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("du command failed"));
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let size_str = output_str
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid du output"))?;

    size_str
        .parse::<u64>()
        .context("Failed to parse size from du output")
}

/// Load snapshot metadata from the waypoint metadata file
/// This reads the snapshots.json file to get information about which subvolumes are included
fn load_snapshot_metadata(snapshot_name: &str) -> Result<waypoint_common::SnapshotInfo> {
    let config = WaypointConfig::new();
    let metadata_path = &config.metadata_file;

    if !metadata_path.exists() {
        bail!("Snapshot metadata file not found: {}", metadata_path.display());
    }

    let contents = fs::read_to_string(metadata_path)
        .context("Failed to read snapshot metadata")?;

    let snapshots: Vec<waypoint_common::SnapshotInfo> = serde_json::from_str(&contents)
        .context("Failed to parse snapshot metadata")?;

    snapshots
        .into_iter()
        .find(|s| s.name == snapshot_name)
        .ok_or_else(|| anyhow!("Snapshot '{}' not found in metadata", snapshot_name))
}

/// Convert a mount point path to a subdirectory name for backups
/// "/" becomes "root", "/home" becomes "home", "/var/lib" becomes "var_lib"
fn mount_point_to_subdir_name(mount_point: &Path) -> String {
    if mount_point == Path::new("/") {
        "root".to_string()
    } else {
        mount_point
            .to_string_lossy()
            .trim_start_matches('/')
            .replace('/', "_")
    }
}

/// Backup a snapshot to destination using btrfs send/receive or rsync
///
/// Automatically detects filesystem type and uses appropriate method:
/// - btrfs: Uses btrfs send/receive (supports incremental)
/// - ntfs/exfat/vfat/cifs/nfs: Uses rsync (full copy)
///
/// Returns a tuple of (backup_path, size_bytes)
pub fn backup_snapshot(
    snapshot_path: &str,
    destination_mount: &str,
    parent_snapshot: Option<&str>,
    progress_tx: Option<SyncSender<BackupProgress>>,
) -> Result<(String, u64)> {
    let snapshot = Path::new(snapshot_path);

    // Validate inputs
    if !snapshot.exists() {
        return Err(anyhow::anyhow!(
            "Snapshot does not exist: {}",
            snapshot_path
        ));
    }

    // SECURITY: Validate destination_mount is a legitimate backup destination
    // This prevents attackers from writing to arbitrary system directories like /etc, /usr, etc.
    let validated_dest = validate_backup_destination(destination_mount)?;
    let destination_mount_str = validated_dest.to_str()
        .ok_or_else(|| anyhow::anyhow!("Validated destination path contains invalid UTF-8"))?;

    // Detect destination filesystem type
    let fstype = detect_filesystem_type(destination_mount_str)?;

    // Route to appropriate backup method (use validated path)
    if fstype == "btrfs" {
        backup_snapshot_btrfs(snapshot_path, destination_mount_str, parent_snapshot, progress_tx)
    } else {
        backup_snapshot_rsync(snapshot_path, destination_mount_str, progress_tx)
    }
}

/// Detect the filesystem type of a mount point
fn detect_filesystem_type(mount_point: &str) -> Result<String> {
    let output = Command::new("findmnt")
        .arg("-n")
        .arg("-o")
        .arg("FSTYPE")
        .arg(mount_point)
        .output()
        .context("Failed to detect filesystem type")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Failed to query mount point"));
    }

    let fstype = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    if fstype.is_empty() {
        return Err(anyhow::anyhow!("Could not determine filesystem type"));
    }

    Ok(fstype)
}

/// Backup a single subvolume using btrfs send/receive
/// Returns Ok(()) on success
fn backup_single_subvolume_btrfs(
    subvol_path: &Path,
    parent_subvol: Option<&Path>,
    receive_dir: &Path,
) -> Result<()> {
    // Verify it's actually a btrfs subvolume
    let is_subvolume = Command::new("btrfs")
        .arg("subvolume")
        .arg("show")
        .arg(subvol_path)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !is_subvolume {
        bail!("Path is not a btrfs subvolume: {}", subvol_path.display());
    }

    // Build btrfs send command
    let mut send_cmd = Command::new("btrfs");
    send_cmd.arg("send");

    // Add parent if this is incremental
    if let Some(parent) = parent_subvol {
        if !parent.exists() {
            bail!("Parent subvolume does not exist: {}", parent.display());
        }
        send_cmd.arg("-p").arg(parent);
    }

    send_cmd.arg(subvol_path);
    send_cmd.stdout(std::process::Stdio::piped());
    send_cmd.stderr(std::process::Stdio::piped());

    // Build btrfs receive command
    let mut receive_cmd = Command::new("btrfs");
    receive_cmd.arg("receive").arg(receive_dir);

    // Execute send | receive pipeline
    let mut send_child = send_cmd.spawn().context("Failed to start btrfs send")?;

    let send_stdout = send_child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture send output"))?;

    let send_stderr_handle = send_child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            buf
        })
    });

    receive_cmd.stdin(send_stdout);

    let receive_output = receive_cmd
        .output()
        .context("Failed to run btrfs receive")?;

    let send_status = send_child.wait().context("Failed to wait for btrfs send")?;

    let send_stderr = match send_stderr_handle {
        Some(handle) => handle.join().unwrap_or_default(),
        None => String::new(),
    };

    if !send_status.success() {
        bail!(
            "btrfs send failed: {}{}",
            send_status,
            if send_stderr.trim().is_empty() {
                String::new()
            } else {
                format!(" - {}", send_stderr.trim())
            }
        );
    }

    if !receive_output.status.success() {
        let stderr = String::from_utf8_lossy(&receive_output.stderr);
        bail!("btrfs receive failed: {}", stderr);
    }

    Ok(())
}

/// Backup a snapshot to a btrfs destination using btrfs send/receive
/// Handles multi-subvolume snapshots by backing up each subvolume separately
///
/// Returns a tuple of (backup_path, size_bytes)
fn backup_snapshot_btrfs(
    snapshot_path: &str,
    destination_mount: &str,
    parent_snapshot: Option<&str>,
    progress_tx: Option<SyncSender<BackupProgress>>,
) -> Result<(String, u64)> {
    let snapshot = Path::new(snapshot_path);
    let dest_mount = Path::new(destination_mount);

    // Get snapshot name from path
    let snapshot_name = snapshot
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Invalid snapshot path"))?;

    // Load snapshot metadata to get list of subvolumes
    let metadata = load_snapshot_metadata(snapshot_name)
        .context("Failed to load snapshot metadata")?;

    // Send "preparing" stage
    if let Some(ref tx) = progress_tx {
        match tx.try_send(BackupProgress {
            snapshot_id: snapshot_name.to_string(),
            destination_uuid: String::new(), // Will be filled by caller
            bytes_transferred: 0,
            total_bytes: 0,
            speed_bytes_per_sec: 0,
            stage: "preparing".to_string(),
        }) {
            Ok(()) => {},
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                log::warn!("Progress channel full (preparing stage), consumer may be slow");
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                log::debug!("Progress channel disconnected, consumer has stopped");
            }
        }
    }

    // Create waypoint-backups directory at destination
    let backup_dir = dest_mount.join("waypoint-backups");
    fs::create_dir_all(&backup_dir).context("Failed to create backup directory")?;

    // Create snapshot-specific directory
    let snapshot_backup_dir = backup_dir.join(snapshot_name);
    fs::create_dir_all(&snapshot_backup_dir)
        .context("Failed to create snapshot backup directory")?;

    log::info!(
        "Backing up {} subvolumes for snapshot '{}'",
        metadata.subvolumes.len(),
        snapshot_name
    );

    // Send "transferring" stage
    if let Some(ref tx) = progress_tx {
        match tx.try_send(BackupProgress {
            snapshot_id: snapshot_name.to_string(),
            destination_uuid: String::new(),
            bytes_transferred: 0,
            total_bytes: 0,
            speed_bytes_per_sec: 0,
            stage: "transferring".to_string(),
        }) {
            Ok(()) => {},
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                log::warn!("Progress channel full (transferring stage), consumer may be slow");
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                log::debug!("Progress channel disconnected, consumer has stopped");
            }
        }
    }

    // Backup each subvolume
    for mount_point in &metadata.subvolumes {
        let subvol_name = mount_point_to_subdir_name(mount_point);
        let subvol_path = snapshot.join(&subvol_name);

        if !subvol_path.exists() {
            log::warn!(
                "Subvolume '{}' not found in snapshot, skipping",
                subvol_name
            );
            continue;
        }

        log::info!("Backing up subvolume: {} ({})", subvol_name, mount_point.display());

        // Determine parent subvolume for incremental backup
        let parent_subvol = if let Some(parent_snap) = parent_snapshot {
            let parent_path = Path::new(parent_snap);
            let parent_subvol_path = parent_path.join(&subvol_name);
            if parent_subvol_path.exists() {
                Some(parent_subvol_path)
            } else {
                log::warn!(
                    "Parent subvolume '{}' not found, doing full backup",
                    subvol_name
                );
                None
            }
        } else {
            None
        };

        // Create subdirectory for this subvolume in the backup
        let subvol_backup_dir = snapshot_backup_dir.join(&subvol_name);
        if let Some(parent) = subvol_backup_dir.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create subvolume backup directory")?;
        } else {
            anyhow::bail!("Backup directory has no parent: {}", subvol_backup_dir.display());
        }

        // Backup this subvolume
        backup_single_subvolume_btrfs(
            &subvol_path,
            parent_subvol.as_deref(),
            &snapshot_backup_dir,
        )
        .with_context(|| format!("Failed to backup subvolume '{}'", subvol_name))?;

        log::info!("Successfully backed up subvolume: {}", subvol_name);
    }

    // Calculate total backup size
    let size_bytes = calculate_directory_size(&snapshot_backup_dir)?;

    log::info!(
        "Backup complete: {} ({} bytes)",
        snapshot_backup_dir.display(),
        size_bytes
    );

    // Send "complete" stage
    if let Some(ref tx) = progress_tx {
        match tx.try_send(BackupProgress {
            snapshot_id: snapshot_name.to_string(),
            destination_uuid: String::new(),
            bytes_transferred: size_bytes,
            total_bytes: size_bytes,
            speed_bytes_per_sec: 0,
            stage: "complete".to_string(),
        }) {
            Ok(()) => {},
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                log::warn!("Progress channel full (complete stage), consumer may be slow");
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                log::debug!("Progress channel disconnected, consumer has stopped");
            }
        }
    }

    Ok((snapshot_backup_dir.to_string_lossy().to_string(), size_bytes))
}

/// Backup a snapshot to a non-btrfs destination using rsync
/// Handles multi-subvolume snapshots by rsyncing each subvolume into separate directories
///
/// Returns a tuple of (backup_path, size_bytes)
fn backup_snapshot_rsync(
    snapshot_path: &str,
    destination_mount: &str,
    progress_tx: Option<SyncSender<BackupProgress>>,
) -> Result<(String, u64)> {
    let snapshot = Path::new(snapshot_path);
    let dest_mount = Path::new(destination_mount);

    // Get snapshot name
    let snapshot_name = snapshot
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Invalid snapshot path"))?;

    // Load snapshot metadata to get list of subvolumes
    let metadata = load_snapshot_metadata(snapshot_name)
        .context("Failed to load snapshot metadata")?;

    // Send "preparing" stage
    if let Some(ref tx) = progress_tx {
        let _ = tx.send(BackupProgress {
            snapshot_id: snapshot_name.to_string(),
            destination_uuid: String::new(),
            bytes_transferred: 0,
            total_bytes: 0,
            speed_bytes_per_sec: 0,
            stage: "preparing".to_string(),
        });
    }

    // Create waypoint-backups directory at destination
    let backup_dir = dest_mount.join("waypoint-backups");
    fs::create_dir_all(&backup_dir).context("Failed to create backup directory")?;

    // Create snapshot-specific directory
    let snapshot_backup_dir = backup_dir.join(snapshot_name);
    fs::create_dir_all(&snapshot_backup_dir)
        .context("Failed to create snapshot backup directory")?;

    log::info!(
        "Backing up {} subvolumes for snapshot '{}' using rsync",
        metadata.subvolumes.len(),
        snapshot_name
    );

    // Send "transferring" stage
    if let Some(ref tx) = progress_tx {
        match tx.try_send(BackupProgress {
            snapshot_id: snapshot_name.to_string(),
            destination_uuid: String::new(),
            bytes_transferred: 0,
            total_bytes: 0,
            speed_bytes_per_sec: 0,
            stage: "transferring".to_string(),
        }) {
            Ok(()) => {},
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                log::warn!("Progress channel full (transferring stage), consumer may be slow");
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                log::debug!("Progress channel disconnected, consumer has stopped");
            }
        }
    }

    // Backup each subvolume
    for mount_point in &metadata.subvolumes {
        let subvol_name = mount_point_to_subdir_name(mount_point);
        let subvol_snapshot_path = snapshot.join(&subvol_name);

        if !subvol_snapshot_path.exists() {
            log::warn!(
                "Subvolume '{}' not found in snapshot, skipping",
                subvol_name
            );
            continue;
        }

        // For btrfs subvolumes, the actual filesystem is inside a "root" subdirectory
        // For rsync backups, we want to copy the contents, not the subvolume structure
        let source_dir = subvol_snapshot_path.join("root");
        if !source_dir.exists() {
            log::warn!(
                "Subvolume '{}' does not have a root directory, skipping",
                subvol_name
            );
            continue;
        }

        log::info!("Backing up subvolume: {} ({})", subvol_name, mount_point.display());

        // Create destination directory for this subvolume
        let dest_subvol_dir = snapshot_backup_dir.join(&subvol_name);
        fs::create_dir_all(&dest_subvol_dir)
            .context("Failed to create subvolume backup directory")?;

        // Use rsync to copy snapshot contents
        //
        // Flags overview:
        // - -aHAX: archive + preserve hard-links, ACLs, xattrs
        // - --delete-after: defer deletions until the end to reduce random seeks
        // - --inplace/--partial: write in-place so only touched blocks are updated and allow resume
        // - --no-inc-recursive: avoid the incremental recursion bookkeeping (less metadata churn)
        // - --human-readable/--info=progress2/--outbuf=L: friendlier logging + steady progress output
        let output = Command::new("rsync")
            .arg("-aHAX")
            .arg("--delete-after")
            .arg("--inplace")
            .arg("--partial")
            .arg("--no-inc-recursive")
            .arg("--human-readable")
            .arg("--info=progress2")
            .arg("--outbuf=L")
            .arg(format!("{}/", source_dir.display())) // Trailing slash = copy contents
            .arg(&dest_subvol_dir)
            .output()
            .context("Failed to run rsync")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("rsync failed for subvolume '{}': {}", subvol_name, stderr);
        }

        log::info!("Successfully backed up subvolume: {}", subvol_name);
    }

    // Calculate total backup size
    let size_bytes = calculate_directory_size(&snapshot_backup_dir)?;

    log::info!(
        "Backup complete: {} ({} bytes)",
        snapshot_backup_dir.display(),
        size_bytes
    );

    // Send "complete" stage
    if let Some(ref tx) = progress_tx {
        match tx.try_send(BackupProgress {
            snapshot_id: snapshot_name.to_string(),
            destination_uuid: String::new(),
            bytes_transferred: size_bytes,
            total_bytes: size_bytes,
            speed_bytes_per_sec: 0,
            stage: "complete".to_string(),
        }) {
            Ok(()) => {},
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                log::warn!("Progress channel full (complete stage), consumer may be slow");
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                log::debug!("Progress channel disconnected, consumer has stopped");
            }
        }
    }

    Ok((snapshot_backup_dir.to_string_lossy().to_string(), size_bytes))
}

/// List backups at a destination
/// Supports both btrfs subvolumes and rsync'd directories
pub fn list_backups(destination_mount: &str) -> Result<Vec<String>> {
    let backup_dir = Path::new(destination_mount).join("waypoint-backups");

    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();

    for entry in std::fs::read_dir(&backup_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip if not a directory
        if !path.is_dir() {
            continue;
        }

        // Check if it's a btrfs subvolume or a regular directory
        let is_btrfs_subvolume = Command::new("btrfs")
            .arg("subvolume")
            .arg("show")
            .arg(&path)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if is_btrfs_subvolume {
            // It's a btrfs backup
            backups.push(path.to_string_lossy().to_string());
        } else {
            // It's likely an rsync backup - verify it has snapshot structure
            // A valid backup should have typical filesystem directories
            let has_valid_structure = path.join("etc").exists()
                || path.join("home").exists()
                || path.join("usr").exists();

            if has_valid_structure {
                backups.push(path.to_string_lossy().to_string());
            } else {
                log::warn!("Skipping invalid backup directory: {}", path.display());
            }
        }
    }

    Ok(backups)
}

/// Restore a backup from destination to snapshots directory
/// Automatically detects if backup is btrfs subvolume or rsync directory
///
/// TODO: Multi-subvolume restore support
/// Currently, this function assumes single-subvolume backups. For multi-subvolume
/// backups created after the multi-subvolume backup feature, this needs to:
/// 1. Detect if the backup directory contains multiple subvolumes
/// 2. Restore each subvolume to the correct location
/// 3. Recreate the snapshot directory structure
pub fn restore_from_backup(backup_path: &str, snapshots_dir: &str) -> Result<String> {
    let backup = Path::new(backup_path);
    let dest = Path::new(snapshots_dir);

    if !backup.is_absolute() || !dest.is_absolute() {
        return Err(anyhow::anyhow!("Paths must be absolute"));
    }

    // Canonicalize both paths - this validates they exist and resolves symlinks
    // The canonicalization happens immediately before use to minimize TOCTOU window
    //
    // SECURITY NOTE: There is still a small race window between canonicalization and use
    // where an attacker with filesystem write access could replace the path with a symlink.
    // Mitigations in place:
    // 1. Polkit authentication required (must be admin)
    // 2. Path validation requires "waypoint-backups" substring
    // 3. Commands use .arg() preventing shell injection
    // 4. Immediate re-verification after canonicalization
    let backup = validate_backup_path(backup)?;
    let dest = dest
        .canonicalize()
        .context("Failed to resolve snapshots directory - does not exist or is inaccessible")?;

    // SECURITY: Validate snapshots_dir is the legitimate snapshot directory
    // This prevents attackers from restoring to arbitrary directories like / or /etc
    use waypoint_common::WaypointConfig;
    let config = WaypointConfig::new();
    let expected_snapshot_dir = config.snapshot_dir.canonicalize()
        .context("Failed to canonicalize configured snapshot directory")?;

    if dest != expected_snapshot_dir {
        return Err(anyhow::anyhow!(
            "Security: Destination directory '{}' does not match the configured snapshot directory '{}'. \
             This prevents restoring backups to arbitrary filesystem locations.",
            dest.display(),
            expected_snapshot_dir.display()
        ));
    }

    // Re-verify the backup path still exists and hasn't been swapped
    // This reduces the TOCTOU window further
    if !backup.exists() {
        return Err(anyhow::anyhow!(
            "Backup path no longer exists - possible race condition or filesystem modification"
        ));
    }

    // Detect if backup is a btrfs subvolume or rsync directory
    // Use the canonicalized path immediately to minimize race window
    let is_btrfs_subvolume = Command::new("btrfs")
        .arg("subvolume")
        .arg("show")
        .arg(&backup)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if is_btrfs_subvolume {
        restore_from_backup_btrfs(&backup, &dest)
    } else {
        restore_from_backup_rsync(&backup, &dest)
    }
}

/// Restore a btrfs backup using btrfs send/receive
fn restore_from_backup_btrfs(backup: &Path, dest: &Path) -> Result<String> {

    // Build send command
    let mut send_cmd = Command::new("btrfs");
    send_cmd
        .arg("send")
        .arg(&backup)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Build receive command
    let mut receive_cmd = Command::new("btrfs");
    receive_cmd.arg("receive").arg(&dest);

    // Execute pipeline
    let mut send_child = send_cmd.spawn().context("Failed to start btrfs send")?;

    let send_stdout = send_child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture send output"))?;

    let send_stderr_handle = send_child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            buf
        })
    });

    receive_cmd.stdin(send_stdout);

    let receive_output = receive_cmd
        .output()
        .context("Failed to run btrfs receive")?;

    let send_status = send_child.wait().context("Failed to wait for btrfs send")?;

    let send_stderr = match send_stderr_handle {
        Some(handle) => handle.join().unwrap_or_default(),
        None => String::new(),
    };

    if !send_status.success() {
        return Err(anyhow::anyhow!(
            "btrfs send failed: {}{}",
            send_status,
            if send_stderr.trim().is_empty() {
                String::new()
            } else {
                format!(" - {}", send_stderr.trim())
            }
        ));
    }

    if !receive_output.status.success() {
        let stderr = String::from_utf8_lossy(&receive_output.stderr);
        return Err(anyhow::anyhow!("btrfs receive failed: {}", stderr));
    }

    // Return restored snapshot path
    let snapshot_name = backup
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid backup path"))?;

    let restored_path = dest.join(snapshot_name);
    Ok(restored_path.to_string_lossy().to_string())
}

/// Restore an rsync backup by creating a new btrfs snapshot and copying files
fn restore_from_backup_rsync(backup: &Path, dest: &Path) -> Result<String> {
    // Get backup name
    let snapshot_name = backup
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid backup path"))?;

    let restored_path = dest.join(snapshot_name);

    // Create a new btrfs subvolume for the restored snapshot
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("create")
        .arg(&restored_path)
        .output()
        .context("Failed to create restore subvolume")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Failed to create subvolume: {}", stderr));
    }

    // Create the "root" directory inside the subvolume
    let root_dir = restored_path.join("root");
    std::fs::create_dir_all(&root_dir).context("Failed to create root directory")?;

    // Use rsync to copy backup contents into the root directory
    let output = Command::new("rsync")
        .arg("-aHAX")
        .arg(format!("{}/", backup.display())) // Trailing slash = copy contents
        .arg(&root_dir)
        .output()
        .context("Failed to run rsync for restore")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Clean up failed restore
        let _ = Command::new("btrfs")
            .arg("subvolume")
            .arg("delete")
            .arg(&restored_path)
            .output();
        return Err(anyhow::anyhow!("rsync restore failed: {}", stderr));
    }

    Ok(restored_path.to_string_lossy().to_string())
}

/// Drive health statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub backup_count: usize,
    pub last_backup_timestamp: Option<i64>, // Unix timestamp
    pub oldest_backup_timestamp: Option<i64>,
}

/// Get drive statistics for a backup destination
pub fn get_drive_stats(destination_mount: &str) -> Result<DriveStats> {
    use std::os::unix::fs::MetadataExt;

    let mount_path = Path::new(destination_mount);

    // Get filesystem space using statvfs
    let stats = nix::sys::statvfs::statvfs(mount_path)
        .context("Failed to get filesystem statistics")?;

    let total_bytes = stats.blocks() * stats.block_size();
    let available_bytes = stats.blocks_available() * stats.block_size();
    let used_bytes = total_bytes - available_bytes;

    // Get backup information
    let backup_dir = mount_path.join("waypoint-backups");
    let mut backup_count = 0;
    let mut last_backup_timestamp: Option<i64> = None;
    let mut oldest_backup_timestamp: Option<i64> = None;

    if backup_dir.exists() {
        for entry in std::fs::read_dir(&backup_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            // Check if it's a valid backup
            // Backups are directories containing subvolume directories (root/, home/, etc.)
            // or btrfs subvolumes themselves
            let is_valid_backup = {
                // Check if this directory contains subvolume directories
                // Common subvolume names: root, home, var, etc.
                let has_subvolumes = path.join("root").exists()
                    || path.join("home").exists()
                    || path.join("var").exists();

                if has_subvolumes {
                    true
                } else {
                    // For single-subvolume backups, check if it's a btrfs subvolume
                    Command::new("btrfs")
                        .arg("subvolume")
                        .arg("show")
                        .arg(&path)
                        .output()
                        .map(|output| output.status.success())
                        .unwrap_or(false)
                }
            };

            if !is_valid_backup {
                continue;
            }

            backup_count += 1;

            // Get modification time
            if let Ok(metadata) = entry.metadata() {
                let mtime = metadata.mtime();

                match last_backup_timestamp {
                    None => last_backup_timestamp = Some(mtime),
                    Some(current) if mtime > current => last_backup_timestamp = Some(mtime),
                    _ => {}
                }

                match oldest_backup_timestamp {
                    None => oldest_backup_timestamp = Some(mtime),
                    Some(current) if mtime < current => oldest_backup_timestamp = Some(mtime),
                    _ => {}
                }
            }
        }
    }

    Ok(DriveStats {
        total_bytes,
        used_bytes,
        available_bytes,
        backup_count,
        last_backup_timestamp,
        oldest_backup_timestamp,
    })
}

/// Check if a path is on a btrfs filesystem
fn is_btrfs_filesystem(path: &Path) -> Result<bool> {
    let output = Command::new("stat")
        .args(["-f", "-c", "%T"])
        .arg(path)
        .output()?;

    if !output.status.success() {
        bail!("Failed to check filesystem type");
    }

    let fstype = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(fstype == "btrfs")
}

/// Verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub success: bool,
    pub message: String,
    pub details: Vec<String>,
}

/// Verify a backup exists and check its integrity
pub fn verify_backup(
    snapshot_path: &str,
    destination_mount: &str,
    snapshot_id: &str,
) -> Result<VerificationResult> {
    let config = WaypointConfig::new();
    let snapshot_path = Path::new(snapshot_path);
    let destination_mount = Path::new(destination_mount);

    // Ensure snapshot lives within configured snapshot directory
    let canonical_snapshot = snapshot_path
        .canonicalize()
        .context("Failed to resolve snapshot path")?;
    let canonical_snapshot_root = config
        .snapshot_dir
        .canonicalize()
        .context("Failed to resolve configured snapshot directory")?;
    if !canonical_snapshot.starts_with(&canonical_snapshot_root) {
        anyhow::bail!(
            "Security: Snapshot path {} is outside configured snapshot directory {}",
            canonical_snapshot.display(),
            canonical_snapshot_root.display()
        );
    }

    // Ensure destination mount is a valid backup destination
    let canonical_destination = validate_backup_destination(
        destination_mount
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Destination mount contains invalid UTF-8"))?,
    )?;

    // Check if original snapshot exists
    if !canonical_snapshot.exists() {
        return Ok(VerificationResult {
            success: false,
            message: "Original snapshot not found".to_string(),
            details: vec![format!(
                "Snapshot path {} does not exist",
                canonical_snapshot.display()
            )],
        });
    }

    // Find the backup directory
    let waypoint_backups = canonical_destination.join("waypoint-backups");
    if !waypoint_backups.exists() {
        return Ok(VerificationResult {
            success: false,
            message: "No backups found on destination".to_string(),
            details: vec![format!("Directory {} does not exist", waypoint_backups.display())],
        });
    }

    let backup_path = waypoint_backups.join(snapshot_id);
    if !backup_path.exists() {
        return Ok(VerificationResult {
            success: false,
            message: "Backup not found".to_string(),
            details: vec![format!("Backup {} does not exist on destination", snapshot_id)],
        });
    }

    // Check if destination is btrfs
    let is_btrfs_dest = is_btrfs_filesystem(&canonical_destination)?;
    let mut details = Vec::new();

    if is_btrfs_dest {
        // For btrfs: verify subvolume properties
        details.push("Destination filesystem: btrfs".to_string());

        // Check if backup is a valid btrfs subvolume
        let output = Command::new("btrfs")
            .args(["subvolume", "show", &backup_path.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            return Ok(VerificationResult {
                success: false,
                message: "Backup is not a valid btrfs subvolume".to_string(),
                details: vec![
                    "Expected a btrfs subvolume but verification failed".to_string(),
                ],
            });
        }

        details.push("✓ Backup is a valid btrfs subvolume".to_string());

        // Compare basic metadata
        let backup_info = String::from_utf8_lossy(&output.stdout);
        if backup_info.contains("Snapshot(s):") || backup_info.contains("UUID:") {
            details.push("✓ Subvolume metadata present".to_string());
        }
    } else {
        // For non-btrfs: compare file counts and total size
        details.push(format!("Destination filesystem: non-btrfs"));

        let orig_stats = get_directory_stats(&canonical_snapshot)?;
        let backup_stats = get_directory_stats(&backup_path)?;

        details.push(format!("Original: {} files, {} MB",
            orig_stats.0,
            orig_stats.1 / (1024 * 1024)
        ));
        details.push(format!("Backup: {} files, {} MB",
            backup_stats.0,
            backup_stats.1 / (1024 * 1024)
        ));

        // Allow small differences due to filesystem overhead
        let size_diff_percent = if orig_stats.1 > 0 {
            ((backup_stats.1 as i64 - orig_stats.1 as i64).abs() as f64 / orig_stats.1 as f64) * 100.0
        } else {
            0.0
        };

        if backup_stats.0 != orig_stats.0 {
            return Ok(VerificationResult {
                success: false,
                message: format!("File count mismatch: {} files backed up vs {} original",
                    backup_stats.0, orig_stats.0),
                details,
            });
        }

        if size_diff_percent > 5.0 {
            return Ok(VerificationResult {
                success: false,
                message: format!("Size difference too large: {:.1}%", size_diff_percent),
                details,
            });
        }

        details.push("✓ File counts match".to_string());
        details.push("✓ Sizes are consistent".to_string());
    }

    // Check read access
    match fs::read_dir(&backup_path) {
        Ok(_) => details.push("✓ Backup is readable".to_string()),
        Err(e) => {
            return Ok(VerificationResult {
                success: false,
                message: format!("Cannot read backup: {}", e),
                details,
            });
        }
    }

    Ok(VerificationResult {
        success: true,
        message: "Backup verified successfully".to_string(),
        details,
    })
}

/// Get directory statistics (file count and total size)
fn get_directory_stats(path: &Path) -> Result<(usize, u64)> {
    let output = Command::new("du")
        .args(["-s", "--apparent-size", "--block-size=1"])
        .arg(path)
        .output()?;

    if !output.status.success() {
        bail!("Failed to get directory size");
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let size: u64 = output_str
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Count files
    let output = Command::new("find")
        .arg(path)
        .args(["-type", "f"])
        .output()?;

    let file_count = output.stdout.iter().filter(|&&b| b == b'\n').count();

    Ok((file_count, size))
}
