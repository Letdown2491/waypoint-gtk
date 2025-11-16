//! D-Bus client for communicating with waypoint-helper privileged service
//!
//! This module provides a safe, blocking interface to the waypoint-helper D-Bus service,
//! which runs with elevated privileges to perform snapshot operations.
//!
//! # Architecture
//! - GUI application (unprivileged) ↔ D-Bus IPC ↔ waypoint-helper (privileged)
//! - All operations require Polkit authorization
//! - Operations are blocking and should be run in background threads for UI responsiveness
//!
//! # Example
//! ```no_run
//! use waypoint::dbus_client::WaypointHelperClient;
//!
//! let client = WaypointHelperClient::new()?;
//! let (success, msg) = client.create_snapshot(
//!     "backup-2025".to_string(),
//!     "Before upgrade".to_string(),
//!     vec!["/".to_string()]
//! )?;
//! # Ok::<(), anyhow::Error>(())
//! ```

use anyhow::{Context, Result};
use waypoint_common::*;
use zbus::blocking::Connection as BlockingConnection;

/// Result of snapshot integrity verification
///
/// Contains validation status and any errors or warnings found during verification.
/// A snapshot is considered valid only if `is_valid` is true and `errors` is empty.
#[derive(Debug, serde::Deserialize)]
pub struct VerificationResult {
    /// Whether the snapshot passed all validation checks
    pub is_valid: bool,
    /// Critical errors that make the snapshot invalid (e.g., missing subvolumes)
    pub errors: Vec<String>,
    /// Non-critical issues that don't affect validity (e.g., missing metadata)
    pub warnings: Vec<String>,
}

/// Information about a single package change during restore
///
/// Represents the difference between the current system state and the snapshot state
/// for a single package.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PackageChange {
    /// Package name
    pub name: String,
    /// Currently installed version (None if not installed)
    pub current_version: Option<String>,
    /// Version in the snapshot (None if not present in snapshot)
    pub snapshot_version: Option<String>,
    /// Change type is redundant since packages are categorized into separate vectors,
    /// but kept for JSON schema consistency
    #[allow(dead_code)]
    pub change_type: String,
}

/// Preview of system changes that will occur during snapshot restore
///
/// Provides a comprehensive summary of what will change if a restore operation proceeds,
/// including package changes, kernel changes, and affected subvolumes.
///
/// This allows users to review changes before committing to a restore operation.
#[derive(Debug, serde::Deserialize)]
pub struct RestorePreview {
    /// Name of the snapshot being restored
    pub snapshot_name: String,
    /// When the snapshot was created (formatted string)
    pub snapshot_timestamp: String,
    /// Optional description provided when snapshot was created
    pub snapshot_description: Option<String>,
    /// Currently running kernel version
    pub current_kernel: Option<String>,
    /// Kernel version from the snapshot
    pub snapshot_kernel: Option<String>,
    /// List of subvolumes that will be affected by the restore
    pub affected_subvolumes: Vec<String>,
    /// Packages that will be installed (present in snapshot but not in current system)
    pub packages_to_add: Vec<PackageChange>,
    /// Packages that will be removed (present in current system but not in snapshot)
    pub packages_to_remove: Vec<PackageChange>,
    /// Packages that will be upgraded (newer version in snapshot)
    pub packages_to_upgrade: Vec<PackageChange>,
    /// Packages that will be downgraded (older version in snapshot)
    pub packages_to_downgrade: Vec<PackageChange>,
    /// Total number of package changes across all categories
    pub total_package_changes: usize,
}

/// Blocking D-Bus client for waypoint-helper privileged service
///
/// Provides methods to create, delete, restore, and verify btrfs snapshots through
/// the waypoint-helper D-Bus service. All operations require Polkit authorization.
///
/// # Thread Safety
/// This client uses blocking I/O and should be used from background threads when
/// called from GUI code to avoid blocking the UI.
///
/// # Connection
/// Connects to the system D-Bus bus. The waypoint-helper service must be running
/// (typically activated automatically via D-Bus service activation).
pub struct WaypointHelperClient {
    connection: BlockingConnection,
}

impl WaypointHelperClient {
    /// Connect to the waypoint-helper D-Bus service
    ///
    /// Establishes a connection to the system D-Bus bus and prepares to communicate
    /// with the waypoint-helper service.
    ///
    /// # Errors
    /// - D-Bus system bus connection failure (check if dbus-daemon is running)
    ///
    /// # Example
    /// ```no_run
    /// use waypoint::dbus_client::WaypointHelperClient;
    ///
    /// let client = WaypointHelperClient::new()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new() -> Result<Self> {
        let connection = BlockingConnection::system().context("Failed to connect to system bus")?;

        Ok(Self { connection })
    }

    /// Create a new snapshot of specified subvolumes
    ///
    /// Creates read-only btrfs snapshots of the specified subvolumes. The snapshot
    /// is stored with metadata including timestamp, description, and package list.
    ///
    /// # Arguments
    /// * `name` - Snapshot name (must be filesystem-safe, validated for security)
    /// * `description` - Human-readable description (can be empty string)
    /// * `subvolumes` - Mount points to snapshot (e.g., `vec!["/".to_string()]`)
    ///                  Empty vec defaults to root filesystem only
    ///
    /// # Returns
    /// * `Ok((true, msg))` - Snapshot created successfully, `msg` contains confirmation
    /// * `Ok((false, msg))` - Operation failed, `msg` contains error details
    /// * `Err(_)` - D-Bus communication error
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Polkit authorization denied (requires admin privileges)
    /// - Invalid snapshot name (path traversal, special characters)
    /// - Insufficient disk space
    /// - Btrfs command execution failure
    ///
    /// # Security
    /// Requires root privileges via Polkit authentication. User will be prompted
    /// for administrator password.
    ///
    /// # Example
    /// ```no_run
    /// # use waypoint::dbus_client::WaypointHelperClient;
    /// let client = WaypointHelperClient::new()?;
    /// let (success, msg) = client.create_snapshot(
    ///     "pre-upgrade-2025".to_string(),
    ///     "Before system upgrade".to_string(),
    ///     vec!["/".to_string()]
    /// )?;
    /// if success {
    ///     println!("Created: {}", msg);
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn create_snapshot(
        &self,
        name: String,
        description: String,
        subvolumes: Vec<String>,
    ) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("CreateSnapshot", &(name, description, subvolumes))
            .context("Failed to call CreateSnapshot")?;

        Ok(result)
    }

    /// Delete a snapshot permanently
    ///
    /// Removes the specified snapshot and all its btrfs subvolumes. This operation
    /// cannot be undone.
    ///
    /// # Arguments
    /// * `name` - Snapshot name (directory name on disk, not the display name)
    ///
    /// # Returns
    /// * `Ok((true, msg))` - Snapshot deleted successfully
    /// * `Ok((false, msg))` - Deletion failed, `msg` contains error details
    /// * `Err(_)` - D-Bus communication error
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Polkit authorization denied
    /// - Snapshot not found
    /// - Btrfs subvolume deletion failure (snapshot may be in use)
    ///
    /// # Security
    /// Requires root privileges via Polkit authentication.
    ///
    /// # Warning
    /// This operation is irreversible. The snapshot and all its data will be
    /// permanently removed from the filesystem.
    pub fn delete_snapshot(&self, name: String) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("DeleteSnapshot", &(name,))
            .context("Failed to call DeleteSnapshot")?;

        Ok(result)
    }

    /// Restore system to a previous snapshot state (rollback)
    ///
    /// Performs a system rollback by making the specified snapshot the active root
    /// filesystem. The system will boot into the snapshot state after reboot.
    ///
    /// # Arguments
    /// * `name` - Snapshot name to restore
    ///
    /// # Returns
    /// * `Ok((true, msg))` - Restore configured successfully, reboot required
    /// * `Ok((false, msg))` - Restore failed, `msg` contains error details
    /// * `Err(_)` - D-Bus communication error
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Polkit authorization denied
    /// - Snapshot not found
    /// - Bootloader configuration failure
    /// - Fstab update failure
    ///
    /// # Security
    /// Requires root privileges via Polkit authentication.
    ///
    /// # Important
    /// - Creates a backup snapshot before restoring
    /// - System **MUST** be rebooted for changes to take effect
    /// - All changes after the snapshot was created will be **LOST**
    /// - Package states will be reverted to snapshot state
    /// - Kernel version may change
    ///
    /// # Example
    /// ```no_run
    /// # use waypoint::dbus_client::WaypointHelperClient;
    /// let client = WaypointHelperClient::new()?;
    /// let (success, msg) = client.restore_snapshot("backup-2025".to_string())?;
    /// if success {
    ///     println!("{}", msg);
    ///     // User should reboot now
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn restore_snapshot(&self, name: String) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("RestoreSnapshot", &(name,))
            .context("Failed to call RestoreSnapshot")?;

        Ok(result)
    }

    /// List all snapshots
    #[allow(dead_code)]
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let json: String = proxy
            .call("ListSnapshots", &())
            .context("Failed to call ListSnapshots")?;

        let snapshots: Vec<SnapshotInfo> =
            serde_json::from_str(&json).context("Failed to parse snapshot list")?;

        Ok(snapshots)
    }

    /// Get sizes for multiple snapshots via privileged helper
    ///
    /// This method uses the D-Bus helper which runs with privileges,
    /// so it can access snapshot directories without password prompts.
    /// Uses parallel processing for efficiency.
    ///
    /// # Arguments
    /// * `snapshot_names` - Vec of snapshot names to get sizes for
    ///
    /// # Returns
    /// HashMap mapping snapshot names to sizes in bytes
    pub fn get_snapshot_sizes(
        &self,
        snapshot_names: Vec<String>,
    ) -> Result<std::collections::HashMap<String, u64>> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let json: String = proxy
            .call("GetSnapshotSizes", &(snapshot_names,))
            .context("Failed to call GetSnapshotSizes")?;

        let sizes: std::collections::HashMap<String, u64> =
            serde_json::from_str(&json).context("Failed to parse snapshot sizes")?;

        Ok(sizes)
    }

    /// Verify snapshot integrity and consistency
    ///
    /// Checks if a snapshot is valid by verifying:
    /// - Snapshot directory exists
    /// - All expected subvolumes are present
    /// - Each subvolume is a valid btrfs subvolume
    /// - Metadata is consistent (if available)
    ///
    /// # Arguments
    /// * `name` - Snapshot name to verify
    ///
    /// # Returns
    /// `VerificationResult` containing validation status, errors, and warnings
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - JSON parsing error
    ///
    /// # Note
    /// This is a read-only operation and does not require authentication.
    /// Older snapshots may show warnings about missing metadata, which is normal.
    pub fn verify_snapshot(&self, name: String) -> Result<VerificationResult> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let json: String = proxy
            .call("VerifySnapshot", &(name,))
            .context("Failed to call VerifySnapshot")?;

        let result: VerificationResult =
            serde_json::from_str(&json).context("Failed to parse verification result")?;

        Ok(result)
    }

    /// Preview system changes before restoring a snapshot
    ///
    /// Analyzes the differences between the current system state and the snapshot
    /// to show what will change if the restore proceeds. This includes package
    /// changes, kernel changes, and affected subvolumes.
    ///
    /// # Arguments
    /// * `name` - Snapshot name to preview
    ///
    /// # Returns
    /// `RestorePreview` containing detailed change information
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Snapshot not found
    /// - Package list comparison failure
    /// - JSON parsing error
    ///
    /// # Security
    /// Requires restore authorization via Polkit before data is returned.
    pub fn preview_restore(&self, name: String) -> Result<RestorePreview> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("PreviewRestore", &(name,))
            .context("Failed to call PreviewRestore")?;

        if !result.0 {
            anyhow::bail!(result.1);
        }

        let preview: RestorePreview =
            serde_json::from_str(&result.1).context("Failed to parse restore preview result")?;

        Ok(preview)
    }

    /// Save schedules TOML configuration file
    ///
    /// Writes the schedules configuration in TOML format to the system config directory.
    /// This requires elevated privileges.
    ///
    /// # Arguments
    /// * `toml_content` - Complete schedules configuration in TOML format
    ///
    /// # Returns
    /// * `Ok((true, msg))` - Configuration saved successfully
    /// * `Ok((false, msg))` - Save failed, `msg` contains error details
    /// * `Err(_)` - D-Bus communication error
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Polkit authorization denied
    /// - Invalid TOML configuration
    /// - File write permission error
    ///
    /// # Security
    /// Requires root privileges via Polkit authentication.
    pub fn save_schedules_config(&self, toml_content: String) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("SaveSchedulesConfig", &(toml_content,))
            .context("Failed to call SaveSchedulesConfig")?;

        Ok(result)
    }

    /// Restart the snapshot scheduler service
    ///
    /// Restarts the runit service that runs scheduled snapshots. Call this after
    /// updating scheduler configuration to apply changes.
    ///
    /// # Returns
    /// * `Ok((true, msg))` - Service restarted successfully
    /// * `Ok((false, msg))` - Restart failed, `msg` contains error details
    /// * `Err(_)` - D-Bus communication error
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Polkit authorization denied
    /// - Service control command failure
    ///
    /// # Security
    /// Requires root privileges via Polkit authentication.
    pub fn restart_scheduler(&self) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("RestartScheduler", &())
            .context("Failed to call RestartScheduler")?;

        Ok(result)
    }

    /// Get current status of the snapshot scheduler service
    ///
    /// Queries the runit service manager for the current state of the
    /// waypoint-snapshots service.
    ///
    /// # Returns
    /// Service status string (e.g., "run", "down", "finish")
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Service status query failure
    ///
    /// # Note
    /// This is a read-only operation and does not require authentication.
    pub fn get_scheduler_status(&self) -> Result<String> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let status: String = proxy
            .call("GetSchedulerStatus", &())
            .context("Failed to call GetSchedulerStatus")?;

        Ok(status)
    }

    /// Clean up old snapshots based on retention policies
    ///
    /// Applies retention policies to delete old snapshots. Can use either
    /// per-schedule retention (recommended) or legacy global retention.
    ///
    /// # Arguments
    /// * `schedule_based` - If true, use per-schedule retention from schedules.toml.
    ///                      If false, use legacy global retention policy.
    ///
    /// # Returns
    /// * `Ok((true, msg))` - Cleanup completed successfully
    /// * `Ok((false, msg))` - Cleanup failed, `msg` contains error details
    /// * `Err(_)` - D-Bus communication error
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Polkit authorization denied
    /// - Snapshot deletion failure
    ///
    /// # Security
    /// Requires root privileges via Polkit authentication.
    ///
    /// # Recommended Usage
    /// Always use `schedule_based = true` for per-schedule retention policies.
    pub fn cleanup_snapshots(&self, schedule_based: bool) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("CleanupSnapshots", &(schedule_based,))
            .context("Failed to call CleanupSnapshots")?;

        Ok(result)
    }

    /// Restore files from a snapshot to the filesystem
    ///
    /// Restores individual files or directories from a snapshot back to the live system.
    /// Can restore to original locations or to a custom directory.
    ///
    /// # Arguments
    /// * `snapshot_name` - Name of the snapshot to restore from
    /// * `file_paths` - Paths within the snapshot to restore (e.g., vec!["/etc/fstab".to_string()])
    /// * `target_directory` - Where to restore files. Empty string means original locations
    /// * `overwrite` - Whether to overwrite existing files
    ///
    /// # Returns
    /// * `Ok((true, msg))` - Files restored successfully
    /// * `Ok((false, msg))` - Restoration failed, `msg` contains error details
    /// * `Err(_)` - D-Bus communication error
    ///
    /// # Errors
    /// - D-Bus connection failure
    /// - Polkit authorization denied
    /// - Snapshot not found
    /// - File not found in snapshot
    /// - Permission error during file copy
    ///
    /// # Security
    /// Requires root privileges via Polkit authentication.
    ///
    /// # Example
    /// ```no_run
    /// # use waypoint::dbus_client::WaypointHelperClient;
    /// let client = WaypointHelperClient::new()?;
    /// // Restore /etc/fstab to its original location
    /// let (success, msg) = client.restore_files(
    ///     "backup-2025".to_string(),
    ///     vec!["/etc/fstab".to_string()],
    ///     "".to_string(),  // empty = original location
    ///     true  // overwrite existing file
    /// )?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn restore_files(
        &self,
        snapshot_name: String,
        file_paths: Vec<String>,
        target_directory: String,
        overwrite: bool,
    ) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call(
                "RestoreFiles",
                &(snapshot_name, file_paths, target_directory, overwrite),
            )
            .context("Failed to call RestoreFiles")?;

        Ok(result)
    }

    /// Compare two snapshots and get list of changed files
    ///
    /// Returns JSON string containing array of changes
    ///
    /// **Limitation**: Due to a 25-second D-Bus timeout in zbus 4.0, this operation
    /// will fail for large snapshots that take longer than 25 seconds to compare.
    /// This is a known limitation. For very large snapshots, use package comparison instead.
    ///
    /// **Note**: Currently unused in favor of local package comparison.
    /// Kept for potential future use with file-level comparisons.
    #[allow(dead_code)]
    pub fn compare_snapshots(
        &self,
        old_snapshot_name: String,
        new_snapshot_name: String,
    ) -> Result<String> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("CompareSnapshots", &(old_snapshot_name, new_snapshot_name))
            .context("Failed to call CompareSnapshots")?;

        if !result.0 {
            anyhow::bail!(result.1);
        }

        Ok(result.1)
    }

    /// Enable btrfs quotas on the snapshot filesystem
    pub fn enable_quotas(&self, use_simple: bool) -> Result<String> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("EnableQuotas", &(use_simple,))
            .context("Failed to call EnableQuotas")?;

        if !result.0 {
            anyhow::bail!(result.1);
        }

        Ok(result.1)
    }

    /// Disable btrfs quotas on the snapshot filesystem
    pub fn disable_quotas(&self) -> Result<String> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("DisableQuotas", &())
            .context("Failed to call DisableQuotas")?;

        if !result.0 {
            anyhow::bail!(result.1);
        }

        Ok(result.1)
    }

    /// Get quota usage information
    pub fn get_quota_usage(&self) -> Result<waypoint_common::QuotaUsage> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("GetQuotaUsage", &())
            .context("Failed to call GetQuotaUsage")?;

        if !result.0 {
            anyhow::bail!(result.1);
        }

        let usage: waypoint_common::QuotaUsage = serde_json::from_str(&result.1)?;
        Ok(usage)
    }

    /// Set quota limit in bytes
    pub fn set_quota_limit(&self, limit_bytes: u64) -> Result<String> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("SetQuotaLimit", &(limit_bytes,))
            .context("Failed to call SetQuotaLimit")?;

        if !result.0 {
            anyhow::bail!(result.1);
        }

        Ok(result.1)
    }

    /// Save quota configuration via D-Bus helper
    pub fn save_quota_config(&self, config_toml: String) -> Result<String> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("SaveQuotaConfig", &(config_toml,))
            .context("Failed to call SaveQuotaConfig")?;

        if !result.0 {
            anyhow::bail!(result.1);
        }

        Ok(result.1)
    }

    /// Scan for available backup destinations
    pub fn scan_backup_destinations(&self) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("ScanBackupDestinations", &())
            .context("Failed to call ScanBackupDestinations")?;

        Ok(result)
    }

    /// Backup a snapshot to an external drive
    ///
    /// Returns (success, path_or_error, size_bytes)
    pub fn backup_snapshot(
        &self,
        snapshot_path: String,
        destination_mount: String,
        parent_snapshot: String,
    ) -> Result<(bool, String, u64)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String, u64) = proxy
            .call(
                "BackupSnapshot",
                &(snapshot_path, destination_mount, parent_snapshot),
            )
            .context("Failed to call BackupSnapshot")?;

        Ok(result)
    }

    /// List backups at a destination
    pub fn list_backups(&self, destination_mount: String) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("ListBackups", &(destination_mount,))
            .context("Failed to call ListBackups")?;

        Ok(result)
    }

    /// Restore a snapshot from backup
    #[allow(dead_code)]
    pub fn restore_from_backup(
        &self,
        backup_path: String,
        snapshots_dir: String,
    ) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("RestoreFromBackup", &(backup_path, snapshots_dir))
            .context("Failed to call RestoreFromBackup")?;

        Ok(result)
    }
}
