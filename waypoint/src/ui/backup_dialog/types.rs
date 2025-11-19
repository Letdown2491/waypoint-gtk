//! Types used across backup dialog modules

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DriveType {
    Removable,
    Network,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDestination {
    pub mount_point: String,
    pub label: String,
    pub drive_type: DriveType,
    pub uuid: Option<String>,
    pub fstype: String, // Filesystem type (btrfs, ntfs, exfat, etc.)
}

/// Result of verifying all backups on a destination
#[derive(Debug)]
pub struct VerificationResults {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub details: Vec<(String, bool, String)>, // (snapshot_id, success, message)
}
