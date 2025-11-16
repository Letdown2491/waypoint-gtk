// Waypoint Helper - Privileged D-Bus service for snapshot operations
// This binary runs with elevated privileges via D-Bus activation

use anyhow::{Context, Result};
use tokio::signal::unix::{signal, SignalKind};
use waypoint_common::*;
use zbus::{interface, Connection, ConnectionBuilder};

mod backup;
mod btrfs;
mod packages;

/// Get the configured scheduler config path
fn scheduler_config_path() -> String {
    let config = WaypointConfig::new();
    config.scheduler_config.to_string_lossy().to_string()
}

/// Get the configured scheduler service path
fn scheduler_service_path() -> String {
    let config = WaypointConfig::new();
    config.scheduler_service_path().to_string_lossy().to_string()
}

/// Main D-Bus service interface for Waypoint operations
struct WaypointHelper;

#[interface(name = "tech.geektoshi.waypoint.Helper")]
impl WaypointHelper {
    /// Signal emitted when a snapshot is created
    #[zbus(signal)]
    async fn snapshot_created(
        ctxt: &zbus::SignalContext<'_>,
        snapshot_name: &str,
        created_by: &str,
    ) -> zbus::Result<()>;

    /// Create a new snapshot
    async fn create_snapshot(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_context)] ctxt: zbus::SignalContext<'_>,
        name: String,
        description: String,
        subvolumes: Vec<String>,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CREATE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        // Create the snapshot
        match Self::create_snapshot_impl(&name, &description, subvolumes) {
            Ok(msg) => {
                // Emit signal for successful snapshot creation
                // Try to determine who created the snapshot
                let created_by = if hdr.sender().map(|s| s.as_str()).unwrap_or("").contains("waypoint-scheduler") {
                    "scheduler"
                } else {
                    "gui"
                };

                if let Err(e) = Self::snapshot_created(&ctxt, &name, created_by).await {
                    log::error!("Failed to emit snapshot_created signal: {}", e);
                }

                (true, msg)
            },
            Err(e) => (false, format!("Failed to create snapshot: {}", e)),
        }
    }

    /// Delete a snapshot
    async fn delete_snapshot(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        name: String,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_DELETE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        // Delete the snapshot
        match btrfs::delete_snapshot(&name) {
            Ok(_) => (true, format!("Snapshot '{}' deleted successfully", name)),
            Err(e) => (false, format!("Failed to delete snapshot: {}", e)),
        }
    }

    /// Restore a snapshot (rollback system)
    async fn restore_snapshot(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        name: String,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_RESTORE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        // Perform rollback
        match Self::restore_snapshot_impl(&name) {
            Ok(msg) => (true, msg),
            Err(e) => (false, format!("Failed to restore snapshot: {}", e)),
        }
    }

    /// List all snapshots
    async fn list_snapshots(&self) -> String {
        // Listing doesn't require authorization (read-only)
        match btrfs::list_snapshots() {
            Ok(snapshots) => {
                let snapshot_infos: Vec<SnapshotInfo> = snapshots
                    .into_iter()
                    .map(|s| s.into())
                    .collect();

                serde_json::to_string(&snapshot_infos).unwrap_or_else(|_| "[]".to_string())
            }
            Err(e) => {
                log::error!("Failed to list snapshots: {}", e);
                "[]".to_string()
            }
        }
    }

    /// Verify snapshot integrity
    async fn verify_snapshot(&self, name: String) -> String {
        // Verification is read-only, no authorization needed
        match btrfs::verify_snapshot(&name) {
            Ok(result) => {
                serde_json::to_string(&result).unwrap_or_else(|_| {
                    r#"{"is_valid":false,"errors":["Failed to serialize result"],"warnings":[]}"#.to_string()
                })
            }
            Err(e) => {
                log::error!("Failed to verify snapshot: {}", e);
                serde_json::to_string(&btrfs::VerificationResult {
                    is_valid: false,
                    errors: vec![format!("Verification failed: {}", e)],
                    warnings: vec![],
                }).unwrap_or_else(|_| {
                    r#"{"is_valid":false,"errors":["Failed to verify"],"warnings":[]}"#.to_string()
                })
            }
        }
    }

    /// Preview what will happen if a snapshot is restored
    async fn preview_restore(&self, name: String) -> String {
        // Preview is read-only, no authorization needed
        match btrfs::preview_restore(&name) {
            Ok(result) => {
                serde_json::to_string(&result).unwrap_or_else(|_| {
                    r#"{"snapshot_name":"","snapshot_timestamp":"","snapshot_description":null,"current_kernel":null,"snapshot_kernel":null,"affected_subvolumes":[],"packages_to_add":[],"packages_to_remove":[],"packages_to_upgrade":[],"packages_to_downgrade":[],"total_package_changes":0}"#.to_string()
                })
            }
            Err(e) => {
                log::error!("Failed to preview restore: {}", e);
                format!(r#"{{"error":"Failed to preview restore: {}"}}"#, e.to_string().replace('"', "\\\""))
            }
        }
    }

    /// Update scheduler configuration
    async fn update_scheduler_config(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        config_content: String,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CONFIGURE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        // Write configuration file
        match std::fs::write(scheduler_config_path(), config_content) {
            Ok(_) => (true, "Scheduler configuration updated".to_string()),
            Err(e) => (false, format!("Failed to update configuration: {}", e)),
        }
    }

    /// Save schedules TOML configuration file
    async fn save_schedules_config(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        toml_content: String,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CONFIGURE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        let config = WaypointConfig::new();
        let schedules_path = config.schedules_config;

        // Create parent directory if it doesn't exist
        if let Some(parent) = schedules_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return (false, format!("Failed to create config directory: {}", e));
            }
        }

        // Write configuration file
        match std::fs::write(&schedules_path, toml_content) {
            Ok(_) => (true, "Schedules configuration saved".to_string()),
            Err(e) => (false, format!("Failed to save configuration: {}", e)),
        }
    }

    /// Restart scheduler service
    async fn restart_scheduler(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CONFIGURE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        // Restart the service using sv
        match std::process::Command::new("sv")
            .arg("restart")
            .arg("waypoint-scheduler")
            .status()
        {
            Ok(status) if status.success() => {
                (true, "Scheduler service restarted".to_string())
            }
            Ok(_) => (false, "Failed to restart scheduler service".to_string()),
            Err(e) => (false, format!("Failed to execute sv command: {}", e)),
        }
    }

    /// Get scheduler service status
    async fn get_scheduler_status(&self) -> String {
        // No authorization needed for status check (read-only)
        // Note: waypoint-helper runs as root, so sv commands should work

        // First check if service is enabled (linked in service directory)
        let service_enabled = std::path::Path::new(&scheduler_service_path()).exists();

        if !service_enabled {
            return "disabled".to_string();
        }

        // Service is enabled, check if it's running
        match std::process::Command::new("sv")
            .arg("status")
            .arg("waypoint-scheduler")
            .output()
        {
            Ok(output) => {
                let status_str = String::from_utf8_lossy(&output.stdout);
                let stderr_str = String::from_utf8_lossy(&output.stderr);

                // sv status returns "run:" when running, "down:" when stopped
                if status_str.contains("run:") {
                    "running".to_string()
                } else if status_str.contains("down:") || stderr_str.contains("unable to") {
                    "stopped".to_string()
                } else {
                    "unknown".to_string()
                }
            }
            Err(_) => "unknown".to_string(),
        }
    }

    /// Apply retention cleanup based on schedule-based or global retention rules
    async fn cleanup_snapshots(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        schedule_based: bool,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_DELETE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        // Perform cleanup
        match Self::cleanup_snapshots_impl(schedule_based) {
            Ok(msg) => (true, msg),
            Err(e) => (false, format!("Cleanup failed: {}", e)),
        }
    }

    /// Restore files from a snapshot to the filesystem
    ///
    /// Restores individual files or directories from a snapshot back to the live system.
    /// Can restore to original locations or to a custom directory.
    ///
    /// # Arguments
    /// * `snapshot_name` - Name of the snapshot to restore from
    /// * `file_paths` - Paths within the snapshot to restore (e.g., "/etc/fstab", "/home/user/doc.txt")
    /// * `target_directory` - Where to restore files. Empty string = original locations, otherwise custom path
    /// * `overwrite` - Whether to overwrite existing files
    async fn restore_files(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        snapshot_name: String,
        file_paths: Vec<String>,
        target_directory: String,
        overwrite: bool,
    ) -> (bool, String) {
        // Check authorization - file restoration requires restore permissions
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_RESTORE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        // Perform file restoration
        match Self::restore_files_impl(&snapshot_name, file_paths, &target_directory, overwrite) {
            Ok(msg) => (true, msg),
            Err(e) => (false, format!("File restoration failed: {}", e)),
        }
    }

    /// Compare two snapshots and return list of changed files
    ///
    /// Uses btrfs send with --no-data to efficiently detect file changes between snapshots.
    ///
    /// # Arguments
    /// * `old_snapshot_name` - Name of the older snapshot
    /// * `new_snapshot_name` - Name of the newer snapshot
    ///
    /// # Returns
    /// JSON string containing array of changes, each with: type (Added/Modified/Deleted) and path
    ///
    /// This is a read-only operation and does not require authorization
    async fn compare_snapshots(
        &self,
        old_snapshot_name: String,
        new_snapshot_name: String,
    ) -> (bool, String) {
        // Perform comparison
        match Self::compare_snapshots_impl(&old_snapshot_name, &new_snapshot_name) {
            Ok(json) => (true, json),
            Err(e) => (false, format!("Comparison failed: {}", e)),
        }
    }

    /// Enable btrfs quotas on the snapshot filesystem
    ///
    /// # Arguments
    /// * `use_simple` - Whether to use simple quotas (true) or traditional qgroups (false)
    async fn enable_quotas(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        use_simple: bool,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CONFIGURE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        match Self::enable_quotas_impl(use_simple) {
            Ok(msg) => (true, msg),
            Err(e) => (false, format!("Failed to enable quotas: {}", e)),
        }
    }

    /// Disable btrfs quotas on the snapshot filesystem
    async fn disable_quotas(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CONFIGURE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        match Self::disable_quotas_impl() {
            Ok(msg) => (true, msg),
            Err(e) => (false, format!("Failed to disable quotas: {}", e)),
        }
    }

    /// Get quota usage for the snapshot filesystem
    ///
    /// Returns JSON string with quota usage information
    async fn get_quota_usage(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_RESTORE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        match Self::get_quota_usage_impl() {
            Ok(json) => (true, json),
            Err(e) => (false, format!("Failed to get quota usage: {}", e)),
        }
    }

    /// Set quota limit for the snapshot filesystem
    ///
    /// # Arguments
    /// * `limit_bytes` - Size limit in bytes
    async fn set_quota_limit(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        limit_bytes: u64,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CONFIGURE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        match Self::set_quota_limit_impl(limit_bytes) {
            Ok(msg) => (true, msg),
            Err(e) => (false, format!("Failed to set quota limit: {}", e)),
        }
    }

    /// Save quota configuration to /etc/waypoint/quota.toml
    ///
    /// # Arguments
    /// * `config_toml` - TOML string of quota configuration
    async fn save_quota_config(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        config_toml: String,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CONFIGURE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        match Self::save_quota_config_impl(&config_toml) {
            Ok(msg) => (true, msg),
            Err(e) => (false, format!("Failed to save quota configuration: {}", e)),
        }
    }

    /// Scan for available backup destinations
    ///
    /// This is a read-only operation and does not require authorization
    async fn scan_backup_destinations(&self) -> (bool, String) {
        match backup::scan_backup_destinations() {
            Ok(destinations) => {
                match serde_json::to_string(&destinations) {
                    Ok(json) => (true, json),
                    Err(e) => (false, format!("Failed to serialize destinations: {}", e)),
                }
            }
            Err(e) => (false, format!("Failed to scan destinations: {}", e)),
        }
    }

    /// Backup a snapshot to an external drive
    ///
    /// # Arguments
    /// * `snapshot_path` - Full path to the snapshot (e.g., /.snapshots/my-snapshot)
    /// * `destination_mount` - Mount point of backup destination
    /// * `parent_snapshot` - Optional parent snapshot path for incremental backup
    ///
    /// # Returns
    /// * `(success, message_or_path, size_bytes)` - On success: (true, backup_path, size). On failure: (false, error, 0)
    async fn backup_snapshot(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        snapshot_path: String,
        destination_mount: String,
        parent_snapshot: String,
    ) -> (bool, String, u64) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CREATE).await {
            return (false, format!("Authorization failed: {}", e), 0);
        }

        let parent = if parent_snapshot.is_empty() {
            None
        } else {
            Some(parent_snapshot.as_str())
        };

        match backup::backup_snapshot(&snapshot_path, &destination_mount, parent) {
            Ok((backup_path, size_bytes)) => (true, backup_path, size_bytes),
            Err(e) => (false, format!("Failed to backup snapshot: {}", e), 0),
        }
    }

    /// List backups at a destination
    async fn list_backups(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        destination_mount: String,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CREATE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        match backup::list_backups(&destination_mount) {
            Ok(backups) => {
                match serde_json::to_string(&backups) {
                    Ok(json) => (true, json),
                    Err(e) => (false, format!("Failed to serialize backups: {}", e)),
                }
            }
            Err(e) => (false, format!("Failed to list backups: {}", e)),
        }
    }

    /// Restore a snapshot from backup
    async fn restore_from_backup(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        backup_path: String,
        snapshots_dir: String,
    ) -> (bool, String) {
        // Check authorization - use restore action since we're restoring a snapshot
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_RESTORE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        match backup::restore_from_backup(&backup_path, &snapshots_dir) {
            Ok(restored_path) => (true, restored_path),
            Err(e) => (false, format!("Failed to restore from backup: {}", e)),
        }
    }
}

impl WaypointHelper {
    fn create_snapshot_impl(name: &str, description: &str, subvolumes: Vec<String>) -> Result<String> {
        // Check quota and cleanup if needed
        if let Err(e) = Self::check_quota_and_cleanup() {
            log::warn!("Failed to check quota before snapshot: {}", e);
            // Continue anyway - quota check is not critical
        }

        // Get installed packages
        let packages = packages::get_installed_packages()
            .context("Failed to get installed packages")?;

        // Convert String paths to PathBuf
        let subvol_paths: Vec<std::path::PathBuf> = subvolumes
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect();

        // Create btrfs snapshot
        btrfs::create_snapshot(name, Some(description), packages, subvol_paths)
            .context("Failed to create btrfs snapshot")?;

        Ok(format!("Snapshot '{}' created successfully", name))
    }

    fn restore_snapshot_impl(name: &str) -> Result<String> {
        // Create pre-rollback backup (only root filesystem for safety)
        let backup_name = format!("waypoint-pre-rollback-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"));

        let packages = packages::get_installed_packages()
            .context("Failed to get installed packages for backup")?;

        // Backup only root filesystem
        let root_only = vec![std::path::PathBuf::from("/")];
        btrfs::create_snapshot(&backup_name, Some("Pre-rollback backup"), packages, root_only)
            .context("Failed to create pre-rollback backup")?;

        // Perform the rollback
        btrfs::restore_snapshot(name)
            .context("Failed to restore snapshot")?;

        Ok(format!(
            "Snapshot '{}' will be active after reboot. Backup created: '{}'",
            name, backup_name
        ))
    }

    fn cleanup_snapshots_impl(schedule_based: bool) -> Result<String> {
        use waypoint_common::schedules::SchedulesConfig;
        use waypoint_common::WaypointConfig;
        use std::collections::HashSet;

        let config = WaypointConfig::new();
        let snapshots = btrfs::list_snapshots()
            .context("Failed to list snapshots")?;

        // Load snapshot metadata to check for pinned/favorited snapshots
        let favorited_ids: HashSet<String> = {
            if let Ok(content) = std::fs::read_to_string(&config.metadata_file) {
                if let Ok(metadata) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                    metadata.iter()
                        .filter(|m| m.get("is_favorite").and_then(|v| v.as_bool()).unwrap_or(false))
                        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                        .collect()
                } else {
                    HashSet::new()
                }
            } else {
                HashSet::new()
            }
        };

        let to_delete = if schedule_based {
            // Use per-schedule retention from schedules.toml
            let schedules = SchedulesConfig::load_from_file(&config.schedules_config)
                .context("Failed to load schedules configuration")?;

            // Apply schedule-based retention directly
            let mut all_to_delete = Vec::new();

            for schedule in &schedules.schedules {
                if !schedule.enabled {
                    continue;
                }

                let mut matching: Vec<_> = snapshots.iter()
                    .filter(|s| s.name.starts_with(&schedule.prefix))
                    .collect();

                // Sort by timestamp (oldest first)
                matching.sort_by_key(|s| s.timestamp);

                let now = chrono::Utc::now();

                for (idx, snapshot) in matching.iter().enumerate() {
                    let mut should_delete = false;

                    // Apply keep_count
                    if schedule.keep_count > 0 {
                        let position_from_end = matching.len() - idx;
                        if position_from_end > schedule.keep_count as usize {
                            should_delete = true;
                        }
                    }

                    // Apply keep_days
                    if schedule.keep_days > 0 && !should_delete {
                        let age = now.signed_duration_since(snapshot.timestamp);
                        let max_age = chrono::Duration::days(schedule.keep_days as i64);
                        if age > max_age {
                            should_delete = true;
                        }
                    }

                    // Never delete favorited/pinned snapshots
                    if should_delete && !favorited_ids.contains(&snapshot.name) {
                        all_to_delete.push(snapshot.name.clone());
                    }
                }
            }
            all_to_delete
        } else {
            // Use legacy global retention policy (for backward compatibility)
            log::warn!("Global retention policy is deprecated. Consider using --schedule-based flag.");
            vec![] // Legacy mode not implemented in this version
        };

        if to_delete.is_empty() {
            return Ok("No snapshots to clean up".to_string());
        }

        // Delete snapshots
        let mut deleted = 0;
        let mut failed = Vec::new();

        for snapshot_name in &to_delete {
            match btrfs::delete_snapshot(snapshot_name) {
                Ok(_) => {
                    log::info!("Deleted old snapshot: {}", snapshot_name);
                    deleted += 1;
                }
                Err(e) => {
                    log::error!("Failed to delete snapshot '{}': {}", snapshot_name, e);
                    failed.push(snapshot_name.clone());
                }
            }
        }

        if failed.is_empty() {
            Ok(format!("Cleaned up {} snapshot(s)", deleted))
        } else {
            Ok(format!("Cleaned up {} snapshot(s), failed to delete: {:?}", deleted, failed))
        }
    }

    fn restore_files_impl(
        snapshot_name: &str,
        file_paths: Vec<String>,
        target_directory: &str,
        overwrite: bool,
    ) -> Result<String> {
        use std::fs;
        use std::path::{Path, PathBuf};

        let config = WaypointConfig::new();
        let snapshot_dir = config.snapshot_dir.join(snapshot_name).join("root");

        // Validate snapshot exists
        if !snapshot_dir.exists() {
            anyhow::bail!("Snapshot '{}' not found", snapshot_name);
        }

        if file_paths.is_empty() {
            anyhow::bail!("No files specified for restoration");
        }

        let mut restored_count = 0;
        let mut failed_files = Vec::new();
        let use_custom_target = !target_directory.is_empty();

        for file_path in &file_paths {
            // Ensure path starts with /
            let normalized_path = if file_path.starts_with('/') {
                file_path.to_string()
            } else {
                format!("/{}", file_path)
            };

            // Build source path in snapshot
            let source = snapshot_dir.join(normalized_path.trim_start_matches('/'));

            if !source.exists() {
                log::warn!("File not found in snapshot: {}", normalized_path);
                failed_files.push(normalized_path.clone());
                continue;
            }

            // Determine target path
            let target = if use_custom_target {
                // Restore to custom directory, preserving filename
                let filename = source.file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?;
                Path::new(target_directory).join(filename)
            } else {
                // Restore to original location
                PathBuf::from(&normalized_path)
            };

            // Check if target exists
            if target.exists() && !overwrite {
                log::warn!("File exists and overwrite disabled: {}", target.display());
                failed_files.push(normalized_path.clone());
                continue;
            }

            // Create parent directory if needed
            if let Some(parent) = target.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    log::error!("Failed to create parent directory for {}: {}", target.display(), e);
                    failed_files.push(normalized_path.clone());
                    continue;
                }
            }

            // Copy file or directory recursively
            if source.is_dir() {
                if let Err(e) = copy_dir_recursive(&source, &target) {
                    log::error!("Failed to restore directory {}: {}", normalized_path, e);
                    failed_files.push(normalized_path);
                } else {
                    restored_count += 1;
                }
            } else {
                if let Err(e) = fs::copy(&source, &target) {
                    log::error!("Failed to restore file {}: {}", normalized_path, e);
                    failed_files.push(normalized_path);
                } else {
                    // Preserve permissions and ownership
                    if let Err(e) = preserve_metadata(&source, &target) {
                        log::warn!("File restored but failed to preserve metadata for {}: {}", target.display(), e);
                    }
                    restored_count += 1;
                }
            }
        }

        if failed_files.is_empty() {
            Ok(format!("Successfully restored {} file(s) from snapshot '{}'", restored_count, snapshot_name))
        } else {
            Ok(format!(
                "Restored {} file(s), failed to restore {}: {:?}",
                restored_count,
                failed_files.len(),
                failed_files
            ))
        }
    }

    /// Compare two snapshots using btrfs send/receive
    fn compare_snapshots_impl(old_snapshot_name: &str, new_snapshot_name: &str) -> Result<String> {
        use std::io::{BufReader, Read};
        use std::os::unix::process::ExitStatusExt;
        use std::process::{Command, Stdio};

        let config = WaypointConfig::new();
        let old_path = config.snapshot_dir.join(old_snapshot_name).join("root");
        let new_path = config.snapshot_dir.join(new_snapshot_name).join("root");

        // Verify both snapshots exist
        if !old_path.exists() {
            anyhow::bail!("Old snapshot not found: {}", old_path.display());
        }
        if !new_path.exists() {
            anyhow::bail!("New snapshot not found: {}", new_path.display());
        }

        // Run: btrfs send --no-data -p <old> <new> | btrfs receive --dump
        let mut send_cmd = Command::new("btrfs")
            .arg("send")
            .arg("--no-data")
            .arg("-p")
            .arg(&old_path)
            .arg(&new_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn btrfs send")?;

        let send_stdout = send_cmd.stdout.take()
            .context("Failed to capture btrfs send stdout")?;

        let mut receive_cmd = Command::new("btrfs")
            .arg("receive")
            .arg("--dump")
            .stdin(Stdio::from(send_stdout))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn btrfs receive")?;
        let receive_stdout = receive_cmd.stdout.take()
            .context("Failed to capture btrfs receive stdout")?;

        // Drain receive stdout concurrently to prevent pipe deadlocks
        let reader_handle = std::thread::spawn(move || -> Result<String> {
            let mut reader = BufReader::new(receive_stdout);
            let mut output = String::new();
            reader.read_to_string(&mut output)?;
            Ok(output)
        });

        // Wait for receive first to ensure dump output is consumed
        let receive_status = receive_cmd.wait()?;
        if !receive_status.success() {
            anyhow::bail!("btrfs receive --dump failed with status: {}", receive_status);
        }

        let output = reader_handle
            .join()
            .map_err(|_| anyhow::anyhow!("Failed to join receive output reader"))??;

        let send_status = send_cmd.wait()?;
        if !send_status.success() {
            // btrfs send may emit SIGPIPE if receive exits after finishing.
            if send_status.signal() == Some(libc::SIGPIPE) {
                log::debug!("btrfs send exited with SIGPIPE after receive completed");
            } else {
                anyhow::bail!("btrfs send failed with status: {}", send_status);
            }
        }

        // Parse the dump output and convert to JSON
        let changes = parse_btrfs_dump(&output)?;

        // Serialize to JSON
        serde_json::to_string(&changes)
            .context("Failed to serialize changes to JSON")
    }

    /// Enable quotas on the btrfs filesystem
    fn enable_quotas_impl(use_simple: bool) -> Result<String> {
        use std::process::Command;

        let config = WaypointConfig::new();
        let snapshot_dir = &config.snapshot_dir;

        // Check if quotas are already enabled
        let check_output = Command::new("btrfs")
            .arg("qgroup")
            .arg("show")
            .arg(snapshot_dir)
            .output();

        if let Ok(output) = check_output {
            if output.status.success() {
                return Ok("Quotas are already enabled".to_string());
            }
        }

        // Enable quotas
        let mut cmd = Command::new("btrfs");
        cmd.arg("quota").arg("enable");

        if use_simple {
            cmd.arg("--simple");
        }

        cmd.arg(snapshot_dir);

        let output = cmd.output()
            .context("Failed to execute btrfs quota enable")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to enable quotas: {}", stderr);
        }

        let quota_type = if use_simple { "simple" } else { "traditional" };
        Ok(format!("Successfully enabled {} quotas", quota_type))
    }

    /// Disable quotas on the btrfs filesystem
    fn disable_quotas_impl() -> Result<String> {
        use std::process::Command;

        let config = WaypointConfig::new();
        let snapshot_dir = &config.snapshot_dir;

        let output = Command::new("btrfs")
            .arg("quota")
            .arg("disable")
            .arg(snapshot_dir)
            .output()
            .context("Failed to execute btrfs quota disable")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to disable quotas: {}", stderr);
        }

        Ok("Successfully disabled quotas".to_string())
    }

    /// Get quota usage information
    fn get_quota_usage_impl() -> Result<String> {
        use std::process::Command;
        use waypoint_common::{QuotaUsage, QuotaConfig};

        let config = WaypointConfig::new();
        let snapshot_dir = &config.snapshot_dir;

        // Get qgroup information
        let output = Command::new("btrfs")
            .arg("qgroup")
            .arg("show")
            .arg("--raw")
            .arg(snapshot_dir)
            .output()
            .context("Failed to execute btrfs qgroup show")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to get quota usage: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse qgroup output
        // Format: qgroupid rfer excl max_rfer max_excl
        // Sum up all level-0 qgroups (snapshots)
        let mut total_referenced = 0u64;
        let mut total_exclusive = 0u64;

        for line in stdout.lines().skip(2) { // Skip header lines
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() && parts[0].starts_with("0/") {
                // Only count level-0 qgroups (actual snapshots)
                if parts.len() >= 3 {
                    if let (Ok(rfer), Ok(excl)) = (parts[1].parse::<u64>(), parts[2].parse::<u64>()) {
                        total_referenced += rfer;
                        total_exclusive += excl;
                    }
                }
            }
        }

        // Get limit from our config file, not from btrfs
        // (btrfs quotas are per-subvolume, we want total limit)
        let quota_config = QuotaConfig::load().unwrap_or_default();
        let limit = quota_config.total_limit_bytes;

        let usage = QuotaUsage {
            referenced: total_referenced,
            exclusive: total_exclusive,
            limit,
        };

        serde_json::to_string(&usage)
            .context("Failed to serialize quota usage to JSON")
    }

    /// Set quota limit for the filesystem
    ///
    /// Note: The limit is stored in our config file and enforced by cleanup logic.
    /// Btrfs quotas are used for monitoring usage, not enforcing limits (as they are per-subvolume).
    fn set_quota_limit_impl(_limit_bytes: u64) -> Result<String> {
        // The limit is already saved in quota.toml by save_quota_config_impl
        // and enforced by check_quota_and_cleanup during snapshot creation.
        // We don't need to set btrfs qgroup limits since those are per-subvolume,
        // and we want a total limit across all snapshots.
        Ok("Quota limit updated in configuration".to_string())
    }

    /// Check quota usage and cleanup old snapshots if needed
    fn check_quota_and_cleanup() -> Result<()> {
        use waypoint_common::QuotaConfig;

        // Load quota configuration
        let quota_config = QuotaConfig::load()?;

        // Only proceed if quotas are enabled and auto-cleanup is on
        if !quota_config.enabled || !quota_config.auto_cleanup {
            return Ok(());
        }

        // Get current usage
        let usage_json = Self::get_quota_usage_impl()?;
        let usage: waypoint_common::QuotaUsage = serde_json::from_str(&usage_json)?;

        // Check if we exceed the threshold
        if usage.exceeds_threshold(quota_config.cleanup_threshold) {
            log::info!("Quota usage exceeds threshold ({}%), triggering cleanup",
                       quota_config.cleanup_threshold * 100.0);

            // Load snapshots and find oldest ones
            let metadata_path = WaypointConfig::new().metadata_file;
            if !metadata_path.exists() {
                return Ok(());
            }

            let contents = std::fs::read_to_string(&metadata_path)?;
            let mut snapshots: Vec<waypoint_common::SnapshotInfo> =
                serde_json::from_str(&contents)?;

            // Sort by timestamp (oldest first)
            snapshots.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

            // Delete oldest snapshots until we're below threshold
            let target_usage = quota_config.cleanup_threshold * 0.8; // Target 80% of threshold
            let mut deleted_count = 0;

            for snapshot in snapshots.iter() {
                // Re-check usage after each deletion
                let current_usage_json = Self::get_quota_usage_impl()?;
                let current_usage: waypoint_common::QuotaUsage =
                    serde_json::from_str(&current_usage_json)?;

                if let Some(pct) = current_usage.usage_percent() {
                    if pct <= target_usage {
                        break; // We've cleaned up enough
                    }
                }

                // Delete this snapshot
                log::info!("Auto-cleanup: Deleting snapshot '{}'", snapshot.name);
                if let Err(e) = btrfs::delete_snapshot(&snapshot.name) {
                    log::error!("Failed to delete snapshot '{}': {}", snapshot.name, e);
                    continue;
                }

                deleted_count += 1;
            }

            if deleted_count > 0 {
                log::info!("Auto-cleanup: Deleted {} snapshot(s) to free quota space", deleted_count);
            }
        }

        Ok(())
    }

    /// Save quota configuration to file
    fn save_quota_config_impl(config_toml: &str) -> Result<String> {
        use waypoint_common::QuotaConfig;

        // Validate TOML by parsing it
        let _config: QuotaConfig = toml::from_str(config_toml)
            .context("Invalid quota configuration")?;

        let config_path = QuotaConfig::default_path();

        // Create parent directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }

        // Write configuration
        std::fs::write(&config_path, config_toml)
            .context("Failed to write quota configuration file")?;

        Ok("Quota configuration saved successfully".to_string())
    }
}

/// Recursively copy a directory and its contents
fn copy_dir_recursive(source: &std::path::Path, target: &std::path::Path) -> Result<()> {
    use std::fs;

    // Create target directory
    fs::create_dir_all(target)
        .context(format!("Failed to create directory: {}", target.display()))?;

    // Copy metadata
    preserve_metadata(source, target)?;

    // Iterate through directory entries
    for entry in fs::read_dir(source)
        .context(format!("Failed to read directory: {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if source_path.is_dir() {
            // Recursively copy subdirectory
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            // Copy file
            fs::copy(&source_path, &target_path)
                .context(format!("Failed to copy file: {}", source_path.display()))?;
            preserve_metadata(&source_path, &target_path)?;
        }
    }

    Ok(())
}

/// Preserve file metadata (permissions and ownership)
fn preserve_metadata(source: &std::path::Path, target: &std::path::Path) -> Result<()> {
    use std::fs;

    // Get source metadata
    let metadata = fs::metadata(source)
        .context(format!("Failed to read metadata: {}", source.display()))?;

    // Set permissions
    let permissions = metadata.permissions();
    fs::set_permissions(target, permissions)
        .context(format!("Failed to set permissions: {}", target.display()))?;

    // Set ownership (requires root, which waypoint-helper has)
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let uid = metadata.uid();
        let gid = metadata.gid();

        unsafe {
            let target_cstr = std::ffi::CString::new(target.to_string_lossy().as_bytes())
                .context("Failed to convert path to CString")?;

            if libc::chown(target_cstr.as_ptr(), uid, gid) != 0 {
                let err = std::io::Error::last_os_error();
                log::warn!("Failed to set ownership for {}: {}", target.display(), err);
                // Don't fail the whole operation for ownership issues
            }
        }
    }

    Ok(())
}

/// Parse btrfs receive --dump output into structured changes
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct FileChange {
    change_type: String,  // "Added", "Modified", "Deleted"
    path: String,
}

fn parse_btrfs_dump(dump_output: &str) -> Result<Vec<FileChange>> {
    let mut changes = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for line in dump_output.lines() {
        // Parse key=value format
        // Example lines:
        // mkfile path=some/file
        // write path=some/file offset=0 len=1024
        // unlink path=some/file
        // rename from=old/path to=new/path
        // mkdir path=some/dir

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let command = parts[0];
        let mut path = String::new();

        // Extract path from key=value pairs
        for part in &parts[1..] {
            if let Some((key, value)) = part.split_once('=') {
                if key == "path" {
                    path = decode_path(value);
                    break;
                } else if key == "to" && command == "rename" {
                    // For renames, use the destination path
                    path = decode_path(value);
                    break;
                }
            }
        }

        if path.is_empty() {
            continue;
        }

        // Determine change type based on command
        let change_type = match command {
            "mkfile" | "mkdir" | "mksock" | "mkfifo" | "mknod" | "symlink" | "link" => "Added",
            "write" | "clone" | "set_xattr" | "remove_xattr" | "truncate" | "chmod" | "chown" | "utimes" => "Modified",
            "unlink" | "rmdir" => "Deleted",
            "rename" => "Modified",  // Rename could be considered as modified
            _ => continue,  // Unknown command, skip
        };

        // Add absolute path prefix
        let full_path = format!("/{}", path);

        // Avoid duplicates (e.g., mkfile + write for same file)
        let key = format!("{}:{}", change_type, full_path);
        if seen_paths.insert(key) {
            changes.push(FileChange {
                change_type: change_type.to_string(),
                path: full_path,
            });
        }
    }

    // Sort by path for consistent output
    changes.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(changes)
}

/// Decode path from btrfs receive --dump output
/// Paths may contain C-style escape sequences like \n or \NNN (octal)
fn decode_path(encoded: &str) -> String {
    let mut result = String::new();
    let mut chars = encoded.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Check for escape sequence
            if let Some(&next_ch) = chars.peek() {
                match next_ch {
                    'n' => {
                        chars.next();
                        result.push('\n');
                    }
                    't' => {
                        chars.next();
                        result.push('\t');
                    }
                    'r' => {
                        chars.next();
                        result.push('\r');
                    }
                    '\\' => {
                        chars.next();
                        result.push('\\');
                    }
                    '0'..='7' => {
                        // Octal escape sequence \NNN
                        let mut octal = String::new();
                        for _ in 0..3 {
                            if let Some(&digit @ '0'..='7') = chars.peek() {
                                octal.push(digit);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        if let Ok(code) = u8::from_str_radix(&octal, 8) {
                            result.push(code as char);
                        } else {
                            // Invalid octal, keep as-is
                            result.push('\\');
                            result.push_str(&octal);
                        }
                    }
                    _ => {
                        // Unknown escape, keep backslash
                        result.push(ch);
                    }
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}


/// Check Polkit authorization for an action
///
/// Calls org.freedesktop.PolicyKit1.Authority.CheckAuthorization to verify
/// the caller has permission to perform the requested action.
async fn check_authorization(
    hdr: &zbus::message::Header<'_>,
    connection: &Connection,
    action_id: &str,
) -> Result<()> {
    use zbus::zvariant::{ObjectPath, Value};
    use std::collections::HashMap;

    log::debug!("Authorization requested for action: {}", action_id);

    // Get the caller's bus name from the message header
    let caller = hdr.sender()
        .context("No sender in message header")?
        .to_owned();

    log::debug!("Caller bus name: {}", caller);

    // Get the caller's PID from D-Bus
    let response = connection.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "GetConnectionUnixProcessID",
        &caller.as_str(),
    ).await
        .context("Failed to get caller PID from D-Bus")?;

    let caller_pid: u32 = response.body().deserialize()
        .context("Failed to deserialize caller PID")?;

    log::debug!("Caller PID: {}", caller_pid);

    // Get process start time from /proc
    let start_time = get_process_start_time(caller_pid)?;

    // Build the subject structure for Polkit
    // Subject is (subject_kind, subject_details)
    let mut subject_details: HashMap<String, Value> = HashMap::new();
    subject_details.insert("pid".to_string(), Value::U32(caller_pid));
    subject_details.insert("start-time".to_string(), Value::U64(start_time));

    let subject = ("unix-process", subject_details);

    // Details dict (empty for now)
    let details: HashMap<String, String> = HashMap::new();

    // Flags: 1 = AllowUserInteraction (show password prompt if needed)
    let flags: u32 = 1;

    // Cancellation ID (empty string = no cancellation)
    let cancellation_id = "";

    // Call Polkit CheckAuthorization
    let polkit_path = ObjectPath::try_from("/org/freedesktop/PolicyKit1/Authority")
        .context("Invalid Polkit object path")?;

    let result = connection.call_method(
        Some("org.freedesktop.PolicyKit1"),
        polkit_path,
        Some("org.freedesktop.PolicyKit1.Authority"),
        "CheckAuthorization",
        &(subject, action_id, details, flags, cancellation_id),
    ).await;

    let msg = result
        .context("Failed to call Polkit CheckAuthorization")?;

    // Result is (is_authorized, is_challenge, details)
    let (is_authorized, is_challenge, auth_details): (bool, bool, HashMap<String, String>) =
        msg.body().deserialize()
            .context("Failed to deserialize Polkit response")?;

    log::debug!("Authorization result: authorized={}, challenge={}, details={:?}",
             is_authorized, is_challenge, auth_details);

    if is_authorized {
        Ok(())
    } else {
        anyhow::bail!("Action '{}' not authorized", action_id);
    }
}

/// Get process start time from `/proc/[pid]/stat`
fn get_process_start_time(pid: u32) -> Result<u64> {
    use std::fs;

    let stat_path = format!("/proc/{}/stat", pid);
    let stat_content = fs::read_to_string(&stat_path)
        .context(format!("Failed to read {}", stat_path))?;

    // The start time is the 22nd field in /proc/[pid]/stat
    // Fields are: pid (comm) state ppid ... starttime ...
    // We need to handle the (comm) field which may contain spaces and special characters

    // Find the last ')' to skip the comm field
    let start_pos = stat_content.rfind(')')
        .context("Invalid /proc/[pid]/stat format: missing closing parenthesis")?;

    // Ensure there's content after the ')' character
    if start_pos + 1 >= stat_content.len() {
        anyhow::bail!("Invalid /proc/[pid]/stat format: no fields after command name");
    }

    let fields: Vec<&str> = stat_content[start_pos + 1..]
        .split_whitespace()
        .collect();

    // After skipping (comm), starttime is field 20 (0-indexed 19)
    // According to proc(5) man page, there should be at least 44 fields in modern kernels
    const MIN_REQUIRED_FIELDS: usize = 20;
    if fields.len() < MIN_REQUIRED_FIELDS {
        anyhow::bail!(
            "Not enough fields in /proc/{}/stat (expected at least {}, got {})",
            pid,
            MIN_REQUIRED_FIELDS,
            fields.len()
        );
    }

    let start_time_str = fields[19];
    let start_time: u64 = start_time_str.parse()
        .context(format!(
            "Failed to parse process start time from field '{}' (field 20)",
            start_time_str
        ))?;

    log::debug!("Process {} start time: {}", pid, start_time);

    Ok(start_time)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Must run as root
    if unsafe { libc::geteuid() } != 0 {
        log::error!("waypoint-helper must be run as root");
        std::process::exit(1);
    }

    // Initialize configuration
    btrfs::init_config();

    log::info!("Starting Waypoint Helper service v{}", env!("CARGO_PKG_VERSION"));

    // Build the D-Bus connection
    let _connection = ConnectionBuilder::system()?
        .name(DBUS_SERVICE_NAME)?
        .serve_at(DBUS_OBJECT_PATH, WaypointHelper)?
        .build()
        .await?;

    log::info!("Waypoint Helper is ready at {}", DBUS_OBJECT_PATH);

    // Wait for termination signal
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    tokio::select! {
        _ = sigterm.recv() => log::info!("Received SIGTERM, shutting down..."),
        _ = sigint.recv() => log::info!("Received SIGINT, shutting down..."),
    }

    Ok(())
}
