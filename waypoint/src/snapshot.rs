use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
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
    /// Create a new snapshot with timestamp-based ID
    #[allow(dead_code)]
    pub fn new(name: String, path: PathBuf) -> Self {
        let timestamp = Utc::now();
        let id = format!("snapshot-{}", timestamp.format("%Y%m%d-%H%M%S"));

        Self {
            id,
            name,
            timestamp,
            path,
            description: None,
            kernel_version: Self::get_kernel_version(),
            package_count: None,
            size_bytes: None,
            packages: Rc::new(Vec::new()),
            subvolumes: Rc::new(vec![PathBuf::from("/")]),
        }
    }

    /// Set the packages for this snapshot
    #[allow(dead_code)]
    pub fn with_packages(mut self, packages: Vec<Package>) -> Self {
        self.package_count = Some(packages.len());
        self.packages = Rc::new(packages);
        self
    }

    /// Get current kernel version
    #[allow(dead_code)]
    fn get_kernel_version() -> Option<String> {
        fs::read_to_string("/proc/version")
            .ok()
            .and_then(|v| v.split_whitespace().nth(2).map(String::from))
    }

    /// Format timestamp for display
    pub fn format_timestamp(&self) -> String {
        self.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    /// Format size for display
    #[allow(dead_code)]
    pub fn format_size(&self) -> String {
        match self.size_bytes {
            Some(bytes) => format_bytes(bytes),
            None => "Unknown".to_string(),
        }
    }
}

/// Format bytes to human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}

/// Manage snapshot metadata persistence
pub struct SnapshotManager {
    metadata_file: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    pub fn new() -> Result<Self> {
        let config = WaypointConfig::new();
        let metadata_file = config.metadata_file.clone();

        // Ensure parent directory exists
        if let Some(parent) = metadata_file.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create metadata directory")?;
        }

        Ok(Self { metadata_file })
    }

    /// Get path to snapshots metadata file
    fn metadata_path(&self) -> &PathBuf {
        &self.metadata_file
    }

    /// Load all snapshots from disk
    pub fn load_snapshots(&self) -> Result<Vec<Snapshot>> {
        let path = self.metadata_path();

        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read snapshots metadata")?;

        let mut snapshots: Vec<Snapshot> = serde_json::from_str(&content)
            .context("Failed to parse snapshots metadata")?;

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
            eprintln!("Cleaned up {} phantom snapshot(s) from metadata", initial_count - after_phantom_cleanup);
        }
        if after_dedup < after_phantom_cleanup {
            eprintln!("Cleaned up {} duplicate snapshot(s) from metadata", after_phantom_cleanup - after_dedup);
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
        let content = serde_json::to_string_pretty(snapshots)
            .context("Failed to serialize snapshots")?;

        fs::write(&path, content)
            .context("Failed to write snapshots metadata")?;

        Ok(())
    }

    /// Add or update a snapshot
    /// If a snapshot with the same ID already exists, it will be replaced
    #[allow(dead_code)]
    pub fn add_snapshot(&self, snapshot: Snapshot) -> Result<()> {
        let mut snapshots = self.load_snapshots()?;

        // Remove any existing snapshot with the same ID to avoid duplicates
        snapshots.retain(|s| s.id != snapshot.id);

        // Add the new/updated snapshot
        snapshots.push(snapshot);
        self.save_snapshots(&snapshots)
    }

    /// Remove a snapshot by ID
    #[allow(dead_code)]
    pub fn remove_snapshot(&self, id: &str) -> Result<()> {
        let mut snapshots = self.load_snapshots()?;
        snapshots.retain(|s| s.id != id);
        self.save_snapshots(&snapshots)
    }

    /// Get snapshot by ID
    pub fn get_snapshot(&self, id: &str) -> Result<Option<Snapshot>> {
        let snapshots = self.load_snapshots()?;
        Ok(snapshots.into_iter().find(|s| s.id == id))
    }

    /// Apply retention policy and get list of snapshots that should be deleted
    pub fn get_snapshots_to_cleanup(&self) -> Result<Vec<String>> {
        use crate::retention::RetentionPolicy;

        let policy = RetentionPolicy::load()?;
        let snapshots = self.load_snapshots()?;
        let to_delete = policy.apply(&snapshots);

        Ok(to_delete)
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
