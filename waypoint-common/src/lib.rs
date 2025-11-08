// Shared types and utilities for Waypoint

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A package installed on the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
}

/// Information about a Btrfs subvolume
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SubvolumeInfo {
    /// Mount point (e.g., "/", "/home")
    pub mount_point: PathBuf,
    /// Subvolume path relative to btrfs root (e.g., "@", "@home")
    pub subvol_path: String,
    /// Subvolume ID
    pub id: u64,
    /// User-friendly name for display
    pub display_name: String,
}

/// User configuration for which subvolumes to snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubvolumeConfig {
    /// List of mount points to include in snapshots
    pub enabled_subvolumes: Vec<PathBuf>,
}

impl Default for SubvolumeConfig {
    fn default() -> Self {
        Self {
            // Default to only root filesystem
            enabled_subvolumes: vec![PathBuf::from("/")],
        }
    }
}

/// Information about a snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub description: Option<String>,
    pub package_count: Option<usize>,
    pub packages: Vec<Package>,
    /// List of subvolumes included in this snapshot (mount points)
    #[serde(default)]
    pub subvolumes: Vec<PathBuf>,
}

/// Result of a snapshot operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
}

impl OperationResult {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
        }
    }
}

/// D-Bus interface constants
pub const DBUS_SERVICE_NAME: &str = "tech.geektoshi.waypoint";
pub const DBUS_OBJECT_PATH: &str = "/tech/geektoshi/waypoint";
pub const DBUS_INTERFACE_NAME: &str = "tech.geektoshi.waypoint.Helper";

/// Polkit action IDs
pub const POLKIT_ACTION_CREATE: &str = "tech.geektoshi.waypoint.create-snapshot";
pub const POLKIT_ACTION_DELETE: &str = "tech.geektoshi.waypoint.delete-snapshot";
pub const POLKIT_ACTION_RESTORE: &str = "tech.geektoshi.waypoint.restore-snapshot";
pub const POLKIT_ACTION_CONFIGURE: &str = "tech.geektoshi.waypoint.configure-system";
