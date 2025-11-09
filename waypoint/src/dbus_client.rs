// D-Bus client for communicating with waypoint-helper

use anyhow::{Context, Result};
use waypoint_common::*;
use zbus::blocking::Connection as BlockingConnection;

/// Verification result for a snapshot
#[derive(Debug, serde::Deserialize)]
pub struct VerificationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Package change information for restore preview
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PackageChange {
    pub name: String,
    pub current_version: Option<String>,
    pub snapshot_version: Option<String>,
    pub change_type: String,
}

/// Preview of what will happen if a snapshot is restored
#[derive(Debug, serde::Deserialize)]
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

/// Client for waypoint-helper D-Bus service
pub struct WaypointHelperClient {
    connection: BlockingConnection,
}

impl WaypointHelperClient {
    /// Connect to the waypoint-helper service (blocking)
    pub fn new() -> Result<Self> {
        let connection = BlockingConnection::system()
            .context("Failed to connect to system bus")?;

        Ok(Self { connection })
    }

    /// Create a snapshot
    pub fn create_snapshot(&self, name: String, description: String, subvolumes: Vec<String>) -> Result<(bool, String)> {
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

    /// Delete a snapshot
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

    /// Restore a snapshot (rollback)
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

        let snapshots: Vec<SnapshotInfo> = serde_json::from_str(&json)
            .context("Failed to parse snapshot list")?;

        Ok(snapshots)
    }

    /// Verify snapshot integrity
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

        let result: VerificationResult = serde_json::from_str(&json)
            .context("Failed to parse verification result")?;

        Ok(result)
    }

    /// Preview what will happen if a snapshot is restored
    pub fn preview_restore(&self, name: String) -> Result<RestorePreview> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let json: String = proxy
            .call("PreviewRestore", &(name,))
            .context("Failed to call PreviewRestore")?;

        let result: RestorePreview = serde_json::from_str(&json)
            .context("Failed to parse restore preview result")?;

        Ok(result)
    }

    /// Update scheduler configuration
    pub fn update_scheduler_config(&self, config_content: String) -> Result<(bool, String)> {
        let proxy = zbus::blocking::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )?;

        let result: (bool, String) = proxy
            .call("UpdateSchedulerConfig", &(config_content,))
            .context("Failed to call UpdateSchedulerConfig")?;

        Ok(result)
    }

    /// Restart scheduler service
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

    /// Get scheduler service status
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
}
