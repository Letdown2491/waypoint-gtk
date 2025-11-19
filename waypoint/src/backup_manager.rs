//! Backup manager for coordinating automatic backups
//!
//! Handles:
//! - Loading/saving backup configuration
//! - Queueing backups when snapshots are created
//! - Processing backup queue when drives are mounted
//! - Tracking backup history

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use waypoint_common::{BackupConfig, BackupDestinationConfig, BackupFilter, WaypointConfig};

use crate::dbus_client::WaypointHelperClient;
use crate::signal_listener::BackupProgressEvent;

/// Live progress information for a backup
#[derive(Clone, Debug)]
pub struct LiveBackupProgress {
    pub stage: String,
    /// Transferred bytes - tracked but not yet displayed in UI
    #[allow(dead_code)]
    pub bytes_transferred: u64,
    /// Total bytes - tracked but not yet displayed in UI
    #[allow(dead_code)]
    pub total_bytes: u64,
    /// Transfer speed - tracked but not yet displayed in UI
    #[allow(dead_code)]
    pub speed_bytes_per_sec: u64,
}

/// Manages automatic backups
#[derive(Clone)]
pub struct BackupManager {
    config: Arc<Mutex<BackupConfig>>,
    config_path: PathBuf,
    /// Live progress tracking: (snapshot_id, destination_uuid) -> progress
    progress: Arc<Mutex<HashMap<(String, String), LiveBackupProgress>>>,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new() -> Result<Self> {
        let waypoint_config = WaypointConfig::new();
        let config_path = waypoint_config.backup_config.clone();

        let backup_config =
            BackupConfig::load(&config_path).unwrap_or_else(|_| BackupConfig::default());

        Ok(Self {
            config: Arc::new(Mutex::new(backup_config)),
            config_path,
            progress: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Get the current backup configuration
    pub fn get_config(&self) -> Result<BackupConfig> {
        let config = self.config.lock().unwrap();
        Ok(config.clone())
    }

    /// Save the current configuration to disk
    pub fn save_config(&self) -> Result<()> {
        let config = self.config.lock().unwrap();
        config
            .save(&self.config_path)
            .with_context(|| {
                format!("Failed to save backup configuration to {:?}", self.config_path)
            })?;
        Ok(())
    }

    /// Add or update a destination configuration
    pub fn add_destination(
        &self,
        uuid: String,
        dest_config: BackupDestinationConfig,
    ) -> Result<()> {
        let mut config = self.config.lock().unwrap();
        config.add_destination(uuid, dest_config);
        drop(config);
        self.save_config()?;
        Ok(())
    }

    /// Remove a destination configuration
    pub fn remove_destination(&self, uuid: &str) -> Result<()> {
        let mut config = self.config.lock().unwrap();
        config.remove_destination(uuid);
        drop(config);
        self.save_config()?;
        Ok(())
    }

    /// Update mount check interval
    pub fn set_mount_check_interval(&self, interval_seconds: u64) -> Result<()> {
        let mut config = self.config.lock().unwrap();
        config.mount_check_interval_seconds = interval_seconds;
        drop(config);
        self.save_config()?;
        Ok(())
    }

    /// Queue a snapshot for backup to all enabled destinations
    ///
    /// Called when a new snapshot is created or when manually requested
    pub fn queue_snapshot_backup(&self, snapshot_id: String, is_favorite: bool) -> Result<()> {
        // Collect destinations to backup to (need to avoid borrowing issues)
        let destinations_to_backup: Vec<String> = {
            let config = self.config.lock().unwrap();
            config
                .enabled_destinations()
                .filter_map(|(uuid, dest_config)| {
                    // Check if this destination wants this snapshot
                    let should_backup = match dest_config.filter {
                        BackupFilter::All => true,
                        BackupFilter::Favorites => is_favorite,
                    };

                    if should_backup && !config.is_backed_up(&snapshot_id, uuid) {
                        Some(uuid.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Add to pending queue
        {
            let mut config = self.config.lock().unwrap();
            for uuid in destinations_to_backup {
                config.add_pending_backup(snapshot_id.clone(), uuid);
            }
        }

        self.save_config()?;
        Ok(())
    }

    /// Process pending backups for a specific destination (when drive is mounted)
    ///
    /// Returns: (successful_count, failed_count, errors)
    pub fn process_pending_backups(
        &self,
        destination_uuid: &str,
        destination_mount: &str,
        snapshot_dir: &str,
    ) -> Result<(usize, usize, Vec<String>)> {
        let client = WaypointHelperClient::new().context("Failed to connect to waypoint-helper")?;

        // Collect pending snapshot IDs (need to clone to avoid borrowing issues)
        let pending_snapshot_ids: Vec<String> = {
            let config = self.config.lock().unwrap();
            config
                .pending_for_destination(destination_uuid)
                .iter()
                .map(|pb| pb.snapshot_id.clone())
                .collect()
        };

        if pending_snapshot_ids.is_empty() {
            return Ok((0, 0, Vec::new()));
        }

        let mut success_count = 0;
        let mut fail_count = 0;
        let mut errors = Vec::new();

        // Process each pending backup
        for snapshot_id in pending_snapshot_ids {
            // Build snapshot path
            let snapshot_path = PathBuf::from(snapshot_dir).join(&snapshot_id);

            // Determine parent for incremental backup
            let parent_snapshot = {
                let config = self.config.lock().unwrap();
                config
                    .get_latest_backup(destination_uuid)
                    .map(|r| PathBuf::from(snapshot_dir).join(&r.snapshot_id))
            };

            // Perform backup
            let parent_str = parent_snapshot
                .as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or("")
                .to_string();

            match client.backup_snapshot(
                snapshot_path.to_string_lossy().to_string(),
                destination_mount.to_string(),
                parent_str,
            ) {
                Ok((true, backup_path, size_bytes)) => {
                    // Mark as completed
                    let mut config = self.config.lock().unwrap();
                    config.mark_completed(
                        &snapshot_id,
                        destination_uuid,
                        backup_path,
                        Some(size_bytes),
                        parent_snapshot.is_some(),
                        parent_snapshot.as_ref().and_then(|p| {
                            p.file_name()
                                .and_then(|n| n.to_str())
                                .map(|s| s.to_string())
                        }),
                    );
                    success_count += 1;
                }
                Ok((false, error_msg, _)) => {
                    // D-Bus call succeeded but backup failed
                    let mut config = self.config.lock().unwrap();
                    config.mark_failed(&snapshot_id, destination_uuid, error_msg.clone());
                    fail_count += 1;
                    errors.push(format!("{snapshot_id}: {error_msg}"));
                }
                Err(e) => {
                    // D-Bus call failed
                    let error = e.to_string();
                    let mut config = self.config.lock().unwrap();
                    config.mark_failed(&snapshot_id, destination_uuid, error.clone());
                    fail_count += 1;
                    errors.push(format!("{snapshot_id}: {error}"));
                }
            }
        }

        self.save_config()?;

        Ok((success_count, fail_count, errors))
    }

    /// Retry failed backups for a destination
    pub fn retry_failed_backups(&self, destination_uuid: &str) -> Result<()> {
        let mut config = self.config.lock().unwrap();

        // Find all failed backups for this destination
        let failed_snapshots: Vec<String> = config
            .pending_backups
            .iter()
            .filter(|pb| {
                pb.destination_uuid == destination_uuid
                    && pb.status == waypoint_common::BackupStatus::Failed
            })
            .map(|pb| pb.snapshot_id.clone())
            .collect();

        // Reset them to pending
        for snapshot_id in failed_snapshots {
            config.retry_backup(&snapshot_id, destination_uuid);
        }

        drop(config);
        self.save_config()?;
        Ok(())
    }

    /// Get count of pending backups for a destination
    pub fn get_pending_count(&self, destination_uuid: &str) -> usize {
        let config = self.config.lock().unwrap();
        config.pending_for_destination(destination_uuid).len()
    }

    /// Check if a snapshot is backed up to any destination
    pub fn is_snapshot_backed_up(&self, snapshot_id: &str) -> bool {
        let config = self.config.lock().unwrap();
        config
            .enabled_destinations()
            .any(|(uuid, _)| config.is_backed_up(snapshot_id, uuid))
    }

    /// Get list of destinations where a snapshot is backed up
    pub fn get_snapshot_backup_destinations(&self, snapshot_id: &str) -> Vec<String> {
        let config = self.config.lock().unwrap();
        config
            .get_snapshot_backups(snapshot_id)
            .iter()
            .map(|record| record.destination_uuid.clone())
            .collect()
    }

    /// Update progress for a backup
    pub fn update_progress(&self, event: BackupProgressEvent) {
        let mut progress = self.progress.lock().unwrap();
        let key = (event.snapshot_id.clone(), event.destination_uuid.clone());

        // Remove completed backups from progress tracking
        if event.stage == "complete" {
            progress.remove(&key);
        } else {
            progress.insert(key, LiveBackupProgress {
                stage: event.stage,
                bytes_transferred: event.bytes_transferred,
                total_bytes: event.total_bytes,
                speed_bytes_per_sec: event.speed_bytes_per_sec,
            });
        }
    }

    /// Get progress for a specific backup
    pub fn get_progress(&self, snapshot_id: &str, destination_uuid: &str) -> Option<LiveBackupProgress> {
        let progress = self.progress.lock().unwrap();
        let key = (snapshot_id.to_string(), destination_uuid.to_string());
        progress.get(&key).cloned()
    }
}
