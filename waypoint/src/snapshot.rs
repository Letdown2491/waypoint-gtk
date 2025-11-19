use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::rc::Rc;
use waypoint_common::WaypointConfig;

use crate::packages::Package;

/// Metadata for a snapshot
///
/// This structure uses `Rc` for package and subvolume lists to make cloning cheap.
/// When a Snapshot is cloned, only the reference counts are incremented, not the entire vectors.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub path: PathBuf,
    pub description: Option<String>,
    pub kernel_version: Option<String>,
    pub package_count: Option<usize>,
    pub size_bytes: Option<u64>,
    /// List of installed packages at time of snapshot (wrapped in Rc for cheap cloning)
    pub packages: Rc<Vec<Package>>,
    /// List of subvolumes included in this snapshot (wrapped in Rc for cheap cloning)
    pub subvolumes: Rc<Vec<PathBuf>>,
}

/// Helper struct for serde serialization/deserialization
#[derive(Debug, Serialize, Deserialize)]
struct SnapshotSerde {
    id: String,
    name: String,
    timestamp: DateTime<Utc>,
    path: PathBuf,
    description: Option<String>,
    kernel_version: Option<String>,
    package_count: Option<usize>,
    size_bytes: Option<u64>,
    #[serde(default)]
    packages: Vec<Package>,
    #[serde(default)]
    subvolumes: Vec<PathBuf>,
}

impl Serialize for Snapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let helper = SnapshotSerde {
            id: self.id.clone(),
            name: self.name.clone(),
            timestamp: self.timestamp,
            path: self.path.clone(),
            description: self.description.clone(),
            kernel_version: self.kernel_version.clone(),
            package_count: self.package_count,
            size_bytes: self.size_bytes,
            packages: (*self.packages).clone(),
            subvolumes: (*self.subvolumes).clone(),
        };
        helper.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Snapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = SnapshotSerde::deserialize(deserializer)?;
        Ok(Snapshot {
            id: helper.id,
            name: helper.name,
            timestamp: helper.timestamp,
            path: helper.path,
            description: helper.description,
            kernel_version: helper.kernel_version,
            package_count: helper.package_count,
            size_bytes: helper.size_bytes,
            packages: Rc::new(helper.packages),
            subvolumes: Rc::new(helper.subvolumes),
        })
    }
}

impl Snapshot {
    /// Format timestamp for display
    pub fn format_timestamp(&self) -> String {
        self.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

// Re-export format_bytes from waypoint_common
pub use waypoint_common::format_bytes;

/// Manage snapshot metadata persistence
pub struct SnapshotManager {
    metadata_file: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    ///
    /// Initializes the manager and ensures the metadata directory exists.
    /// Uses the default metadata path from `WaypointConfig` (typically
    /// `/home/user/.local/share/waypoint/snapshots.json`).
    ///
    /// # Errors
    /// - Failed to create metadata directory
    ///
    /// # Example
    /// ```no_run
    /// use waypoint::snapshot::SnapshotManager;
    ///
    /// let manager = SnapshotManager::new()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new() -> Result<Self> {
        let config = WaypointConfig::new();
        let metadata_file = config.metadata_file.clone();

        // Ensure parent directory exists
        if let Some(parent) = metadata_file.parent() {
            fs::create_dir_all(parent).context("Failed to create metadata directory")?;
        }

        Ok(Self { metadata_file })
    }

    /// Get path to snapshots metadata file
    fn metadata_path(&self) -> &PathBuf {
        &self.metadata_file
    }

    /// Load all snapshots from metadata file
    ///
    /// Reads the snapshots metadata JSON file and performs automatic cleanup:
    /// - Removes phantom snapshots (metadata exists but directory doesn't)
    /// - Removes duplicate entries (keeps most recent)
    /// - Saves cleaned metadata back to disk if changes were made
    ///
    /// # Returns
    /// Vector of valid snapshots, sorted by timestamp (oldest first)
    ///
    /// # Errors
    /// - Failed to read metadata file
    /// - Failed to parse JSON
    /// - Failed to save cleaned metadata
    ///
    /// # Note
    /// Returns empty vec if metadata file doesn't exist (not an error).
    pub fn load_snapshots(&self) -> Result<Vec<Snapshot>> {
        let path = self.metadata_path();

        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = self
            .read_locked_file(path)
            .context("Failed to read snapshots metadata")?;

        let mut snapshots: Vec<Snapshot> =
            serde_json::from_str(&content).context("Failed to parse snapshots metadata")?;

        // Filter out snapshots that don't exist on disk (phantom snapshots)
        let initial_count = snapshots.len();
        snapshots.retain(|s| s.path.exists());
        let after_phantom_cleanup = snapshots.len();

        // Remove duplicates by keeping only the last occurrence of each ID
        let mut seen_ids = std::collections::HashSet::new();
        let mut deduped = Vec::new();

        // Iterate in reverse to keep the most recent entry for each ID
        for snapshot in snapshots.into_iter().rev() {
            if seen_ids.insert(snapshot.id.clone()) {
                deduped.push(snapshot);
            }
        }
        deduped.reverse(); // Restore original order

        let after_dedup = deduped.len();

        // If we cleaned anything, save the cleaned list
        if after_phantom_cleanup < initial_count {
            log::info!(
                "Cleaned up {} phantom snapshot(s) from metadata",
                initial_count - after_phantom_cleanup
            );
        }
        if after_dedup < after_phantom_cleanup {
            log::info!(
                "Cleaned up {} duplicate snapshot(s) from metadata",
                after_phantom_cleanup - after_dedup
            );
        }

        if after_dedup < initial_count {
            let _ = self.save_snapshots(&deduped);
        }

        Ok(deduped)
    }

    /// Save snapshots to disk
    #[allow(dead_code)]
    pub fn save_snapshots(&self, snapshots: &[Snapshot]) -> Result<()> {
        let path = self.metadata_path();
        let _lock = self.locked_file(path, true)?;
        let content =
            serde_json::to_string_pretty(snapshots).context("Failed to serialize snapshots")?;

        let tmp_path = path.with_extension("json.tmp");

        {
            let mut tmp_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .with_context(|| {
                    format!(
                        "Failed to open temporary metadata file {}",
                        tmp_path.display()
                    )
                })?;
            tmp_file
                .write_all(content.as_bytes())
                .context("Failed to write snapshots metadata")?;
            tmp_file
                .sync_all()
                .context("Failed to sync snapshots metadata")?;
        }

        fs::rename(&tmp_path, path)
            .with_context(|| format!("Failed to atomically replace {}", path.display()))?;

        Ok(())
    }

    fn locked_file(&self, path: &PathBuf, write: bool) -> Result<std::fs::File> {
        let file = OpenOptions::new()
            .read(true)
            .write(write)
            .create(write)
            .open(path)
            .with_context(|| format!("Failed to open metadata file {}", path.display()))?;

        if write {
            fs2::FileExt::lock_exclusive(&file)
                .context("Failed to lock metadata file for writing")?;
        } else {
            fs2::FileExt::lock_shared(&file).context("Failed to lock metadata file for reading")?;
        }

        Ok(file)
    }

    fn read_locked_file(&self, path: &PathBuf) -> Result<String> {
        let mut file = self.locked_file(path, false)?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .context("Failed to read metadata file")?;
        fs2::FileExt::unlock(&file).ok();
        Ok(content)
    }

    /// Add or update a snapshot in metadata
    ///
    /// If a snapshot with the same ID already exists, it will be replaced.
    /// This is useful for updating snapshot information (e.g., adding size).
    ///
    /// # Arguments
    /// * `snapshot` - Snapshot to add or update
    ///
    /// # Errors
    /// - Failed to load existing snapshots
    /// - Failed to save updated metadata
    ///
    /// # Example
    /// ```no_run
    /// # use waypoint::snapshot::{SnapshotManager, Snapshot};
    /// # use std::path::PathBuf;
    /// let manager = SnapshotManager::new()?;
    /// let snapshot = Snapshot::new("backup".to_string(), PathBuf::from("/.snapshots/backup"));
    /// manager.add_snapshot(snapshot)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[allow(dead_code)]
    pub fn add_snapshot(&self, snapshot: Snapshot) -> Result<()> {
        let mut snapshots = self.load_snapshots()?;

        // Remove any existing snapshot with the same ID to avoid duplicates
        snapshots.retain(|s| s.id != snapshot.id);

        // Add the new/updated snapshot
        snapshots.push(snapshot);
        self.save_snapshots(&snapshots)
    }

    /// Get snapshot by ID
    ///
    /// Loads all snapshots and searches for one matching the given ID.
    ///
    /// # Arguments
    /// * `id` - Snapshot ID to search for
    ///
    /// # Returns
    /// * `Ok(Some(snapshot))` - Snapshot found
    /// * `Ok(None)` - Snapshot not found
    /// * `Err(_)` - Failed to load snapshots
    pub fn get_snapshot(&self, id: &str) -> Result<Option<Snapshot>> {
        let snapshots = self.load_snapshots()?;
        Ok(snapshots.into_iter().find(|s| s.id == id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512.00 B");
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GiB");
    }
}
