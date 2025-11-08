// D-Bus client for communicating with waypoint-helper

use anyhow::{Context, Result};
use waypoint_common::*;
use zbus::blocking::Connection as BlockingConnection;
use zbus::Connection as AsyncConnection;

/// Client for waypoint-helper D-Bus service
pub struct WaypointHelperClient {
    connection: BlockingConnection,
    async_connection: AsyncConnection,
}

impl WaypointHelperClient {
    /// Connect to the waypoint-helper service (blocking)
    pub fn new() -> Result<Self> {
        let connection = BlockingConnection::system()
            .context("Failed to connect to system bus")?;

        // Create an async connection by using futures executor
        let async_connection = futures_executor::block_on(async {
            AsyncConnection::system().await
        }).context("Failed to create async connection")?;

        Ok(Self { connection, async_connection })
    }

    /// Connect to the waypoint-helper service (async)
    pub async fn new_async() -> Result<Self> {
        let connection = BlockingConnection::system()
            .context("Failed to connect to system bus")?;

        let async_connection = AsyncConnection::system().await
            .context("Failed to create async connection")?;

        Ok(Self { connection, async_connection })
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
            .call("create_snapshot", &(name, description, subvolumes))
            .context("Failed to call create_snapshot")?;

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
            .call("delete_snapshot", &(name,))
            .context("Failed to call delete_snapshot")?;

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
            .call("restore_snapshot", &(name,))
            .context("Failed to call restore_snapshot")?;

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
            .call("list_snapshots", &())
            .context("Failed to call list_snapshots")?;

        let snapshots: Vec<SnapshotInfo> = serde_json::from_str(&json)
            .context("Failed to parse snapshot list")?;

        Ok(snapshots)
    }

    /// Update scheduler configuration
    pub async fn update_scheduler_config(&self, config_content: String) -> Result<(bool, String)> {
        let proxy = zbus::Proxy::new(
            &self.async_connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        ).await?;

        let result: (bool, String) = proxy
            .call("update_scheduler_config", &(config_content,))
            .await
            .context("Failed to call update_scheduler_config")?;

        Ok(result)
    }

    /// Restart scheduler service
    pub async fn restart_scheduler(&self) -> Result<(bool, String)> {
        let proxy = zbus::Proxy::new(
            &self.async_connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        ).await?;

        let result: (bool, String) = proxy
            .call("restart_scheduler", &())
            .await
            .context("Failed to call restart_scheduler")?;

        Ok(result)
    }

    /// Get scheduler service status
    pub async fn get_scheduler_status(&self) -> Result<String> {
        let proxy = zbus::Proxy::new(
            &self.async_connection,
            DBUS_SERVICE_NAME,
            DBUS_OBJECT_PATH,
            DBUS_INTERFACE_NAME,
        ).await?;

        let status: String = proxy
            .call("get_scheduler_status", &())
            .await
            .context("Failed to call get_scheduler_status")?;

        Ok(status)
    }
}
