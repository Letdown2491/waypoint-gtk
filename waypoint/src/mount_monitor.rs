//! Mount point monitoring for detecting when backup drives are connected
//!
//! Periodically checks for newly mounted filesystems and triggers backup processing

use anyhow::Result;
use gtk::glib;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use crate::dbus_client::WaypointHelperClient;

/// Tracks mounted filesystems and detects changes
pub struct MountMonitor {
    /// Currently mounted UUIDs
    mounted_uuids: Arc<Mutex<HashSet<String>>>,
    /// Prevents logging the same D-Bus error every interval
    last_error_message: Arc<Mutex<Option<String>>>,
    /// Indicates a scan is already running to avoid overlapping work
    scan_in_progress: Arc<AtomicBool>,
}

impl MountMonitor {
    /// Create a new mount monitor
    pub fn new() -> Self {
        Self {
            mounted_uuids: Arc::new(Mutex::new(HashSet::new())),
            last_error_message: Arc::new(Mutex::new(None)),
            scan_in_progress: Arc::new(AtomicBool::new(false)),
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

    /// Check for newly mounted and unmounted filesystems in a single scan
    ///
    /// Returns (new_mounts, unmounted_uuids)
    fn detect_mount_changes(&self) -> Result<(Vec<(String, String)>, Vec<String>)> {
        let destinations = self.scan_destinations()?;
        let mut mounted = self.mounted_uuids.lock().unwrap();
        let mut new_mounts = Vec::new();

        for dest in &destinations {
            if let Some(uuid) = &dest.uuid {
                if !mounted.contains(uuid) {
                    // New mount detected
                    new_mounts.push((uuid.clone(), dest.mount_point.clone()));
                    mounted.insert(uuid.clone());
                }
            }
        }

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

        Ok((new_mounts, unmounted))
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
        let interval_secs = interval_secs.max(5);

        let monitor = Arc::new(self);
        let callback: Rc<dyn Fn(String, String) + 'static> = Rc::new(callback);

        // Schedule periodic checks using glib
        gtk::glib::timeout_add_seconds_local(interval_secs as u32, {
            let monitor = monitor.clone();
            let callback = callback.clone();

            move || {
                if monitor.scan_in_progress.swap(true, Ordering::SeqCst) {
                    log::debug!("Mount monitor scan already running; skipping this interval");
                    return gtk::glib::ControlFlow::Continue;
                }

                // Run the scan on a background thread so we don't block the UI if
                // waypoint-helper is busy performing rsync work.
                let (tx, rx) = mpsc::channel();
                std::thread::spawn({
                    let monitor = monitor.clone();
                    let scan_guard_flag = monitor.scan_in_progress.clone();
                    move || {
                        let _guard = ScanGuard::new(scan_guard_flag);
                        let result = monitor.detect_mount_changes();
                        let _ = tx.send(result);
                    }
                });

                let cb = callback.clone();
                let monitor_for_future = monitor.clone();
                gtk::glib::spawn_future_local(async move {
                    let result = loop {
                        match rx.try_recv() {
                            Ok(result) => break result,
                            Err(mpsc::TryRecvError::Empty) => {
                                glib::timeout_future(std::time::Duration::from_millis(50)).await;
                                continue;
                            }
                            Err(mpsc::TryRecvError::Disconnected) => {
                                log::error!("Mount monitor thread disconnected unexpectedly");
                                return;
                            }
                        }
                    };

                    match result {
                        Ok((new_mounts, unmounted)) => {
                            for (uuid, mount_point) in new_mounts {
                                log::info!("Detected new backup drive: {uuid} at {mount_point}");
                                cb(uuid, mount_point);
                            }

                            for uuid in unmounted {
                                log::info!("Backup drive unmounted: {uuid}");
                            }

                            monitor_for_future.clear_scan_error();
                        }
                        Err(e) => {
                            monitor_for_future
                                .log_scan_error(&format!("Failed to scan for mounts: {e}"));
                        }
                    }
                });

                gtk::glib::ControlFlow::Continue
            }
        });
    }
}

impl MountMonitor {
    fn log_scan_error(&self, message: &str) {
        let mut last = self.last_error_message.lock().unwrap();
        if last.as_deref() != Some(message) {
            log::error!("{message}");
            *last = Some(message.to_string());
        } else {
            log::debug!("{message}");
        }
    }

    fn clear_scan_error(&self) {
        let mut last = self.last_error_message.lock().unwrap();
        if last.is_some() {
            log::info!("Mount monitor reconnected to system bus");
            *last = None;
        }
    }
}

struct ScanGuard {
    flag: Arc<AtomicBool>,
}

impl ScanGuard {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }
}

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
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
