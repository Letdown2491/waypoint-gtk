// D-Bus client for communicating with waypoint-helper

use anyhow::{Context, Result};
use waypoint_common::*;
use zbus::Connection;

/// Client for waypoint-helper D-Bus service
pub struct WaypointHelperClient {
    connection: Connection,
}

impl WaypointHelperClient {
    /// Connect to the waypoint-helper service
    pub async fn new() -> Result<Self> {
        let connection = Connection::system()
            .await
            .context("Failed to connect to system bus")?;

        Ok(Self { connection })
    }

    /// Create a snapshot
    pub async fn create_snapshot(&self, name: String, description: String, subvolumes: Vec<String>) -> Result<(bool, String)> {
        let proxy = zbus::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )
        .await?;

        let result: (bool, String) = proxy
            .call("create_snapshot", &(name, description, subvolumes))
            .await
            .context("Failed to call create_snapshot")?;

        Ok(result)
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&self, name: String) -> Result<(bool, String)> {
        let proxy = zbus::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )
        .await?;

        let result: (bool, String) = proxy
            .call("delete_snapshot", &(name,))
            .await
            .context("Failed to call delete_snapshot")?;

        Ok(result)
    }

    /// Restore a snapshot (rollback)
    pub async fn restore_snapshot(&self, name: String) -> Result<(bool, String)> {
        let proxy = zbus::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )
        .await?;

        let result: (bool, String) = proxy
            .call("restore_snapshot", &(name,))
            .await
            .context("Failed to call restore_snapshot")?;

        Ok(result)
    }

    /// List all snapshots
    #[allow(dead_code)]
    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>> {
        let proxy = zbus::Proxy::new(
            &self.connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        )
        .await?;

        let json: String = proxy
            .call("list_snapshots", &())
            .await
            .context("Failed to call list_snapshots")?;

        let snapshots: Vec<SnapshotInfo> = serde_json::from_str(&json)
            .context("Failed to parse snapshot list")?;

        Ok(snapshots)
    }
}
