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
use waypoint_common::{BackupConfig, BackupDestinationConfig, SnapshotInfo, WaypointConfig};

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
    ///
    /// # Arguments
    /// * `snapshot` - The snapshot to potentially backup
    /// * `is_favorite` - Whether this snapshot is marked as a favorite
    /// * `all_snapshots` - All snapshots (needed for filters like LastN)
    ///
    /// # Returns
    /// * List of destination UUIDs that were queued
    pub fn queue_snapshot_backup(
        &self,
        snapshot: &SnapshotInfo,
        is_favorite: bool,
        all_snapshots: &[SnapshotInfo],
    ) -> Result<Vec<String>> {
        log::info!("queue_snapshot_backup called for snapshot: {}", snapshot.name);
        log::debug!("  is_favorite: {}, total_snapshots: {}", is_favorite, all_snapshots.len());

        // Collect destinations to backup to (need to avoid borrowing issues)
        let destinations_to_backup: Vec<String> = {
            let config = self.config.lock().unwrap();
            let enabled_count = config.enabled_destinations().count();
            log::debug!("  Found {} enabled destinations", enabled_count);

            config
                .enabled_destinations()
                .filter_map(|(uuid, dest_config)| {
                    log::debug!("  Checking destination: {} ({})", dest_config.label, uuid);
                    log::debug!("    on_snapshot_creation: {}", dest_config.on_snapshot_creation);

                    // Check if this destination wants this snapshot using the new filter matching logic
                    let should_backup = dest_config.filter.matches(snapshot, is_favorite, all_snapshots);
                    log::debug!("    filter.matches: {}", should_backup);

                    let already_backed_up = config.is_backed_up(&snapshot.name, uuid);
                    log::debug!("    already_backed_up: {}", already_backed_up);

                    if should_backup && !already_backed_up {
                        log::info!("  -> Will queue snapshot {} for destination {}", snapshot.name, dest_config.label);
                        Some(uuid.clone())
                    } else {
                        log::debug!("  -> Skipping (should_backup={}, already_backed_up={})", should_backup, already_backed_up);
                        None
                    }
                })
                .collect()
        };

        log::info!("  Queueing snapshot {} for {} destination(s)", snapshot.name, destinations_to_backup.len());

        // Add to pending queue
        {
            let mut config = self.config.lock().unwrap();
            for uuid in &destinations_to_backup {
                log::info!("  Adding pending backup: snapshot={}, destination={}", snapshot.name, uuid);
                config.add_pending_backup(snapshot.name.clone(), uuid.clone());
            }
        }

        if !destinations_to_backup.is_empty() {
            log::info!("Saving backup config with {} new pending backups", destinations_to_backup.len());
            self.save_config()?;
            log::info!("Backup config saved successfully");
        } else {
            log::debug!("No destinations to queue, skipping config save");
        }

        Ok(destinations_to_backup)
    }

    /// Queue snapshots for a specific destination based on its filter
    ///
    /// Called when a drive is mounted with "backup on mount" enabled.
    /// Evaluates the destination's filter against all snapshots and queues those
    /// that should be backed up but aren't already.
    ///
    /// # Arguments
    /// * `destination_uuid` - UUID of the destination
    /// * `all_snapshots` - All available snapshots
    ///
    /// # Returns
    /// * Number of snapshots queued
    pub fn queue_destination_snapshots(
        &self,
        destination_uuid: &str,
        all_snapshots: &[crate::snapshot::Snapshot],
    ) -> Result<usize> {
        log::info!("Evaluating snapshots for destination {}", destination_uuid);

        // Get destination config and check if on_drive_mount is enabled
        let (dest_config, on_drive_mount) = {
            let config = self.config.lock().unwrap();
            match config.get_destination(destination_uuid) {
                Some(dest) => (dest.clone(), dest.on_drive_mount),
                None => {
                    log::warn!("Destination {} not found", destination_uuid);
                    return Ok(0);
                }
            }
        };

        if !on_drive_mount {
            log::debug!("Destination {} does not have on_drive_mount enabled", destination_uuid);
            return Ok(0);
        }

        log::info!("Destination '{}' has backup_on_mount enabled, evaluating {} snapshots",
                   dest_config.label, all_snapshots.len());

        // Convert snapshots to SnapshotInfo
        let snapshot_infos: Vec<waypoint_common::SnapshotInfo> =
            all_snapshots.iter().map(|s| s.into()).collect();

        let mut queued_count = 0;

        // Check each snapshot against the filter
        for snapshot in &snapshot_infos {
            // Check if this snapshot matches the destination's filter
            // Note: favorites are tracked in user_preferences, not in Snapshot struct
            // For now, we pass false for is_favorite (filters like LastN don't need it)
            let is_favorite = false;

            let matches_filter = dest_config.filter.matches(snapshot, is_favorite, &snapshot_infos);

            if !matches_filter {
                continue;
            }

            // Check if already backed up
            let already_backed_up = {
                let config = self.config.lock().unwrap();
                config.is_backed_up(&snapshot.name, destination_uuid)
            };

            if already_backed_up {
                log::debug!("Snapshot {} already backed up to {}", snapshot.name, dest_config.label);
                continue;
            }

            // Queue this snapshot
            log::info!("Queueing snapshot {} for destination {}", snapshot.name, dest_config.label);
            {
                let mut config = self.config.lock().unwrap();
                config.add_pending_backup(snapshot.name.clone(), destination_uuid.to_string());
            }
            queued_count += 1;
        }

        if queued_count > 0 {
            log::info!("Queued {} snapshot(s) for destination {}", queued_count, dest_config.label);
            self.save_config()?;
        } else {
            log::info!("No new snapshots to queue for destination {}", dest_config.label);
        }

        Ok(queued_count)
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

        // Load all snapshots to get their timestamps for sorting
        let snapshot_manager = crate::snapshot::SnapshotManager::new()
            .context("Failed to create snapshot manager")?;
        let all_snapshots = snapshot_manager.load_snapshots()
            .context("Failed to load snapshots")?;

        // Create a map of snapshot_id -> timestamp for quick lookup
        let timestamp_map: std::collections::HashMap<String, chrono::DateTime<chrono::Utc>> =
            all_snapshots.iter()
                .map(|s| (s.name.clone(), s.timestamp))
                .collect();

        // Sort pending snapshot IDs by timestamp (oldest first)
        let mut sorted_snapshot_ids = pending_snapshot_ids;
        sorted_snapshot_ids.sort_by(|a, b| {
            let time_a = timestamp_map.get(a);
            let time_b = timestamp_map.get(b);

            match (time_a, time_b) {
                (Some(ta), Some(tb)) => ta.cmp(tb), // Both have timestamps, compare them
                (Some(_), None) => std::cmp::Ordering::Less, // a has timestamp, b doesn't - a goes first
                (None, Some(_)) => std::cmp::Ordering::Greater, // b has timestamp, a doesn't - b goes first
                (None, None) => a.cmp(b), // Neither has timestamp, sort alphabetically
            }
        });

        log::info!(
            "Processing {} pending backups for destination {} (sorted oldest to newest)",
            sorted_snapshot_ids.len(),
            destination_uuid
        );

        let mut success_count = 0;
        let mut fail_count = 0;
        let mut errors = Vec::new();

        // Process each pending backup (in chronological order)
        for snapshot_id in sorted_snapshot_ids {
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

        // Apply retention policy if configured
        let (retention_days, filter) = {
            let config = self.config.lock().unwrap();
            if let Some(dest) = config.get_destination(destination_uuid) {
                (dest.retention_days, dest.filter.clone())
            } else {
                (None, waypoint_common::BackupFilter::All)
            }
        };

        if let Some(days) = retention_days {
            log::info!(
                "Applying retention policy for destination {}: {} days",
                destination_uuid,
                days
            );

            // Load all snapshots for filter evaluation
            let snapshot_manager = crate::snapshot::SnapshotManager::new()
                .context("Failed to create snapshot manager")?;
            let all_snapshots = snapshot_manager.load_snapshots()
                .context("Failed to load snapshots")?;

            // Convert to SnapshotInfo
            let snapshot_infos: Vec<waypoint_common::SnapshotInfo> =
                all_snapshots.iter().map(|s| s.into()).collect();

            match client.apply_backup_retention(
                destination_mount.to_string(),
                days,
                &filter,
                &snapshot_infos,
            ) {
                Ok(deleted_paths) => {
                    if !deleted_paths.is_empty() {
                        log::info!(
                            "Retention policy deleted {} backups: {:?}",
                            deleted_paths.len(),
                            deleted_paths
                        );
                    }
                }
                Err(e) => {
                    log::error!("Failed to apply retention policy: {}", e);
                    // Don't fail the entire backup operation if retention fails
                    // Just log the error and continue
                }
            }
        }

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

    /// Check if a destination is currently mounted and get its mount point
    pub fn get_mounted_destination(&self, destination_uuid: &str) -> Option<String> {
        use crate::dbus_client::WaypointHelperClient;

        let client = WaypointHelperClient::new().ok()?;
        let (success, result) = client.scan_backup_destinations().ok()?;

        if !success {
            return None;
        }

        // Parse JSON response
        let destinations: Vec<serde_json::Value> = serde_json::from_str(&result).ok()?;

        // Find the destination with matching UUID
        for dest in destinations {
            if let Some(uuid) = dest.get("uuid").and_then(|v| v.as_str()) {
                if uuid == destination_uuid {
                    if let Some(mount_point) = dest.get("mount_point").and_then(|v| v.as_str()) {
                        // Check if it's actually mounted (not "(not connected)")
                        if !mount_point.contains("(not connected)") {
                            return Some(mount_point.to_string());
                        }
                    }
                }
            }
        }

        None
    }

    /// Get backup status summary for display in footer
    pub fn get_backup_status_summary(&self) -> BackupStatusSummary {
        let config = self.config.lock().unwrap();
        let progress = self.progress.lock().unwrap();

        // Count enabled destinations
        let enabled_destinations: Vec<_> = config.enabled_destinations().collect();
        let total_destinations = enabled_destinations.len();

        // If no destinations configured
        if total_destinations == 0 {
            return BackupStatusSummary {
                status_type: BackupStatusType::NotConfigured,
                message: "No backup destinations configured".to_string(),
                clickable: true,
            };
        }

        // Count connected destinations
        let connected_count = enabled_destinations
            .iter()
            .filter(|(uuid, _)| self.get_mounted_destination(uuid).is_some())
            .count();

        // Check for active backups
        if !progress.is_empty() {
            let active_count = progress.len();
            return BackupStatusSummary {
                status_type: BackupStatusType::Active,
                message: if active_count == 1 {
                    "Backing up...".to_string()
                } else {
                    format!("Backing up {} snapshots...", active_count)
                },
                clickable: true,
            };
        }

        // Count pending and failed backups
        let pending_count = config.pending_backups.iter()
            .filter(|pb| matches!(pb.status, waypoint_common::BackupStatus::Pending))
            .count();
        let failed_count = config.pending_backups.iter()
            .filter(|pb| matches!(pb.status, waypoint_common::BackupStatus::Failed))
            .count();

        // Priority: Failed > Pending > Disconnected > Healthy
        if failed_count > 0 {
            return BackupStatusSummary {
                status_type: BackupStatusType::Failed,
                message: if failed_count == 1 {
                    "1 backup failed".to_string()
                } else {
                    format!("{} backups failed", failed_count)
                },
                clickable: true,
            };
        }

        if pending_count > 0 {
            return BackupStatusSummary {
                status_type: BackupStatusType::Pending,
                message: if pending_count == 1 {
                    "1 backup pending".to_string()
                } else {
                    format!("{} backups pending", pending_count)
                },
                clickable: true,
            };
        }

        // Check connection status
        if connected_count == 0 {
            return BackupStatusSummary {
                status_type: BackupStatusType::Disconnected,
                message: format!("All {} destinations disconnected", total_destinations),
                clickable: true,
            };
        } else if connected_count < total_destinations {
            return BackupStatusSummary {
                status_type: BackupStatusType::Disconnected,
                message: format!("{} of {} destinations connected", connected_count, total_destinations),
                clickable: true,
            };
        }

        // All healthy - show last backup time
        let last_backup = config.backup_history.iter()
            .max_by_key(|record| record.completed_at);

        if let Some(record) = last_backup {
            // Convert Unix timestamp to DateTime
            let datetime = chrono::DateTime::from_timestamp(record.completed_at, 0)
                .unwrap_or_else(chrono::Utc::now);
            let relative_time = format_relative_time(datetime);
            BackupStatusSummary {
                status_type: BackupStatusType::Healthy,
                message: format!("All backups current • Last: {}", relative_time),
                clickable: true,
            }
        } else {
            // Have destinations but no backup history yet
            BackupStatusSummary {
                status_type: BackupStatusType::Healthy,
                message: format!("Ready to backup • {} destinations configured", total_destinations),
                clickable: true,
            }
        }
    }
}

/// Type of backup status
#[derive(Debug, Clone, PartialEq)]
pub enum BackupStatusType {
    NotConfigured,
    Healthy,
    Active,
    Pending,
    Failed,
    Disconnected,
}

/// Summary of backup status for UI display
#[derive(Debug, Clone)]
pub struct BackupStatusSummary {
    pub status_type: BackupStatusType,
    pub message: String,
    #[allow(dead_code)] // Will be used when we make the footer clickable
    pub clickable: bool,
}

/// Format a timestamp as relative time (e.g., "2 hours ago")
fn format_relative_time(timestamp: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(timestamp);

    if duration.num_seconds() < 60 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        let mins = duration.num_minutes();
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if duration.num_days() == 1 {
        "yesterday".to_string()
    } else if duration.num_days() < 7 {
        format!("{} days ago", duration.num_days())
    } else if duration.num_weeks() == 1 {
        "1 week ago".to_string()
    } else if duration.num_weeks() < 4 {
        format!("{} weeks ago", duration.num_weeks())
    } else {
        // For older backups, show the date
        timestamp.format("%b %d, %Y").to_string()
    }
}
