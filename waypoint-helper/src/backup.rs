//! Backup operations for waypoint-helper
//! This module handles btrfs send/receive operations with root privileges

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

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

        // Detect drive type and filesystem
        let source = entry.source.as_deref().unwrap_or("");
        let fstype = entry.fstype;
        let drive_type = detect_drive_type(&mount_point, source, &fstype);

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
) -> Result<(String, u64)> {
    let snapshot = Path::new(snapshot_path);
    let dest_mount = Path::new(destination_mount);

    // Validate inputs
    if !snapshot.exists() {
        return Err(anyhow::anyhow!(
            "Snapshot does not exist: {}",
            snapshot_path
        ));
    }

    if !dest_mount.exists() {
        return Err(anyhow::anyhow!(
            "Destination does not exist: {}",
            destination_mount
        ));
    }

    // Detect destination filesystem type
    let fstype = detect_filesystem_type(destination_mount)?;

    // Route to appropriate backup method
    if fstype == "btrfs" {
        backup_snapshot_btrfs(snapshot_path, destination_mount, parent_snapshot)
    } else {
        backup_snapshot_rsync(snapshot_path, destination_mount)
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

/// Backup a snapshot to a btrfs destination using btrfs send/receive
///
/// Returns a tuple of (backup_path, size_bytes)
fn backup_snapshot_btrfs(
    snapshot_path: &str,
    destination_mount: &str,
    parent_snapshot: Option<&str>,
) -> Result<(String, u64)> {
    let snapshot = Path::new(snapshot_path);
    let dest_mount = Path::new(destination_mount);

    // Create waypoint-backups directory at destination
    let backup_dir = dest_mount.join("waypoint-backups");
    if !backup_dir.exists() {
        std::fs::create_dir_all(&backup_dir).context("Failed to create backup directory")?;
    }

    // Build btrfs send command
    let mut send_cmd = Command::new("btrfs");
    send_cmd.arg("send");

    // Add parent if this is incremental
    if let Some(parent) = parent_snapshot {
        let parent_path = Path::new(parent);
        if !parent_path.exists() {
            return Err(anyhow::anyhow!(
                "Parent snapshot does not exist: {}",
                parent
            ));
        }
        send_cmd.arg("-p").arg(parent);
    }

    send_cmd.arg(snapshot);
    send_cmd.stdout(std::process::Stdio::piped());
    send_cmd.stderr(std::process::Stdio::piped());

    // Build btrfs receive command
    let mut receive_cmd = Command::new("btrfs");
    receive_cmd.arg("receive").arg(&backup_dir);

    // Execute send | receive pipeline
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

    // Return the backup path and size
    let snapshot_name = snapshot
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid snapshot path"))?;

    let backup_path = backup_dir.join(snapshot_name);

    // Calculate backup size using du
    let size_bytes = calculate_directory_size(&backup_path)?;

    Ok((backup_path.to_string_lossy().to_string(), size_bytes))
}

/// Backup a snapshot to a non-btrfs destination using rsync
///
/// Returns a tuple of (backup_path, size_bytes)
fn backup_snapshot_rsync(
    snapshot_path: &str,
    destination_mount: &str,
) -> Result<(String, u64)> {
    let snapshot = Path::new(snapshot_path);
    let dest_mount = Path::new(destination_mount);

    // Create waypoint-backups directory at destination
    let backup_dir = dest_mount.join("waypoint-backups");
    if !backup_dir.exists() {
        std::fs::create_dir_all(&backup_dir).context("Failed to create backup directory")?;
    }

    // Get snapshot name
    let snapshot_name = snapshot
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid snapshot path"))?;

    let backup_path = backup_dir.join(snapshot_name);

    // The snapshot contains a "root" subdirectory with the actual filesystem
    let snapshot_root = snapshot.join("root");
    if !snapshot_root.exists() {
        return Err(anyhow::anyhow!("Snapshot root directory not found"));
    }

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
        .arg(format!("{}/", snapshot_root.display())) // Trailing slash = copy contents
        .arg(&backup_path)
        .output()
        .context("Failed to run rsync")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("rsync failed: {}", stderr));
    }

    // Calculate backup size using du
    let size_bytes = calculate_directory_size(&backup_path)?;

    Ok((backup_path.to_string_lossy().to_string(), size_bytes))
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
pub fn restore_from_backup(backup_path: &str, snapshots_dir: &str) -> Result<String> {
    let backup = Path::new(backup_path);
    let dest = Path::new(snapshots_dir);

    if !backup.is_absolute() || !dest.is_absolute() {
        return Err(anyhow::anyhow!("Paths must be absolute"));
    }

    let backup = backup
        .canonicalize()
        .context("Failed to resolve backup path")?;
    let dest = dest
        .canonicalize()
        .context("Failed to resolve snapshots directory")?;

    if !backup.exists() {
        return Err(anyhow::anyhow!(
            "Backup does not exist: {}",
            backup.display()
        ));
    }

    if !dest.exists() {
        return Err(anyhow::anyhow!(
            "Snapshots directory does not exist: {}",
            dest.display()
        ));
    }

    // Detect if backup is a btrfs subvolume or rsync directory
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
