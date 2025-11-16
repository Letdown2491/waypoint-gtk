//! Mount point monitoring for detecting when backup drives are connected
//!
//! Periodically checks for newly mounted filesystems and triggers backup processing

use anyhow::Result;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::dbus_client::WaypointHelperClient;

/// Tracks mounted filesystems and detects changes
pub struct MountMonitor {
    /// Currently mounted UUIDs
    mounted_uuids: Arc<Mutex<HashSet<String>>>,
}

impl MountMonitor {
    /// Create a new mount monitor
    pub fn new() -> Self {
        Self {
            mounted_uuids: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Initialize the monitor with currently mounted filesystems
    pub fn initialize(&self) -> Result<()> {
        let destinations = self.scan_destinations()?;
        let mut mounted = self.mounted_uuids.lock().unwrap();

        for dest in destinations {
            if let Some(uuid) = dest.uuid {
                mounted.insert(uuid);
            }
        }

        Ok(())
    }

    /// Check for newly mounted filesystems
    ///
    /// Returns a list of newly mounted destination UUIDs and their mount points
    pub fn check_for_new_mounts(&self) -> Result<Vec<(String, String)>> {
        let destinations = self.scan_destinations()?;
        let mut mounted = self.mounted_uuids.lock().unwrap();
        let mut new_mounts = Vec::new();

        for dest in destinations {
            if let Some(uuid) = &dest.uuid {
                if !mounted.contains(uuid) {
                    // New mount detected
                    new_mounts.push((uuid.clone(), dest.mount_point.clone()));
                    mounted.insert(uuid.clone());
                }
            }
        }

        Ok(new_mounts)
    }

    /// Check for unmounted filesystems and remove them from tracking
    pub fn check_for_unmounts(&self) -> Result<Vec<String>> {
        let destinations = self.scan_destinations()?;
        let mut mounted = self.mounted_uuids.lock().unwrap();

        // Get currently mounted UUIDs
        let current_uuids: HashSet<String> =
            destinations.into_iter().filter_map(|d| d.uuid).collect();

        // Find UUIDs that are no longer mounted
        let unmounted: Vec<String> = mounted
            .iter()
            .filter(|uuid| !current_uuids.contains(*uuid))
            .cloned()
            .collect();

        // Remove unmounted UUIDs
        for uuid in &unmounted {
            mounted.remove(uuid);
        }

        Ok(unmounted)
    }

    /// Scan for backup destinations using D-Bus
    fn scan_destinations(&self) -> Result<Vec<BackupDestination>> {
        let client = WaypointHelperClient::new()?;
        let (success, result) = client.scan_backup_destinations()?;

        if !success {
            return Err(anyhow::anyhow!(result));
        }

        let destinations: Vec<BackupDestination> = serde_json::from_str(&result)?;
        Ok(destinations)
    }

    /// Start monitoring in the background (using GTK's main loop)
    ///
    /// Calls the callback whenever a new mount is detected
    pub fn start_monitoring<F>(self, interval_secs: u64, callback: F)
    where
        F: Fn(String, String) + 'static,
    {
        let monitor = Arc::new(self);
        let callback = Arc::new(callback);

        // Schedule periodic checks using glib
        gtk::glib::timeout_add_seconds_local(interval_secs as u32, {
            let monitor = monitor.clone();
            let callback = callback.clone();

            move || {
                // Check for new mounts
                if let Ok(new_mounts) = monitor.check_for_new_mounts() {
                    for (uuid, mount_point) in new_mounts {
                        log::info!("Detected new backup drive: {} at {}", uuid, mount_point);
                        callback(uuid, mount_point);
                    }
                }

                // Check for unmounts (just for cleanup)
                if let Ok(unmounted) = monitor.check_for_unmounts() {
                    for uuid in unmounted {
                        log::info!("Backup drive unmounted: {}", uuid);
                    }
                }

                gtk::glib::ControlFlow::Continue
            }
        });
    }
}

/// Simplified backup destination struct for deserialization
#[derive(Debug, Clone, serde::Deserialize)]
struct BackupDestination {
    mount_point: String,
    #[allow(dead_code)]
    label: String,
    #[allow(dead_code)]
    drive_type: DriveType,
    uuid: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
enum DriveType {
    Removable,
    Network,
    Internal,
}
