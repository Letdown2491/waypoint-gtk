// Shared types and utilities for Waypoint

pub mod backup_config;
pub mod config;
pub mod exclude;
pub mod quota;
pub mod retention;
pub mod schedules;
pub mod validation;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use backup_config::{
    BackupConfig, BackupDestinationConfig, BackupFilter, BackupRecord, BackupStatus, PendingBackup,
};
pub use config::WaypointConfig;
pub use exclude::{ExcludeConfig, ExcludePattern, PatternType};
pub use quota::{QuotaConfig, QuotaType, QuotaUsage};
pub use retention::{SnapshotForRetention, TimelineRetention};
pub use schedules::{Schedule, ScheduleType, SchedulesConfig};

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

/// Validate snapshot name for security and filesystem compatibility
///
/// # Arguments
/// * `name` - The snapshot name to validate
///
/// # Returns
/// `Ok(())` if the name is valid, `Err` with description if invalid
///
/// # Validation Rules
/// - Name must not be empty and must be ≤ 255 characters
/// - Cannot contain `/`, null bytes, or `..`
/// - Cannot start with `-` or `.`
/// - Cannot be exactly `.` or `..`
///
/// # Security
/// This prevents path traversal attacks and ensures filesystem safety.
/// Even though we use `.arg()` which escapes properly, this provides defense-in-depth.
pub fn validate_snapshot_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Snapshot name cannot be empty".to_string());
    }

    if name.len() > 255 {
        return Err("Snapshot name too long (max 255 characters)".to_string());
    }

    // Reject names with problematic characters
    if name.contains('/') {
        return Err("Snapshot name cannot contain '/'".to_string());
    }

    if name.contains('\0') {
        return Err("Snapshot name cannot contain null bytes".to_string());
    }

    if name.contains("..") {
        return Err("Snapshot name cannot contain '..'".to_string());
    }

    // Reject names starting with - or .
    if name.starts_with('-') {
        return Err("Snapshot name cannot start with '-'".to_string());
    }

    if name.starts_with('.') {
        return Err("Snapshot name cannot start with '.'".to_string());
    }

    // Reject special names
    if name == "." || name == ".." {
        return Err("Snapshot name cannot be '.' or '..'".to_string());
    }

    Ok(())
}
