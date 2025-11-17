//! Backup configuration and state management
//!
//! Handles automatic backup configuration, pending backup queue, and backup history

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Filter for which snapshots to backup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BackupFilter {
    /// Backup all snapshots
    All,
    /// Only backup snapshots marked as favorites
    Favorites,
}

impl Default for BackupFilter {
    fn default() -> Self {
        Self::All
    }
}

/// Configuration for a single backup destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDestinationConfig {
    /// Filesystem UUID for reliable identification
    pub uuid: String,

    /// Human-readable label
    pub label: String,

    /// Last known mount point (for reference)
    #[serde(default)]
    pub last_mount_point: String,

    /// Filesystem type (btrfs, ntfs, exfat, vfat, cifs, nfs, etc.)
    #[serde(default)]
    pub fstype: String,

    /// Whether automatic backups are enabled for this destination
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Which snapshots to backup
    #[serde(default)]
    pub filter: BackupFilter,

    /// Backup when new snapshots are created
    #[serde(default = "default_true")]
    pub on_snapshot_creation: bool,

    /// Backup when drive is mounted
    #[serde(default = "default_true")]
    pub on_drive_mount: bool,

    /// Retention days (optional, None means keep all backups)
    #[serde(default)]
    pub retention_days: Option<u32>,
}

fn default_true() -> bool {
    true
}

/// Status of a pending backup
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    /// Waiting to be backed up
    Pending,
    /// Currently being backed up
    InProgress,
    /// Successfully backed up
    Completed,
    /// Failed with error
    Failed,
}

/// A pending backup operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingBackup {
    /// Snapshot ID to backup
    pub snapshot_id: String,

    /// Destination UUID
    pub destination_uuid: String,

    /// Current status
    pub status: BackupStatus,

    /// Timestamp when this was queued (Unix timestamp)
    pub queued_at: i64,

    /// Number of retry attempts
    #[serde(default)]
    pub retry_count: u32,

    /// Last error message if failed
    #[serde(default)]
    pub last_error: Option<String>,

    /// Timestamp of last attempt (Unix timestamp)
    #[serde(default)]
    pub last_attempt: Option<i64>,
}

/// Record of a completed backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    /// Snapshot ID that was backed up
    pub snapshot_id: String,

    /// Destination UUID
    pub destination_uuid: String,

    /// Backup path on destination
    pub backup_path: String,

    /// Timestamp when backup completed (Unix timestamp)
    pub completed_at: i64,

    /// Size of the backup in bytes
    #[serde(default)]
    pub size_bytes: Option<u64>,

    /// Whether this was an incremental backup
    #[serde(default)]
    pub is_incremental: bool,

    /// Parent snapshot ID if incremental
    #[serde(default)]
    pub parent_snapshot_id: Option<String>,
}

/// Main backup configuration and state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackupConfig {
    /// Configured backup destinations
    #[serde(default)]
    pub destinations: HashMap<String, BackupDestinationConfig>,

    /// Queue of pending backups
    #[serde(default)]
    pub pending_backups: Vec<PendingBackup>,

    /// History of completed backups
    #[serde(default)]
    pub backup_history: Vec<BackupRecord>,

    /// Mount check interval in seconds (default: 60)
    #[serde(default = "default_mount_check_interval")]
    pub mount_check_interval_seconds: u64,
}

fn default_mount_check_interval() -> u64 {
    60
}

impl BackupConfig {
    /// Get the default config file path (~/.config/waypoint/backup-config.toml)
    pub fn default_path() -> anyhow::Result<PathBuf> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;

        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("waypoint");
        path.push("backup-config.toml");

        Ok(path)
    }

    /// Load configuration from default path
    pub fn load_from_default() -> anyhow::Result<Self> {
        let path = Self::default_path()?;
        Self::load(&path)
    }

    /// Save configuration to default path
    pub fn save_to_default(&self) -> anyhow::Result<()> {
        let path = Self::default_path()?;
        self.save(&path)
    }

    /// Load configuration from file
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Add or update a destination configuration
    pub fn add_destination(&mut self, uuid: String, config: BackupDestinationConfig) {
        self.destinations.insert(uuid, config);
    }

    /// Remove a destination configuration
    pub fn remove_destination(&mut self, uuid: &str) {
        self.destinations.remove(uuid);
    }

    /// Get enabled destinations
    pub fn enabled_destinations(
        &self,
    ) -> impl Iterator<Item = (&String, &BackupDestinationConfig)> {
        self.destinations
            .iter()
            .filter(|(_, config)| config.enabled)
    }

    /// Add a pending backup
    pub fn add_pending_backup(&mut self, snapshot_id: String, destination_uuid: String) {
        // Check if already exists
        if self
            .pending_backups
            .iter()
            .any(|pb| pb.snapshot_id == snapshot_id && pb.destination_uuid == destination_uuid)
        {
            return;
        }

        let pending = PendingBackup {
            snapshot_id,
            destination_uuid,
            status: BackupStatus::Pending,
            queued_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            retry_count: 0,
            last_error: None,
            last_attempt: None,
        };

        self.pending_backups.push(pending);
    }

    /// Get pending backups for a destination
    pub fn pending_for_destination(&self, uuid: &str) -> Vec<&PendingBackup> {
        self.pending_backups
            .iter()
            .filter(|pb| pb.destination_uuid == uuid && pb.status == BackupStatus::Pending)
            .collect()
    }

    /// Mark a backup as completed
    pub fn mark_completed(
        &mut self,
        snapshot_id: &str,
        destination_uuid: &str,
        backup_path: String,
        size_bytes: Option<u64>,
        is_incremental: bool,
        parent_snapshot_id: Option<String>,
    ) {
        // Remove from pending
        self.pending_backups.retain(|pb| {
            !(pb.snapshot_id == snapshot_id && pb.destination_uuid == destination_uuid)
        });

        // Add to history
        let record = BackupRecord {
            snapshot_id: snapshot_id.to_string(),
            destination_uuid: destination_uuid.to_string(),
            backup_path,
            completed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            size_bytes,
            is_incremental,
            parent_snapshot_id,
        };

        self.backup_history.push(record);
    }

    /// Mark a backup as failed
    pub fn mark_failed(&mut self, snapshot_id: &str, destination_uuid: &str, error: String) {
        if let Some(pending) = self
            .pending_backups
            .iter_mut()
            .find(|pb| pb.snapshot_id == snapshot_id && pb.destination_uuid == destination_uuid)
        {
            pending.status = BackupStatus::Failed;
            pending.last_error = Some(error);
            pending.retry_count += 1;
            pending.last_attempt = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            );
        }
    }

    /// Reset a failed backup to pending (for retry)
    pub fn retry_backup(&mut self, snapshot_id: &str, destination_uuid: &str) {
        if let Some(pending) = self
            .pending_backups
            .iter_mut()
            .find(|pb| pb.snapshot_id == snapshot_id && pb.destination_uuid == destination_uuid)
        {
            pending.status = BackupStatus::Pending;
        }
    }

    /// Check if a snapshot is already backed up to a destination
    pub fn is_backed_up(&self, snapshot_id: &str, destination_uuid: &str) -> bool {
        self.backup_history.iter().any(|record| {
            record.snapshot_id == snapshot_id && record.destination_uuid == destination_uuid
        })
    }

    /// Get the latest backup for a snapshot on a destination (for incremental backup parent)
    pub fn get_latest_backup(&self, destination_uuid: &str) -> Option<&BackupRecord> {
        self.backup_history
            .iter()
            .filter(|r| r.destination_uuid == destination_uuid)
            .max_by_key(|r| r.completed_at)
    }

    /// Get backup history for a snapshot
    pub fn get_snapshot_backups(&self, snapshot_id: &str) -> Vec<&BackupRecord> {
        self.backup_history
            .iter()
            .filter(|r| r.snapshot_id == snapshot_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_pending_backup() {
        let mut config = BackupConfig::default();
        config.add_pending_backup("snap1".to_string(), "uuid1".to_string());

        assert_eq!(config.pending_backups.len(), 1);
        assert_eq!(config.pending_backups[0].snapshot_id, "snap1");
        assert_eq!(config.pending_backups[0].destination_uuid, "uuid1");

        // Adding duplicate should not create another entry
        config.add_pending_backup("snap1".to_string(), "uuid1".to_string());
        assert_eq!(config.pending_backups.len(), 1);
    }

    #[test]
    fn test_mark_completed() {
        let mut config = BackupConfig::default();
        config.add_pending_backup("snap1".to_string(), "uuid1".to_string());

        config.mark_completed(
            "snap1",
            "uuid1",
            "/backup/snap1".to_string(),
            Some(1024),
            false,
            None,
        );

        assert_eq!(config.pending_backups.len(), 0);
        assert_eq!(config.backup_history.len(), 1);
        assert_eq!(config.backup_history[0].snapshot_id, "snap1");
    }

    #[test]
    fn test_is_backed_up() {
        let mut config = BackupConfig::default();
        config.mark_completed(
            "snap1",
            "uuid1",
            "/backup/snap1".to_string(),
            Some(1024),
            false,
            None,
        );

        assert!(config.is_backed_up("snap1", "uuid1"));
        assert!(!config.is_backed_up("snap1", "uuid2"));
        assert!(!config.is_backed_up("snap2", "uuid1"));
    }
}
