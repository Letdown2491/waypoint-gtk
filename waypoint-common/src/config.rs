// Centralized configuration for Waypoint

use std::path::PathBuf;

/// Waypoint configuration with support for environment variable overrides
#[derive(Debug, Clone)]
pub struct WaypointConfig {
    /// Directory where snapshots are stored (default: /.snapshots)
    pub snapshot_dir: PathBuf,

    /// Path to metadata file (default: /var/lib/waypoint/snapshots.json)
    pub metadata_file: PathBuf,

    /// Path to scheduler configuration (default: /etc/waypoint/scheduler.conf)
    /// DEPRECATED: Use schedules_config instead
    pub scheduler_config: PathBuf,

    /// Path to schedules TOML configuration (default: /etc/waypoint/schedules.toml)
    pub schedules_config: PathBuf,

    /// Path to backup configuration (default: ~/.config/waypoint/backup-config.toml)
    pub backup_config: PathBuf,

    /// Path to service directory for scheduler (default: /var/service, runit-specific)
    pub service_dir: PathBuf,

    /// Minimum free space required before creating snapshots (in bytes)
    pub min_free_space_bytes: u64,

    /// Default window width
    pub ui_window_width: i32,

    /// Default window height
    pub ui_window_height: i32,

    /// Maximum window width
    pub ui_max_width: i32,

    /// Default maximum number of snapshots to retain
    pub retention_max_snapshots: usize,

    /// Default maximum age for snapshots (in days)
    pub retention_max_age_days: u64,

    /// Minimum number of snapshots to always keep
    pub retention_min_snapshots: usize,
}

impl Default for WaypointConfig {
    fn default() -> Self {
        // Get user's config directory for backup config
        let backup_config = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(|home| {
                let mut path = PathBuf::from(home);
                path.push(".config");
                path.push("waypoint");
                path.push("backup-config.toml");
                path
            })
            .unwrap_or_else(|_| PathBuf::from("/tmp/waypoint-backup-config.toml"));

        Self {
            snapshot_dir: PathBuf::from("/.snapshots"),
            metadata_file: PathBuf::from("/var/lib/waypoint/snapshots.json"),
            scheduler_config: PathBuf::from("/etc/waypoint/scheduler.conf"),
            schedules_config: PathBuf::from("/etc/waypoint/schedules.toml"),
            backup_config,
            service_dir: PathBuf::from("/var/service"),
            min_free_space_bytes: 1024 * 1024 * 1024, // 1 GB
            ui_window_width: 800,
            ui_window_height: 600,
            ui_max_width: 800,
            retention_max_snapshots: 10,
            retention_max_age_days: 30,
            retention_min_snapshots: 3,
        }
    }
}

impl WaypointConfig {
    /// Create a new configuration with environment variable overrides
    ///
    /// Supported environment variables:
    /// - WAYPOINT_SNAPSHOT_DIR: Override snapshot directory
    /// - WAYPOINT_METADATA_FILE: Override metadata file path
    /// - WAYPOINT_SCHEDULER_CONFIG: Override scheduler config path (deprecated)
    /// - WAYPOINT_SCHEDULES_CONFIG: Override schedules TOML config path
    /// - WAYPOINT_BACKUP_CONFIG: Override backup config path
    /// - WAYPOINT_SERVICE_DIR: Override service directory (for init system integration)
    /// - WAYPOINT_MIN_FREE_SPACE_GB: Override minimum free space (in GB)
    pub fn new() -> Self {
        let mut config = Self::default();

        // Override from environment variables
        if let Ok(dir) = std::env::var("WAYPOINT_SNAPSHOT_DIR") {
            config.snapshot_dir = PathBuf::from(dir);
        }

        if let Ok(file) = std::env::var("WAYPOINT_METADATA_FILE") {
            config.metadata_file = PathBuf::from(file);
        }

        if let Ok(conf) = std::env::var("WAYPOINT_SCHEDULER_CONFIG") {
            config.scheduler_config = PathBuf::from(conf);
        }

        if let Ok(conf) = std::env::var("WAYPOINT_SCHEDULES_CONFIG") {
            config.schedules_config = PathBuf::from(conf);
        }

        if let Ok(conf) = std::env::var("WAYPOINT_BACKUP_CONFIG") {
            config.backup_config = PathBuf::from(conf);
        }

        if let Ok(dir) = std::env::var("WAYPOINT_SERVICE_DIR") {
            config.service_dir = PathBuf::from(dir);
        }

        if let Ok(space_gb) = std::env::var("WAYPOINT_MIN_FREE_SPACE_GB") {
            if let Ok(gb) = space_gb.parse::<u64>() {
                config.min_free_space_bytes = gb * 1024 * 1024 * 1024;
            }
        }

        config
    }

    /// Get the full path to the scheduler service
    pub fn scheduler_service_path(&self) -> PathBuf {
        self.service_dir.join("waypoint-scheduler")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WaypointConfig::default();
        assert_eq!(config.snapshot_dir, PathBuf::from("/.snapshots"));
        assert_eq!(
            config.metadata_file,
            PathBuf::from("/var/lib/waypoint/snapshots.json")
        );
        assert_eq!(config.min_free_space_bytes, 1024 * 1024 * 1024);
        assert_eq!(config.ui_window_width, 800);
        assert_eq!(config.ui_window_height, 600);
    }

    #[test]
    fn test_scheduler_service_path() {
        let config = WaypointConfig::default();
        assert_eq!(
            config.scheduler_service_path(),
            PathBuf::from("/var/service/waypoint-scheduler")
        );
    }
}
