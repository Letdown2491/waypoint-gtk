// Waypoint Helper - Privileged D-Bus service for snapshot operations
// This binary runs with elevated privileges via D-Bus activation

use anyhow::{Context, Result};
use serde::Deserialize;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::process::Command;
use tokio::signal::unix::{SignalKind, signal};
use waypoint_common::*;
use zbus::{Connection, ConnectionBuilder, interface};

mod backup;
mod btrfs;
mod packages;

/// Structured audit logging for security events
mod audit {
    use chrono::Utc;

    /// Audit log entry for security-relevant events
    #[derive(Debug, serde::Serialize)]
    struct AuditEvent {
        timestamp: String,
        user_id: String,
        user_name: Option<String>,
        process_id: u32,
        operation: String,
        resource: String,
        result: String,
        details: Option<String>,
    }

    impl AuditEvent {
        fn new(
            user_id: String,
            process_id: u32,
            operation: &str,
            resource: &str,
            result: &str,
        ) -> Self {
            // Try to get username from UID
            let user_name = get_username_from_uid(&user_id);

            Self {
                timestamp: Utc::now().to_rfc3339(),
                user_id,
                user_name,
                process_id,
                operation: operation.to_string(),
                resource: resource.to_string(),
                result: result.to_string(),
                details: None,
            }
        }

        fn with_details(mut self, details: String) -> Self {
            self.details = Some(details);
            self
        }

        /// Log the audit event as structured JSON
        fn log(&self) {
            // Log as JSON for easy parsing by audit tools
            if let Ok(json) = serde_json::to_string(self) {
                log::info!(target: "audit", "{}", json);
            } else {
                // Fallback to unstructured if serialization fails
                log::info!(
                    target: "audit",
                    "user={} pid={} operation={} resource={} result={}",
                    self.user_id,
                    self.process_id,
                    self.operation,
                    self.resource,
                    self.result
                );
            }
        }
    }

    /// Get username from UID (best effort)
    fn get_username_from_uid(uid_str: &str) -> Option<String> {
        use std::process::Command;

        let output = Command::new("id")
            .arg("-un")
            .arg(uid_str)
            .output()
            .ok()?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    }

    /// Log a snapshot creation event
    pub fn log_snapshot_create(
        user_id: String,
        process_id: u32,
        snapshot_name: &str,
        success: bool,
        error: Option<&str>,
    ) {
        let result = if success { "success" } else { "failure" };
        let mut event = AuditEvent::new(
            user_id,
            process_id,
            "create_snapshot",
            snapshot_name,
            result,
        );

        if let Some(err) = error {
            event = event.with_details(format!("error: {}", err));
        }

        event.log();
    }

    /// Log a snapshot deletion event
    pub fn log_snapshot_delete(
        user_id: String,
        process_id: u32,
        snapshot_name: &str,
        success: bool,
        error: Option<&str>,
    ) {
        let result = if success { "success" } else { "failure" };
        let mut event = AuditEvent::new(
            user_id,
            process_id,
            "delete_snapshot",
            snapshot_name,
            result,
        );

        if let Some(err) = error {
            event = event.with_details(format!("error: {}", err));
        }

        event.log();
    }

    /// Log a snapshot restore/rollback event
    pub fn log_snapshot_restore(
        user_id: String,
        process_id: u32,
        snapshot_name: &str,
        success: bool,
        error: Option<&str>,
    ) {
        let result = if success { "success" } else { "failure" };
        let mut event = AuditEvent::new(
            user_id,
            process_id,
            "restore_snapshot",
            snapshot_name,
            result,
        );

        if let Some(err) = error {
            event = event.with_details(format!("error: {}", err));
        }

        event.log();
    }

    /// Log a configuration change event
    pub fn log_config_change(
        user_id: String,
        process_id: u32,
        config_type: &str,
        success: bool,
        error: Option<&str>,
    ) {
        let result = if success { "success" } else { "failure" };
        let mut event = AuditEvent::new(
            user_id,
            process_id,
            "modify_configuration",
            config_type,
            result,
        );

        if let Some(err) = error {
            event = event.with_details(format!("error: {}", err));
        }

        event.log();
    }

    /// Log an authorization failure
    pub fn log_auth_failure(
        user_id: String,
        process_id: u32,
        operation: &str,
        reason: &str,
    ) {
        let event = AuditEvent::new(
            user_id,
            process_id,
            operation,
            "authorization",
            "denied",
        ).with_details(format!("reason: {}", reason));

        event.log();
    }
}

/// Simple rate limiter to prevent DoS via expensive operations
/// Implements a per-user, per-operation cooldown period
#[derive(Debug, Clone)]
struct RateLimiter {
    last_operation: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>>,
    window: std::time::Duration,
}

impl RateLimiter {
    fn new(window_seconds: u64) -> Self {
        Self {
            last_operation: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            window: std::time::Duration::from_secs(window_seconds),
        }
    }

    /// Check if operation is allowed for this user
    /// Returns Ok(()) if allowed, Err with time to wait if rate limited
    fn check_rate_limit(&self, user_id: &str, operation: &str) -> Result<(), std::time::Duration> {
        let mut state = self.last_operation.lock().unwrap_or_else(|poisoned| {
            log::error!("Rate limiter mutex poisoned, recovering");
            poisoned.into_inner()
        });
        let key = format!("{}:{}", user_id, operation);
        let now = std::time::Instant::now();

        if let Some(last_time) = state.get(&key) {
            let elapsed = now.duration_since(*last_time);
            if elapsed < self.window {
                // Still within rate limit window
                let wait_time = self.window - elapsed;
                return Err(wait_time);
            }
        }

        // Update last operation time
        state.insert(key, now);
        Ok(())
    }
}

/// Get the configured scheduler config path
fn scheduler_config_path() -> String {
    let config = WaypointConfig::new();
    config.scheduler_config.to_string_lossy().to_string()
}

/// Get the configured scheduler service path
fn scheduler_service_path() -> String {
    let config = WaypointConfig::new();
    config
        .scheduler_service_path()
        .to_string_lossy()
        .to_string()
}

/// Main D-Bus service interface for Waypoint operations
struct WaypointHelper {
    rate_limiter: RateLimiter,
}

impl WaypointHelper {
    fn new() -> Self {
        Self {
            // Rate limit: 1 operation per 5 seconds per user
            rate_limiter: RateLimiter::new(5),
        }
    }

    /// Get caller's user ID from D-Bus header
    async fn get_caller_uid(hdr: &zbus::message::Header<'_>, connection: &Connection) -> Result<String> {
        let caller = hdr
            .sender()
            .context("No sender in message header")?;

        let response = connection
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetConnectionUnixUser",
                &caller.as_str(),
            )
            .await
            .context("Failed to get caller UID from D-Bus")?;

        let uid: u32 = response
            .body()
            .deserialize()
            .context("Failed to deserialize caller UID")?;

        Ok(uid.to_string())
    }

    /// Get caller's process ID from D-Bus header
    async fn get_caller_pid(hdr: &zbus::message::Header<'_>, connection: &Connection) -> Result<u32> {
        let caller = hdr
            .sender()
            .context("No sender in message header")?;

        let response = connection
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetConnectionUnixProcessID",
                &caller.as_str(),
            )
            .await
            .context("Failed to get caller PID from D-Bus")?;

        response
            .body()
            .deserialize()
            .context("Failed to deserialize caller PID")
    }

    /// Get both UID and PID for audit logging
    async fn get_caller_info(hdr: &zbus::message::Header<'_>, connection: &Connection) -> (String, u32) {
        let uid = Self::get_caller_uid(hdr, connection).await.unwrap_or_else(|_| "unknown".to_string());
        let pid = Self::get_caller_pid(hdr, connection).await.unwrap_or(0);
        (uid, pid)
    }
}

#[interface(name = "tech.geektoshi.waypoint.Helper")]
impl WaypointHelper {
    /// Signal emitted when a snapshot is created
    #[zbus(signal)]
    async fn snapshot_created(
        ctxt: &zbus::SignalContext<'_>,
        snapshot_name: &str,
        created_by: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted during backup operations to report progress
    #[zbus(signal)]
    async fn backup_progress(
        ctxt: &zbus::SignalContext<'_>,
        snapshot_id: &str,
        destination_uuid: &str,
        bytes_transferred: u64,
        total_bytes: u64,
        speed_bytes_per_sec: u64,
        stage: &str, // "preparing", "transferring", "verifying", "complete"
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
        // Get caller info for audit logging
        let (uid, pid) = Self::get_caller_info(&hdr, connection).await;

        // Check authorization
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_CREATE).await {
            audit::log_auth_failure(uid, pid, "create_snapshot", &e.to_string());
            return (false, format!("Authorization failed: {}", e));
        }

        // Rate limiting check
        if let Err(wait_time) = self.rate_limiter.check_rate_limit(&uid, "create_snapshot") {
            log::warn!("Rate limit exceeded for user {} creating snapshot", uid);
            audit::log_snapshot_create(uid, pid, &name, false, Some("rate limit exceeded"));
            return (
                false,
                format!(
                    "Rate limit exceeded. Please wait {} seconds before creating another snapshot",
                    wait_time.as_secs()
                ),
            );
        }

        // Create the snapshot
        match Self::create_snapshot_impl(&name, &description, subvolumes) {
            Ok(msg) => {
                // Audit log successful creation
                audit::log_snapshot_create(uid.clone(), pid, &name, true, None);
                // Emit signal for successful snapshot creation
                // Try to determine who created the snapshot
                let created_by = if hdr
                    .sender()
                    .map(|s| s.as_str())
                    .unwrap_or("")
                    .contains("waypoint-scheduler")
                {
                    "scheduler"
                } else {
                    "gui"
                };

                if let Err(e) = Self::snapshot_created(&ctxt, &name, created_by).await {
                    log::error!("Failed to emit snapshot_created signal: {}", e);
                }

                (true, msg)
            }
            Err(e) => {
                // Audit log failed creation
                let error_msg = e.to_string();
                audit::log_snapshot_create(uid, pid, &name, false, Some(&error_msg));
                (false, format!("Failed to create snapshot: {}", sanitize_error_for_client(&e)))
            }
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
        result_to_dbus_response(
            Self::restore_snapshot_impl(&name),
            "Failed to restore snapshot"
        )
    }

    /// List all snapshots
    async fn list_snapshots(&self) -> String {
        // Listing doesn't require authorization (read-only)
        match btrfs::list_snapshots() {
            Ok(snapshots) => {
                let snapshot_infos: Vec<SnapshotInfo> =
                    snapshots.into_iter().map(|s| s.into()).collect();

                serde_json::to_string(&snapshot_infos).unwrap_or_else(|_| "[]".to_string())
            }
            Err(e) => {
                log::error!("Failed to list snapshots: {}", e);
                "[]".to_string()
            }
        }
    }

    /// Get sizes for multiple snapshots
    /// Returns JSON object mapping snapshot names to sizes in bytes
    /// This method runs with privileges, so it can access snapshot directories
    async fn get_snapshot_sizes(&self, snapshot_names: Vec<String>) -> String {
        // Getting sizes is read-only, no authorization needed
        match btrfs::get_snapshot_sizes(snapshot_names) {
            Ok(sizes) => serde_json::to_string(&sizes).unwrap_or_else(|_| "{}".to_string()),
            Err(e) => {
                log::error!("Failed to get snapshot sizes: {}", e);
                "{}".to_string()
            }
        }
    }

    /// Verify snapshot integrity
    async fn verify_snapshot(&self, name: String) -> String {
        // Verification is read-only, no authorization needed
        match btrfs::verify_snapshot(&name) {
            Ok(result) => serde_json::to_string(&result).unwrap_or_else(|_| {
                r#"{"is_valid":false,"errors":["Failed to serialize result"],"warnings":[]}"#
                    .to_string()
            }),
            Err(e) => {
                log::error!("Failed to verify snapshot: {}", e);
                serde_json::to_string(&btrfs::VerificationResult {
                    is_valid: false,
                    errors: vec![format!("Verification failed: {}", e)],
                    warnings: vec![],
                })
                .unwrap_or_else(|_| {
                    r#"{"is_valid":false,"errors":["Failed to verify"],"warnings":[]}"#.to_string()
                })
            }
        }
    }

    /// Preview what will happen if a snapshot is restored
    async fn preview_restore(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        name: String,
    ) -> (bool, String) {
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_RESTORE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        match btrfs::preview_restore(&name) {
            Ok(result) => match serde_json::to_string(&result) {
                Ok(json) => (true, json),
                Err(e) => (false, format!("Failed to serialize preview: {}", e)),
            },
            Err(e) => {
                log::error!("Failed to preview restore: {}", e);
                (false, format!("Failed to preview restore: {}", e))
            }
        }
    }

    /// Update scheduler configuration (DEPRECATED - use save_schedules_config instead)
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

        log::warn!("update_scheduler_config is deprecated, use save_schedules_config instead");

        // Basic validation: check for obvious injection attempts
        // This is legacy key=value format, not TOML
        if config_content.contains('\0') {
            return (false, "Configuration contains invalid null bytes".to_string());
        }

        // Check reasonable size limit (10KB should be more than enough for a simple config)
        if config_content.len() > 10240 {
            return (false, "Configuration file too large".to_string());
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

        // Validate TOML by parsing it first
        use waypoint_common::schedules::SchedulesConfig;
        match toml::from_str::<SchedulesConfig>(&toml_content) {
            Ok(_) => {
                // TOML is valid, proceed to save
            }
            Err(e) => {
                return (false, format!("Invalid TOML configuration: {}", e));
            }
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
        std::fs::write(&schedules_path, toml_content)
            .map(|_| (true, "Schedules configuration saved".to_string()))
            .unwrap_or_else(|e| (false, format!("Failed to save configuration: {}", e)))
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

        run_command("sv", &["restart", "waypoint-scheduler"])
            .map(|_| (true, "Scheduler service restarted".to_string()))
            .unwrap_or_else(|e| (false, format!("Failed to restart scheduler service: {}", e)))
    }

    /// Get scheduler service status
    async fn get_scheduler_status(&self) -> String {
        let service_enabled = std::path::Path::new(&scheduler_service_path()).exists();

        if !service_enabled {
            return "disabled".to_string();
        }

        run_command_with_output("sv", &["status", "waypoint-scheduler"])
            .map(|(stdout, stderr)| {
                if stdout.contains("run:") {
                    "running".to_string()
                } else if stdout.contains("down:") || stderr.contains("unable to") {
                    "stopped".to_string()
                } else {
                    "unknown".to_string()
                }
            })
            .unwrap_or_else(|e| {
                log::warn!("Failed to query scheduler status: {}", e);
                "unknown".to_string()
            })
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
        result_to_dbus_response(
            Self::cleanup_snapshots_impl(schedule_based),
            "Cleanup failed"
        )
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
        result_to_dbus_response(
            Self::restore_files_impl(&snapshot_name, file_paths, &target_directory, overwrite),
            "File restoration failed"
        )
    }

    /// Compare two snapshots and return list of changed files
    ///
    /// Uses btrfs send with --no-data to efficiently detect file changes between snapshots.
    async fn compare_snapshots(
        &self,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &Connection,
        old_snapshot_name: String,
        new_snapshot_name: String,
    ) -> (bool, String) {
        if let Err(e) = check_authorization(&hdr, connection, POLKIT_ACTION_RESTORE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        result_to_dbus_response(
            Self::compare_snapshots_impl(&old_snapshot_name, &new_snapshot_name),
            "Comparison failed"
        )
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

        result_to_dbus_response(
            Self::enable_quotas_impl(use_simple),
            "Failed to enable quotas"
        )
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

        result_to_dbus_response(
            Self::disable_quotas_impl(),
            "Failed to disable quotas"
        )
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

        result_to_dbus_response(
            Self::get_quota_usage_impl(),
            "Failed to get quota usage"
        )
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

        result_to_dbus_response(
            Self::set_quota_limit_impl(limit_bytes),
            "Failed to set quota limit"
        )
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

        result_to_dbus_response(
            Self::save_quota_config_impl(&config_toml),
            "Failed to save quota configuration"
        )
    }

    /// Scan for available backup destinations
    ///
    /// This is a read-only operation and does not require authorization
    async fn scan_backup_destinations(&self) -> (bool, String) {
        match backup::scan_backup_destinations() {
            Ok(destinations) => match serde_json::to_string(&destinations) {
                Ok(json) => (true, json),
                Err(e) => (false, format!("Failed to serialize destinations: {}", e)),
            },
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
        #[zbus(signal_context)] ctxt: zbus::SignalContext<'_>,
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

        // Look up UUID for this mount point by scanning
        let destination_uuid = match backup::scan_backup_destinations() {
            Ok(destinations) => {
                destinations.iter()
                    .find(|d| d.mount_point == destination_mount)
                    .and_then(|d| d.uuid.clone())
                    .unwrap_or_else(|| {
                        log::warn!("Could not find UUID for mount point {}", destination_mount);
                        destination_mount.clone() // Fallback to mount point
                    })
            }
            Err(e) => {
                log::error!("Failed to scan destinations for UUID lookup: {}", e);
                destination_mount.clone() // Fallback to mount point
            }
        };

        // Create bounded channel for progress updates (use std mpsc for sync/blocking code)
        // Buffer size of 100 messages provides backpressure if consumer is slow
        // This prevents unbounded memory growth if progress updates come faster than D-Bus signals can be sent
        let (progress_tx, progress_rx) = std::sync::mpsc::sync_channel::<backup::BackupProgress>(100);
        let progress_rx = std::sync::Arc::new(std::sync::Mutex::new(progress_rx));

        // Clone data for the blocking task
        let snapshot_path_clone = snapshot_path.clone();
        let destination_mount_clone = destination_mount.clone();
        let parent_clone = parent.map(|s| s.to_string());

        // Spawn blocking task for backup
        let mut backup_handle = tokio::task::spawn_blocking(move || {
            backup::backup_snapshot(
                &snapshot_path_clone,
                &destination_mount_clone,
                parent_clone.as_deref(),
                Some(progress_tx),
            )
        });

        // Poll for progress updates and emit signals
        loop {
            tokio::select! {
                // Check for progress messages (non-blocking)
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // Try to receive progress updates
                    let rx_clone = progress_rx.clone();
                    let dest_uuid_ref = destination_uuid.clone();
                    if let Ok(progress) = tokio::task::spawn_blocking(move || {
                        rx_clone.lock().unwrap_or_else(|poisoned| {
                            log::error!("Progress receiver mutex poisoned, recovering");
                            poisoned.into_inner()
                        }).try_recv()
                    }).await {
                        if let Ok(progress) = progress {
                            if let Err(e) = Self::backup_progress(
                                &ctxt,
                                &progress.snapshot_id,
                                &dest_uuid_ref, // Use looked-up UUID
                                progress.bytes_transferred,
                                progress.total_bytes,
                                progress.speed_bytes_per_sec,
                                &progress.stage,
                            ).await {
                                log::error!("Failed to emit backup_progress signal: {}", e);
                            }
                        }
                    }
                }

                // Wait for backup to complete
                result = &mut backup_handle => {
                    // Drain any remaining progress messages
                    loop {
                        let rx_clone = progress_rx.clone();
                        let dest_uuid_ref = destination_uuid.clone();
                        match tokio::task::spawn_blocking(move || {
                            rx_clone.lock().unwrap_or_else(|poisoned| {
                                log::error!("Progress receiver mutex poisoned during drain, recovering");
                                poisoned.into_inner()
                            }).try_recv()
                        }).await {
                            Ok(Ok(progress)) => {
                                let _ = Self::backup_progress(
                                    &ctxt,
                                    &progress.snapshot_id,
                                    &dest_uuid_ref, // Use looked-up UUID
                                    progress.bytes_transferred,
                                    progress.total_bytes,
                                    progress.speed_bytes_per_sec,
                                    &progress.stage,
                                ).await;
                            }
                            _ => break,
                        }
                    }

                    // Return backup result
                    return match result {
                        Ok(Ok((backup_path, size_bytes))) => (true, backup_path, size_bytes),
                        Ok(Err(e)) => (false, format!("Failed to backup snapshot: {}", e), 0),
                        Err(e) => (false, format!("Backup task failed: {}", e), 0),
                    };
                }
            }
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
            Ok(backups) => match serde_json::to_string(&backups) {
                Ok(json) => (true, json),
                Err(e) => (false, format!("Failed to serialize backups: {}", e)),
            },
            Err(e) => (false, format!("Failed to list backups: {}", e)),
        }
    }

    /// Get drive health statistics
    async fn get_drive_stats(
        &self,
        destination_mount: String,
    ) -> (bool, String) {
        match backup::get_drive_stats(&destination_mount) {
            Ok(stats) => match serde_json::to_string(&stats) {
                Ok(json) => (true, json),
                Err(e) => (false, format!("Failed to serialize stats: {}", e)),
            },
            Err(e) => (false, format!("Failed to get drive stats: {}", e)),
        }
    }

    /// Verify a backup's integrity
    ///
    /// # Arguments
    /// * `snapshot_path` - Full path to the original snapshot (e.g., /.snapshots/my-snapshot)
    /// * `destination_mount` - Mount point of backup destination
    /// * `snapshot_id` - ID/name of the snapshot to verify
    ///
    /// # Returns
    /// * `(success, json_result)` - JSON containing verification details
    async fn verify_backup(
        &self,
        snapshot_path: String,
        destination_mount: String,
        snapshot_id: String,
    ) -> (bool, String) {
        // Verification is read-only but still needs input validation to avoid probing arbitrary paths
        match backup::verify_backup(&snapshot_path, &destination_mount, &snapshot_id) {
            Ok(result) => match serde_json::to_string(&result) {
                Ok(json) => (true, json),
                Err(e) => (false, format!("Failed to serialize verification result: {}", e)),
            },
            Err(e) => (false, format!("Verification failed: {}", e)),
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
    fn create_snapshot_impl(
        name: &str,
        description: &str,
        subvolumes: Vec<String>,
    ) -> Result<String> {
        // Check quota and cleanup if needed
        if let Err(e) = Self::check_quota_and_cleanup() {
            log::warn!("Failed to check quota before snapshot: {}", e);
            // Continue anyway - quota check is not critical
        }

        // Get installed packages
        let packages =
            packages::get_installed_packages().context("Failed to get installed packages")?;

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
        // Use timestamp + counter to ensure uniqueness even if multiple rollbacks in same second
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
        let mut backup_name = format!("waypoint-pre-rollback-{}", timestamp);

        // Check if snapshot with this name already exists, add counter if needed
        let existing_snapshots = btrfs::list_snapshots().unwrap_or_default();
        let mut counter = 1;
        while existing_snapshots.iter().any(|s| s.name == backup_name) {
            backup_name = format!("waypoint-pre-rollback-{}-{}", timestamp, counter);
            counter += 1;

            // Sanity check to prevent infinite loop
            if counter > 1000 {
                anyhow::bail!("Too many pre-rollback snapshots with same timestamp");
            }
        }

        let packages = packages::get_installed_packages()
            .context("Failed to get installed packages for backup")?;

        // Backup only root filesystem
        let root_only = vec![std::path::PathBuf::from("/")];
        btrfs::create_snapshot(
            &backup_name,
            Some("Pre-rollback backup"),
            packages,
            root_only,
        )
        .context("Failed to create pre-rollback backup")?;

        // Perform the rollback
        btrfs::restore_snapshot(name).context("Failed to restore snapshot")?;

        Ok(format!(
            "Snapshot '{}' will be active after reboot. Backup created: '{}'",
            name, backup_name
        ))
    }

    fn cleanup_snapshots_impl(schedule_based: bool) -> Result<String> {
        use std::collections::HashSet;
        use waypoint_common::WaypointConfig;
        use waypoint_common::schedules::SchedulesConfig;
        use waypoint_common::retention::{apply_timeline_retention, SnapshotForRetention};

        let config = WaypointConfig::new();
        let snapshots = btrfs::list_snapshots().context("Failed to list snapshots")?;

        // Load snapshot metadata to check for pinned/favorited snapshots
        #[derive(Deserialize)]
        struct SnapshotMetadataEntry {
            id: String,
            #[serde(default)]
            is_favorite: bool,
        }

        let favorited_ids: HashSet<String> = {
            if let Ok(content) = std::fs::read_to_string(&config.metadata_file) {
                if let Ok(metadata) = serde_json::from_str::<Vec<SnapshotMetadataEntry>>(&content) {
                    metadata
                        .iter()
                        .filter(|m| m.is_favorite)
                        .map(|m| m.id.clone())
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

                let matching: Vec<_> = snapshots
                    .iter()
                    .filter(|s| s.name.starts_with(&schedule.prefix))
                    .collect();

                let now = chrono::Utc::now();

                // Use timeline retention if available, otherwise fall back to legacy keep_count/keep_days
                let delete_list = if let Some(timeline) = &schedule.timeline_retention {
                    // Convert to SnapshotForRetention format
                    let retention_snapshots: Vec<SnapshotForRetention> = matching
                        .iter()
                        .map(|s| SnapshotForRetention {
                            name: s.name.clone(),
                            timestamp: s.timestamp,
                        })
                        .collect();

                    // Apply timeline-based retention
                    apply_timeline_retention(&retention_snapshots, timeline, now)
                } else {
                    // Legacy retention: use keep_count and keep_days
                    let mut legacy_delete = Vec::new();
                    let mut matching_sorted = matching.clone();
                    matching_sorted.sort_by_key(|s| s.timestamp);

                    for (idx, snapshot) in matching_sorted.iter().enumerate() {
                        let mut should_delete = false;

                        // Apply keep_count
                        if schedule.keep_count > 0 {
                            let position_from_end = matching_sorted.len() - idx;
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

                        if should_delete {
                            legacy_delete.push(snapshot.name.clone());
                        }
                    }
                    legacy_delete
                };

                // Filter out favorited snapshots
                for name in delete_list {
                    if !favorited_ids.contains(&name) {
                        all_to_delete.push(name);
                    }
                }
            }
            all_to_delete
        } else {
            // Legacy global retention policy is not implemented
            anyhow::bail!(
                "Legacy global retention policy is not implemented. \
                 Please use --schedule-based flag with waypoint-cli cleanup. \
                 Schedule-based retention allows fine-grained control per backup schedule."
            );
        };

        if to_delete.is_empty() {
            return Ok("No snapshots to clean up".to_string());
        }

        // Delete snapshots
        let mut deleted = 0;
        let mut failed = Vec::new();

        for snapshot_name in &to_delete {
            if let Err(e) = btrfs::ensure_snapshot_name(snapshot_name) {
                log::error!(
                    "Skipping snapshot '{}' due to invalid name/path: {}",
                    snapshot_name,
                    e
                );
                failed.push(snapshot_name.clone());
                continue;
            }
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
            Ok(format!(
                "Cleaned up {} snapshot(s), failed to delete: {:?}",
                deleted, failed
            ))
        }
    }

    fn restore_files_impl(
        snapshot_name: &str,
        file_paths: Vec<String>,
        target_directory: &str,
        overwrite: bool,
    ) -> Result<String> {
        use std::fs;
        use std::path::{Component, Path, PathBuf};

        waypoint_common::validate_snapshot_name(snapshot_name)
            .map_err(|e| anyhow::anyhow!("Invalid snapshot name '{}': {}", snapshot_name, e))?;

        let config = WaypointConfig::new();
        let snapshot_base_dir = config.snapshot_dir.join(snapshot_name);

        // Load snapshot metadata (from global metadata file) to get list of subvolumes
        let metadata_snapshot = crate::btrfs::get_snapshot_metadata(snapshot_name)
            .context("Failed to load snapshot metadata")?;
        let subvolumes = if metadata_snapshot.subvolumes.is_empty() {
            vec![PathBuf::from("/")]
        } else {
            metadata_snapshot.subvolumes.clone()
        };

        if subvolumes.is_empty() {
            anyhow::bail!("Snapshot {} has no subvolumes recorded in metadata", snapshot_name);
        }

        // Helper function to map a file path to its subvolume directory name
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

        // Helper to find which subvolume contains a given file path
        fn find_subvolume_for_path(file_path: &Path, subvolumes: &[PathBuf]) -> Result<PathBuf> {
            // Find the most specific (longest) subvolume that contains this path
            let mut best_match: Option<&PathBuf> = None;
            let mut best_len = 0;

            for subvol in subvolumes {
                if file_path.starts_with(subvol) {
                    let len = subvol.as_os_str().len();
                    if len > best_len {
                        best_match = Some(subvol);
                        best_len = len;
                    }
                }
            }

            best_match.cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "No subvolume found for path {}. Available subvolumes: {:?}",
                    file_path.display(),
                    subvolumes
                )
            })
        }

        if file_paths.is_empty() {
            anyhow::bail!("No files specified for restoration");
        }

        fn ensure_safe_absolute(path: &Path) -> Result<()> {
            if !path.is_absolute() {
                anyhow::bail!("Path must be absolute: {}", path.display());
            }
            for component in path.components() {
                match component {
                    Component::RootDir | Component::Normal(_) => {}
                    _ => {
                        anyhow::bail!(
                            "Path contains disallowed component '{}'",
                            component.as_os_str().to_string_lossy()
                        );
                    }
                }
            }
            Ok(())
        }

        fn sanitize_absolute_path(path: &str) -> Result<PathBuf> {
            let candidate = Path::new(path);
            ensure_safe_absolute(candidate)?;
            Ok(candidate.to_path_buf())
        }

        let mut restored_count = 0;
        let mut failed_files = Vec::new();
        let use_custom_target = !target_directory.is_empty();
        let custom_target_base = if use_custom_target {
            let base_path = Path::new(target_directory);
            ensure_safe_absolute(base_path)?;
            Some(base_path.to_path_buf())
        } else {
            None
        };

        for file_path in &file_paths {
            // Ensure path starts with /
            let normalized_path = if file_path.starts_with('/') {
                file_path.to_string()
            } else {
                format!("/{}", file_path)
            };

            // Validate path structure to prevent traversal outside the snapshot
            let path_buf = sanitize_absolute_path(&normalized_path).map_err(|e| {
                anyhow::anyhow!("Invalid restore path '{}': {}", normalized_path, e)
            })?;

            // Find which subvolume contains this file
            let subvolume_mount = find_subvolume_for_path(&path_buf, &subvolumes)?;
            let subvolume_dir_name = mount_point_to_subdir_name(&subvolume_mount);
            let subvolume_dir = snapshot_base_dir.join(&subvolume_dir_name);

            // Verify subvolume directory exists
            let snapshot_root = subvolume_dir.canonicalize().with_context(|| {
                format!(
                    "Subvolume '{}' not found in snapshot '{}' at {}",
                    subvolume_mount.display(),
                    snapshot_name,
                    subvolume_dir.display()
                )
            })?;

            // Verify the canonicalized path is still within the expected snapshot directory
            if !snapshot_root.starts_with(&config.snapshot_dir) {
                anyhow::bail!(
                    "Security: Subvolume path resolves outside snapshot directory. \
                     Expected under {}, got {}",
                    config.snapshot_dir.display(),
                    snapshot_root.display()
                );
            }

            // Build source path relative to subvolume mount point
            // For example: /home/user/file.txt with subvolume /home becomes user/file.txt
            let relative_path = path_buf.strip_prefix(&subvolume_mount)
                .with_context(|| format!(
                    "Path {} should start with subvolume mount {}",
                    path_buf.display(),
                    subvolume_mount.display()
                ))?;

            let source = snapshot_root.join(relative_path);

            if !source.exists() {
                log::warn!("File not found in snapshot: {}", normalized_path);
                failed_files.push(normalized_path.clone());
                continue;
            }

            // Determine target path
            let target = if let Some(base_dir) = &custom_target_base {
                // Restore to custom directory, preserving filename
                let filename = source
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?;
                base_dir.join(filename)
            } else {
                // Restore to original location
                sanitize_absolute_path(&normalized_path)?
            };

            // Check if target exists
            if target.exists() {
                if !overwrite {
                    log::warn!("File exists and overwrite disabled: {}", target.display());
                    failed_files.push(normalized_path.clone());
                    continue;
                }

                if let Ok(target_metadata) = fs::symlink_metadata(&target) {
                    if target_metadata.file_type().is_symlink() || target_metadata.is_file() {
                        if let Err(e) = fs::remove_file(&target) {
                            log::error!("Failed to replace {}: {}", target.display(), e);
                            failed_files.push(normalized_path.clone());
                            continue;
                        }
                    }
                }
            }

            // Create parent directory if needed
            if let Some(parent) = target.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    log::error!(
                        "Failed to create parent directory for {}: {}",
                        target.display(),
                        e
                    );
                    failed_files.push(normalized_path.clone());
                    continue;
                }
            }

            match fs::symlink_metadata(&source) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        #[cfg(unix)]
                        {
                            match fs::read_link(&source) {
                                Ok(link_target) => {
                                    // Validate symlink target for security
                                    if let Err(e) = validate_symlink_target(&link_target, &source, &snapshot_root) {
                                        log::error!(
                                            "Skipping unsafe symlink {}: {} -> {}: {}",
                                            normalized_path,
                                            source.display(),
                                            link_target.display(),
                                            e
                                        );
                                        failed_files.push(normalized_path.clone());
                                        continue;
                                    }

                                    // Safe to restore symlink
                                    match symlink(&link_target, &target) {
                                        Ok(_) => {
                                            restored_count += 1;
                                        }
                                        Err(e) => {
                                            log::error!(
                                                "Failed to create symlink {}: {}",
                                                normalized_path,
                                                e
                                            );
                                            failed_files.push(normalized_path.clone());
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!(
                                        "Failed to read symlink {}: {}",
                                        normalized_path,
                                        e
                                    );
                                    failed_files.push(normalized_path.clone());
                                }
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            log::warn!(
                                "Symlink restore not supported on this platform: {}",
                                normalized_path
                            );
                            failed_files.push(normalized_path.clone());
                        }
                    } else if metadata.is_dir() {
                        if let Err(e) = copy_dir_recursive(&snapshot_root, &source, &target) {
                            log::error!("Failed to restore directory {}: {}", normalized_path, e);
                            failed_files.push(normalized_path.clone());
                        } else {
                            restored_count += 1;
                        }
                    } else if metadata.is_file() {
                        if let Err(e) = fs::copy(&source, &target) {
                            log::error!("Failed to restore file {}: {}", normalized_path, e);
                            failed_files.push(normalized_path.clone());
                        } else {
                            if let Err(e) = preserve_metadata(&source, &target) {
                                log::warn!(
                                    "File restored but failed to preserve metadata for {}: {}",
                                    target.display(),
                                    e
                                );
                            }
                            restored_count += 1;
                        }
                    } else {
                        log::warn!("Unsupported file type in snapshot: {}", normalized_path);
                        failed_files.push(normalized_path.clone());
                    }
                }
                Err(e) => {
                    log::error!("Failed to inspect {}: {}", normalized_path, e);
                    failed_files.push(normalized_path.clone());
                }
            }
        }

        if failed_files.is_empty() {
            Ok(format!(
                "Successfully restored {} file(s) from snapshot '{}'",
                restored_count, snapshot_name
            ))
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

        waypoint_common::validate_snapshot_name(old_snapshot_name)
            .map_err(|e| anyhow::anyhow!("Invalid snapshot name '{}': {}", old_snapshot_name, e))?;
        waypoint_common::validate_snapshot_name(new_snapshot_name)
            .map_err(|e| anyhow::anyhow!("Invalid snapshot name '{}': {}", new_snapshot_name, e))?;

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
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn btrfs send")?;

        let send_stdout = send_cmd
            .stdout
            .take()
            .context("Failed to capture btrfs send stdout")?;
        let send_stderr_handle = send_cmd.stderr.take().map(|stderr| {
            std::thread::spawn(move || -> Result<String> {
                let mut reader = BufReader::new(stderr);
                let mut buf = String::new();
                reader.read_to_string(&mut buf)?;
                Ok(buf)
            })
        });

        let mut receive_cmd = Command::new("btrfs")
            .arg("receive")
            .arg("--dump")
            .stdin(Stdio::from(send_stdout))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn btrfs receive")?;
        let receive_stdout = receive_cmd
            .stdout
            .take()
            .context("Failed to capture btrfs receive stdout")?;
        let receive_stderr_handle = receive_cmd.stderr.take().map(|stderr| {
            std::thread::spawn(move || -> Result<String> {
                let mut reader = BufReader::new(stderr);
                let mut buf = String::new();
                reader.read_to_string(&mut buf)?;
                Ok(buf)
            })
        });

        // Drain receive stdout concurrently to prevent pipe deadlocks
        let reader_handle = std::thread::spawn(move || -> Result<String> {
            let mut reader = BufReader::new(receive_stdout);
            let mut output = String::new();
            reader.read_to_string(&mut output)?;
            Ok(output)
        });

        // Wait for receive first to ensure dump output is consumed
        let receive_status = receive_cmd.wait()?;
        let receive_stderr = match receive_stderr_handle {
            Some(handle) => handle
                .join()
                .map_err(|_| anyhow::anyhow!("Failed to join receive stderr reader"))??,
            None => String::new(),
        };
        if !receive_status.success() {
            let detail = if receive_stderr.is_empty() {
                format!("{}", receive_status)
            } else {
                format!("{} - {}", receive_status, receive_stderr.trim())
            };
            anyhow::bail!("btrfs receive --dump failed: {}", detail);
        }

        let output = reader_handle
            .join()
            .map_err(|_| anyhow::anyhow!("Failed to join receive output reader"))??;

        let send_status = send_cmd.wait()?;
        let send_stderr = match send_stderr_handle {
            Some(handle) => handle
                .join()
                .map_err(|_| anyhow::anyhow!("Failed to join send stderr reader"))??,
            None => String::new(),
        };
        if !send_status.success() {
            // btrfs send may emit SIGPIPE if receive exits after finishing.
            if send_status.signal() == Some(libc::SIGPIPE) {
                log::debug!("btrfs send exited with SIGPIPE after receive completed");
            } else {
                let detail = if send_stderr.is_empty() {
                    format!("{}", send_status)
                } else {
                    format!("{} - {}", send_status, send_stderr.trim())
                };
                anyhow::bail!("btrfs send failed: {}", detail);
            }
        }

        // Parse the dump output and convert to JSON
        let changes = parse_btrfs_dump(&output)?;

        // Serialize to JSON
        serde_json::to_string(&changes).context("Failed to serialize changes to JSON")
    }

    /// Enable quotas on the btrfs filesystem
    fn enable_quotas_impl(use_simple: bool) -> Result<String> {
        let config = WaypointConfig::new();
        let snapshot_dir = &config.snapshot_dir;
        let snapshot_dir_str = snapshot_dir.to_str()
            .ok_or_else(|| anyhow::anyhow!("Snapshot directory path contains invalid UTF-8: {}", snapshot_dir.display()))?;

        // Check if quotas are already enabled
        if run_command(
            "btrfs",
            &["qgroup", "show", snapshot_dir_str],
        )
        .is_ok()
        {
            return Ok("Quotas are already enabled".to_string());
        }

        // Enable quotas
        let mut args = vec!["quota", "enable"];
        if use_simple {
            args.push("--simple");
        }
        args.push(snapshot_dir_str);
        run_command("btrfs", &args)?;

        let quota_type = if use_simple { "simple" } else { "traditional" };
        Ok(format!("Successfully enabled {} quotas", quota_type))
    }

    /// Disable quotas on the btrfs filesystem
    fn disable_quotas_impl() -> Result<String> {
        let config = WaypointConfig::new();
        let snapshot_dir = &config.snapshot_dir;
        let snapshot_dir_str = snapshot_dir.to_str()
            .ok_or_else(|| anyhow::anyhow!("Snapshot directory path contains invalid UTF-8: {}", snapshot_dir.display()))?;

        run_command(
            "btrfs",
            &[
                "quota",
                "disable",
                snapshot_dir_str,
            ],
        )?;

        Ok("Successfully disabled quotas".to_string())
    }

    /// Get quota usage information
    fn get_quota_usage_impl() -> Result<String> {
        use waypoint_common::{QuotaConfig, QuotaUsage};

        let config = WaypointConfig::new();
        let snapshot_dir = &config.snapshot_dir;
        let snapshot_dir_str = snapshot_dir.to_str()
            .ok_or_else(|| anyhow::anyhow!("Snapshot directory path contains invalid UTF-8: {}", snapshot_dir.display()))?;

        // Get qgroup information
        let (stdout, _) = run_command_with_output(
            "btrfs",
            &[
                "qgroup",
                "show",
                "--raw",
                snapshot_dir_str,
            ],
        )?;

        // Parse qgroup output
        // Format: qgroupid rfer excl max_rfer max_excl
        // Sum up all level-0 qgroups (snapshots)
        let mut total_referenced = 0u64;
        let mut total_exclusive = 0u64;
        let mut parsed_lines = 0;

        for (line_num, line) in stdout.lines().skip(2).enumerate() {
            // Skip header lines
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() && parts[0].starts_with("0/") {
                // Only count level-0 qgroups (actual snapshots)
                if parts.len() < 3 {
                    log::warn!(
                        "Unexpected qgroup output format at line {}: '{}'. \
                         Expected at least 3 fields but got {}",
                        line_num + 3, // +3 because we skipped 2 header lines
                        line,
                        parts.len()
                    );
                    continue;
                }

                match (parts[1].parse::<u64>(), parts[2].parse::<u64>()) {
                    (Ok(rfer), Ok(excl)) => {
                        parsed_lines += 1;
                        // Use checked_add to detect overflow - fail loudly rather than silently saturate
                        total_referenced = total_referenced.checked_add(rfer)
                            .ok_or_else(|| anyhow::anyhow!(
                                "Quota calculation overflow: total referenced bytes exceed u64::MAX. \
                                 Current total: {}, attempted to add: {}",
                                total_referenced, rfer
                            ))?;
                        total_exclusive = total_exclusive.checked_add(excl)
                            .ok_or_else(|| anyhow::anyhow!(
                                "Quota calculation overflow: total exclusive bytes exceed u64::MAX. \
                                 Current total: {}, attempted to add: {}",
                                total_exclusive, excl
                            ))?;
                    }
                    (Err(e1), Err(e2)) => {
                        log::warn!(
                            "Failed to parse qgroup values at line {}: '{}'. \
                             Both rfer ('{}') and excl ('{}') parse failed: {}, {}",
                            line_num + 3,
                            line,
                            parts[1],
                            parts[2],
                            e1,
                            e2
                        );
                    }
                    (Err(e), Ok(_)) => {
                        log::warn!(
                            "Failed to parse qgroup rfer value at line {}: '{}'. \
                             Parse error: {}",
                            line_num + 3,
                            line,
                            e
                        );
                    }
                    (Ok(_), Err(e)) => {
                        log::warn!(
                            "Failed to parse qgroup excl value at line {}: '{}'. \
                             Parse error: {}",
                            line_num + 3,
                            line,
                            e
                        );
                    }
                }
            }
        }

        // Log if no qgroups were parsed (possible format change or quotas not enabled)
        if parsed_lines == 0 {
            log::info!(
                "No level-0 qgroups found in btrfs output. \
                 This is normal if quotas are not enabled or no snapshots exist yet."
            );
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

        serde_json::to_string(&usage).context("Failed to serialize quota usage to JSON")
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
            log::info!(
                "Quota usage exceeds threshold ({}%), triggering cleanup",
                quota_config.cleanup_threshold * 100.0
            );

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
                log::info!(
                    "Auto-cleanup: Deleted {} snapshot(s) to free quota space",
                    deleted_count
                );
            }
        }

        Ok(())
    }

    /// Save quota configuration to file
    fn save_quota_config_impl(config_toml: &str) -> Result<String> {
        use waypoint_common::QuotaConfig;

        // Validate TOML by parsing it
        let _config: QuotaConfig =
            toml::from_str(config_toml).context("Invalid quota configuration")?;

        let config_path = QuotaConfig::default_path();

        // Create parent directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        // Write configuration
        std::fs::write(&config_path, config_toml)
            .context("Failed to write quota configuration file")?;

        Ok("Quota configuration saved successfully".to_string())
    }
}

/// Validate that a symlink target is safe to restore
///
/// Ensures symlink targets:
/// 1. Don't point to absolute paths outside the snapshot
/// 2. Don't escape the snapshot via relative paths
///
/// # Arguments
/// * `link_target` - The symlink target path (as read from the symlink)
/// * `source_path` - The full path to the symlink in the snapshot
/// * `snapshot_root` - The canonicalized root of the snapshot
///
/// # Returns
/// Ok(()) if the symlink is safe, Err otherwise
fn validate_symlink_target(
    link_target: &std::path::Path,
    source_path: &std::path::Path,
    snapshot_root: &std::path::Path,
) -> Result<()> {
    // If the target is absolute, it must point within the snapshot
    if link_target.is_absolute() {
        // Resolve the absolute target
        let resolved = if link_target.exists() {
            link_target.canonicalize().ok()
        } else {
            // For non-existent targets, we can't canonicalize, so check the path directly
            Some(link_target.to_path_buf())
        };

        if let Some(resolved_path) = resolved {
            if !resolved_path.starts_with(snapshot_root) {
                anyhow::bail!(
                    "Security: Symlink {} points to absolute path {} outside snapshot",
                    source_path.display(),
                    link_target.display()
                );
            }
        }
    } else {
        // For relative symlinks, resolve them relative to the symlink's directory
        if let Some(parent) = source_path.parent() {
            let resolved_target = parent.join(link_target);

            // Try to canonicalize if it exists
            let final_target = if resolved_target.exists() {
                resolved_target.canonicalize().unwrap_or(resolved_target)
            } else {
                // Manually resolve .. and . components for non-existent paths
                let mut resolved = std::path::PathBuf::new();
                for component in resolved_target.components() {
                    match component {
                        std::path::Component::ParentDir => {
                            resolved.pop();
                        }
                        std::path::Component::CurDir => {
                            // Skip current dir references
                        }
                        other => {
                            resolved.push(other);
                        }
                    }
                }
                resolved
            };

            // Verify the resolved target is still within the snapshot
            if !final_target.starts_with(snapshot_root) {
                anyhow::bail!(
                    "Security: Symlink {} with relative target {} resolves to {} outside snapshot",
                    source_path.display(),
                    link_target.display(),
                    final_target.display()
                );
            }
        }
    }

    Ok(())
}

/// Recursively copy a directory and its contents without escaping the snapshot root
fn copy_dir_recursive(
    snapshot_root: &std::path::Path,
    source: &std::path::Path,
    target: &std::path::Path,
) -> Result<()> {
    use std::fs;

    if !source.starts_with(snapshot_root) {
        anyhow::bail!("Source {} is outside of snapshot root", source.display());
    }

    // Create target directory
    fs::create_dir_all(target)
        .context(format!("Failed to create directory: {}", target.display()))?;

    // Copy metadata
    preserve_metadata(source, target)?;

    // Iterate through directory entries
    for entry in
        fs::read_dir(source).context(format!("Failed to read directory: {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();

        // Validate each entry is still within snapshot root to prevent TOCTOU attacks
        // where directory contents are swapped during iteration
        if !source_path.starts_with(snapshot_root) {
            anyhow::bail!(
                "Security: Directory entry {} is outside snapshot root during copy",
                source_path.display()
            );
        }

        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .context(format!("Failed to stat {}", source_path.display()))?;

        if metadata.file_type().is_symlink() {
            #[cfg(unix)]
            {
                let link_target = fs::read_link(&source_path)
                    .context(format!("Failed to read symlink: {}", source_path.display()))?;

                // Validate symlink target for security
                if let Err(e) = validate_symlink_target(&link_target, &source_path, snapshot_root) {
                    log::error!(
                        "Skipping unsafe symlink during restore: {} -> {}: {}",
                        source_path.display(),
                        link_target.display(),
                        e
                    );
                    // Skip this symlink but continue with other files
                    continue;
                }

                symlink(&link_target, &target_path).context(format!(
                    "Failed to create symlink: {}",
                    target_path.display()
                ))?;
            }
            #[cfg(not(unix))]
            {
                log::warn!(
                    "Symlink restore not supported on this platform: {}",
                    source_path.display()
                );
            }
        } else if metadata.is_dir() {
            // Recursively copy subdirectory
            copy_dir_recursive(snapshot_root, &source_path, &target_path)?;
        } else if metadata.is_file() {
            // Copy file
            fs::copy(&source_path, &target_path)
                .context(format!("Failed to copy file: {}", source_path.display()))?;
            preserve_metadata(&source_path, &target_path)?;
        } else {
            log::warn!(
                "Unsupported file type encountered during restore: {}",
                source_path.display()
            );
        }
    }

    Ok(())
}

/// Preserve file metadata (permissions and ownership)
fn preserve_metadata(source: &std::path::Path, target: &std::path::Path) -> Result<()> {
    use std::fs;

    // Get source metadata
    let metadata =
        fs::metadata(source).context(format!("Failed to read metadata: {}", source.display()))?;

    // Set permissions
    let permissions = metadata.permissions();
    fs::set_permissions(target, permissions)
        .context(format!("Failed to set permissions: {}", target.display()))?;

    // Set ownership (requires root, which waypoint-helper has)
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use nix::unistd::{chown, Uid, Gid};

        let uid = metadata.uid();
        let gid = metadata.gid();

        // Use nix crate's safe wrapper instead of raw libc
        // This properly handles path encoding and errors
        match chown(target, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid))) {
            Ok(_) => {
                // Ownership set successfully
            }
            Err(e) => {
                log::warn!("Failed to set ownership for {}: {}", target.display(), e);
                // Don't fail the whole operation for ownership issues
                // This is intentional - some filesystems don't support ownership changes
            }
        }
    }

    Ok(())
}

/// Sanitize error messages to avoid exposing sensitive system paths
///
/// This function removes full paths from error messages that will be sent
/// to unprivileged clients over D-Bus, logging the full error internally.
fn sanitize_error_for_client(error: &anyhow::Error) -> String {
    let full_error = format!("{:#}", error);

    // Log the full error for administrators
    log::error!("Operation failed: {}", full_error);

    // Return sanitized version to client
    // Remove common path prefixes that could expose system layout
    let sanitized = full_error
        .replace("/home/", "<home>/")
        .replace("/root/", "<root>/")
        .replace("/etc/", "<etc>/")
        .replace("/var/", "<var>/")
        .replace("/usr/", "<usr>/")
        .replace("/opt/", "<opt>/")
        .replace("/tmp/", "<tmp>/")
        .replace("/.snapshots/", "<snapshots>/");

    // If the error is very long (contains stack traces, etc.), truncate it
    if sanitized.len() > 500 {
        format!("{}... (see system logs for details)", &sanitized[..500])
    } else {
        sanitized
    }
}

/// Convert a Result<String> to (bool, String) for D-Bus responses
/// Applies consistent error sanitization and formatting
fn result_to_dbus_response(result: Result<String>, error_prefix: &str) -> (bool, String) {
    match result {
        Ok(msg) => (true, msg),
        Err(e) => {
            let sanitized = sanitize_error_for_client(&e);
            (false, format!("{}: {}", error_prefix, sanitized))
        }
    }
}

/// Parse btrfs receive --dump output into structured changes
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct FileChange {
    change_type: String, // "Added", "Modified", "Deleted"
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
            "write" | "clone" | "set_xattr" | "remove_xattr" | "truncate" | "chmod" | "chown"
            | "utimes" => "Modified",
            "unlink" | "rmdir" => "Deleted",
            "rename" => "Modified", // Rename could be considered as modified
            _ => continue,          // Unknown command, skip
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
                            // Validate the byte is a safe character
                            // Only allow printable ASCII (32-126) and common whitespace (9-13)
                            // Reject null bytes and high bytes that could create invalid UTF-8
                            if (code >= 32 && code <= 126) || (code >= 9 && code <= 13) {
                                result.push(code as char);
                            } else {
                                // Replace unsafe bytes with Unicode replacement character
                                log::warn!("Unsafe byte in path escape sequence: \\{} (value: {})", octal, code);
                                result.push('\u{FFFD}');  // Unicode replacement character
                            }
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
    use std::collections::HashMap;
    use zbus::zvariant::{ObjectPath, Value};

    log::debug!("Authorization requested for action: {}", action_id);

    // Get the caller's bus name from the message header
    let caller = hdr
        .sender()
        .context("No sender in message header")?
        .to_owned();

    log::debug!("Caller bus name: {}", caller);

    // Get the caller's PID from D-Bus
    let response = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "GetConnectionUnixProcessID",
            &caller.as_str(),
        )
        .await
        .context("Failed to get caller PID from D-Bus")?;

    let caller_pid: u32 = response
        .body()
        .deserialize()
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
    // Note: This allows interactive authentication dialogs. For automated contexts
    // or security-sensitive deployments, consider using flag 0 (no interaction)
    // and configuring passwordless Polkit rules in /etc/polkit-1/rules.d/
    let flags: u32 = 1;

    // Cancellation ID (empty string = no cancellation)
    // Could be used to cancel long-running auth requests, but not needed here
    let cancellation_id = "";

    // Call Polkit CheckAuthorization
    // Note: Polkit handles timeouts internally based on system configuration.
    // Default timeout is typically 5 minutes for authentication dialogs.
    // For more restrictive timeouts, configure in /etc/polkit-1/polkit.conf
    let polkit_path = ObjectPath::try_from("/org/freedesktop/PolicyKit1/Authority")
        .context("Invalid Polkit object path")?;

    // Add explicit timeout to D-Bus call
    // This prevents indefinite hangs if Polkit service is unresponsive
    // Configurable via WAYPOINT_POLKIT_TIMEOUT environment variable (default: 120 seconds)
    let timeout_secs = std::env::var("WAYPOINT_POLKIT_TIMEOUT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(120);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        connection.call_method(
            Some("org.freedesktop.PolicyKit1"),
            polkit_path,
            Some("org.freedesktop.PolicyKit1.Authority"),
            "CheckAuthorization",
            &(subject, action_id, details, flags, cancellation_id),
        ),
    )
    .await
    .with_context(|| format!("Polkit authorization timed out after {} seconds", timeout_secs))?;

    let msg = result.context("Failed to call Polkit CheckAuthorization")?;

    // Result is (is_authorized, is_challenge, details)
    let (is_authorized, is_challenge, auth_details): (bool, bool, HashMap<String, String>) = msg
        .body()
        .deserialize()
        .context("Failed to deserialize Polkit response")?;

    log::debug!(
        "Authorization result: authorized={}, challenge={}, details={:?}",
        is_authorized,
        is_challenge,
        auth_details
    );

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
    let stat_content =
        fs::read_to_string(&stat_path).context(format!("Failed to read {}", stat_path))?;

    // The start time is the 22nd field in /proc/[pid]/stat
    // Fields are: pid (comm) state ppid ... starttime ...
    // We need to handle the (comm) field which may contain spaces and special characters

    // Find the last ')' to skip the comm field
    let start_pos = stat_content
        .rfind(')')
        .context("Invalid /proc/[pid]/stat format: missing closing parenthesis")?;

    // Ensure there's content after the ')' character
    if start_pos + 1 >= stat_content.len() {
        anyhow::bail!("Invalid /proc/[pid]/stat format: no fields after command name");
    }

    let fields: Vec<&str> = stat_content[start_pos + 1..].split_whitespace().collect();

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

    let start_time_str = fields.get(19)
        .ok_or_else(|| anyhow::anyhow!("Missing start_time field (index 19) in /proc/{}/stat", pid))?;
    let start_time: u64 = start_time_str.parse().context(format!(
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
    if nix::unistd::geteuid().as_raw() != 0 {
        log::error!("waypoint-helper must be run as root");
        std::process::exit(1);
    }

    // Initialize configuration
    btrfs::init_config();

    log::info!(
        "Starting Waypoint Helper service v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Build the D-Bus connection
    let helper = WaypointHelper::new();
    let _connection = ConnectionBuilder::system()?
        .name(DBUS_SERVICE_NAME)?
        .serve_at(DBUS_OBJECT_PATH, helper)?
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
fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .context(format!("Failed to run {}", cmd))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow::anyhow!("{} failed: {}", cmd, stderr.trim()))
    }
}

fn run_command_with_output(cmd: &str, args: &[&str]) -> Result<(String, String)> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .context(format!("Failed to run {}", cmd))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() {
        Ok((stdout, stderr))
    } else {
        Err(anyhow::anyhow!("{} failed: {}", cmd, stderr.trim()))
    }
}
