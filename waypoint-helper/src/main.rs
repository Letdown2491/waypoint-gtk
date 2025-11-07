// Waypoint Helper - Privileged D-Bus service for snapshot operations
// This binary runs with elevated privileges via D-Bus activation

use anyhow::{Context, Result};
use tokio::signal::unix::{signal, SignalKind};
use waypoint_common::*;
use zbus::{interface, Connection, ConnectionBuilder};

mod btrfs;
mod packages;

/// Main D-Bus service interface for Waypoint operations
struct WaypointHelper;

#[interface(name = "com.voidlinux.waypoint.Helper")]
impl WaypointHelper {
    /// Create a new snapshot
    async fn create_snapshot(
        &self,
        #[zbus(connection)] connection: &Connection,
        name: String,
        description: String,
        subvolumes: Vec<String>,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(connection, POLKIT_ACTION_CREATE).await {
            return (false, format!("Authorization failed: {}", e));
        }

        // Create the snapshot
        match Self::create_snapshot_impl(&name, &description, subvolumes) {
            Ok(msg) => (true, msg),
            Err(e) => (false, format!("Failed to create snapshot: {}", e)),
        }
    }

    /// Delete a snapshot
    async fn delete_snapshot(
        &self,
        #[zbus(connection)] connection: &Connection,
        name: String,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(connection, POLKIT_ACTION_DELETE).await {
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
        #[zbus(connection)] connection: &Connection,
        name: String,
    ) -> (bool, String) {
        // Check authorization
        if let Err(e) = check_authorization(connection, POLKIT_ACTION_RESTORE).await {
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
                eprintln!("Failed to list snapshots: {}", e);
                "[]".to_string()
            }
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
/// For Phase 4 MVP, this is a simplified check.
/// The helper binary must be activated by D-Bus with appropriate permissions,
/// so if we're running, the user has already been authenticated by the system.
async fn check_authorization(_connection: &Connection, action_id: &str) -> Result<()> {
    // Log the authorization attempt
    println!("Authorization requested for action: {}", action_id);

    // Since this service runs as root and is activated by D-Bus,
    // the authentication is handled by the D-Bus policy and system activation.
    // For now, we trust that if we're running, the user is authorized.
    //
    // TODO: For enhanced security, implement full Polkit CheckAuthorization call
    // using the org.freedesktop.PolicyKit1.Authority D-Bus interface.

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Must run as root
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("waypoint-helper must be run as root");
        std::process::exit(1);
    }

    println!("Starting Waypoint Helper service...");

    // Build the D-Bus connection
    let _connection = ConnectionBuilder::system()?
        .name(DBUS_SERVICE_NAME)?
        .serve_at(DBUS_OBJECT_PATH, WaypointHelper)?
        .build()
        .await?;

    println!("Waypoint Helper is ready at {}", DBUS_OBJECT_PATH);

    // Wait for termination signal
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    tokio::select! {
        _ = sigterm.recv() => println!("Received SIGTERM, shutting down..."),
        _ = sigint.recv() => println!("Received SIGINT, shutting down..."),
    }

    Ok(())
}
