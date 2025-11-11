// Waypoint Helper - Privileged D-Bus service for snapshot operations
// This binary runs with elevated privileges via D-Bus activation

use anyhow::{Context, Result};
use tokio::signal::unix::{signal, SignalKind};
use waypoint_common::*;
use zbus::{interface, Connection, ConnectionBuilder};

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
}

impl WaypointHelper {
    fn create_snapshot_impl(name: &str, description: &str, subvolumes: Vec<String>) -> Result<String> {
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

/// Get process start time from /proc/[pid]/stat
fn get_process_start_time(pid: u32) -> Result<u64> {
    use std::fs;

    let stat_path = format!("/proc/{}/stat", pid);
    let stat_content = fs::read_to_string(&stat_path)
        .context(format!("Failed to read {}", stat_path))?;

    // The start time is the 22nd field in /proc/[pid]/stat
    // Fields are: pid (comm) state ppid ... starttime ...
    // We need to handle the (comm) field which may contain spaces

    // Find the last ')' to skip the comm field
    let start_pos = stat_content.rfind(')')
        .context("Invalid /proc/[pid]/stat format")?;

    let fields: Vec<&str> = stat_content[start_pos + 1..]
        .split_whitespace()
        .collect();

    // After skipping (comm), starttime is field 20 (0-indexed 19)
    if fields.len() <= 19 {
        anyhow::bail!("Not enough fields in /proc/[pid]/stat");
    }

    let start_time: u64 = fields[19].parse()
        .context("Failed to parse process start time")?;

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
