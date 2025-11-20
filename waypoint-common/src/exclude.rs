//! Exclude pattern configuration for snapshots
//!
//! Btrfs snapshots cannot natively exclude files, so we use a workaround:
//! 1. Create a writable snapshot
//! 2. Delete files matching exclude patterns
//! 3. Make the snapshot read-only
//!
//! This approach is used by tools like btrbk.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Pattern matching style
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PatternType {
    /// Exact path match (e.g., "/var/cache")
    Exact,
    /// Glob pattern (e.g., "/home/*/.cache")
    Glob,
    /// Prefix match (e.g., "/tmp" matches "/tmp/anything")
    Prefix,
}

/// A single exclude pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludePattern {
    /// The pattern string
    pub pattern: String,
    /// How to match this pattern
    pub pattern_type: PatternType,
    /// Human-readable description
    pub description: String,
    /// Whether this pattern is enabled
    pub enabled: bool,
    /// Whether this is a system default (cannot be deleted, only disabled)
    pub system_default: bool,
}

impl ExcludePattern {
    /// Create a new custom exclude pattern
    pub fn new(pattern: String, pattern_type: PatternType, description: String) -> Self {
        Self {
            pattern,
            pattern_type,
            description,
            enabled: true,
            system_default: false,
        }
    }

    /// Create a system default pattern
    fn system(pattern: &str, pattern_type: PatternType, description: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            pattern_type,
            description: description.to_string(),
            enabled: true,
            system_default: true,
        }
    }

    /// Check if a path matches this pattern
    pub fn matches(&self, path: &Path) -> bool {
        if !self.enabled {
            return false;
        }

        let path_str = path.to_string_lossy();

        match self.pattern_type {
            PatternType::Exact => path_str == self.pattern,
            PatternType::Prefix => path_str.starts_with(&self.pattern),
            PatternType::Glob => {
                // Simple glob matching - could be enhanced with glob crate
                self.simple_glob_match(&path_str, &self.pattern)
            }
        }
    }

    /// Simple glob matching (supports * wildcard)
    fn simple_glob_match(&self, path: &str, pattern: &str) -> bool {
        // Split pattern by * and check if all parts are present in order
        let parts: Vec<&str> = pattern.split('*').collect();

        if parts.is_empty() {
            return path == pattern;
        }

        let mut pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if i == 0 && !part.is_empty() {
                // First part must match at start
                if !path[pos..].starts_with(part) {
                    return false;
                }
                pos += part.len();
            } else if i == parts.len() - 1 && !part.is_empty() {
                // Last part must match at end
                return path[pos..].ends_with(part);
            } else if !part.is_empty() {
                // Middle parts must exist somewhere after current position
                if let Some(index) = path[pos..].find(part) {
                    pos += index + part.len();
                } else {
                    return false;
                }
            }
        }

        true
    }
}

/// Configuration for exclude patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludeConfig {
    /// List of exclude patterns
    pub patterns: Vec<ExcludePattern>,
}

impl Default for ExcludeConfig {
    fn default() -> Self {
        Self {
            patterns: Self::default_patterns(),
        }
    }
}

impl ExcludeConfig {
    /// Get the default system exclude patterns
    /// Based on Timeshift's defaults, adapted for system snapshots
    fn default_patterns() -> Vec<ExcludePattern> {
        vec![
            // Virtual filesystems (should never be in snapshots anyway)
            ExcludePattern::system("/dev", PatternType::Prefix, "Device files"),
            ExcludePattern::system("/proc", PatternType::Prefix, "Process information"),
            ExcludePattern::system("/sys", PatternType::Prefix, "System information"),
            ExcludePattern::system("/run", PatternType::Prefix, "Runtime data"),
            // Temporary directories
            ExcludePattern::system("/tmp", PatternType::Prefix, "Temporary files"),
            ExcludePattern::system("/var/tmp", PatternType::Prefix, "Variable temporary files"),
            ExcludePattern::system("/var/run", PatternType::Prefix, "Runtime variable data"),
            ExcludePattern::system("/var/lock", PatternType::Prefix, "Lock files"),
            // Mount points
            ExcludePattern::system(
                "/media",
                PatternType::Prefix,
                "Removable media mount points",
            ),
            ExcludePattern::system("/mnt", PatternType::Prefix, "Temporary mount points"),
            // Caches
            ExcludePattern::system("/var/cache/xbps", PatternType::Prefix, "XBPS package cache"),
            ExcludePattern::system("/root/.cache", PatternType::Prefix, "Root user cache"),
            ExcludePattern::system(
                "/home/*/.cache",
                PatternType::Glob,
                "User cache directories",
            ),
            // Browser caches
            ExcludePattern::system(
                "/root/.mozilla/firefox/*.default/Cache",
                PatternType::Glob,
                "Firefox cache (root)",
            ),
            ExcludePattern::system(
                "/home/*/.mozilla/firefox/*.default/Cache",
                PatternType::Glob,
                "Firefox cache (users)",
            ),
            ExcludePattern::system(
                "/root/.config/chromium/*/Cache",
                PatternType::Glob,
                "Chromium cache (root)",
            ),
            ExcludePattern::system(
                "/home/*/.config/chromium/*/Cache",
                PatternType::Glob,
                "Chromium cache (users)",
            ),
            // Thumbnails and trash
            ExcludePattern::system("/root/.thumbnails", PatternType::Prefix, "Root thumbnails"),
            ExcludePattern::system("/home/*/.thumbnails", PatternType::Glob, "User thumbnails"),
            ExcludePattern::system(
                "/root/.local/share/Trash",
                PatternType::Prefix,
                "Root trash",
            ),
            ExcludePattern::system(
                "/home/*/.local/share/Trash",
                PatternType::Glob,
                "User trash",
            ),
            // Containers and VMs
            ExcludePattern::system("/var/lib/docker", PatternType::Prefix, "Docker data"),
            ExcludePattern::system(
                "/var/lib/containers",
                PatternType::Prefix,
                "Podman/containers data",
            ),
            // Other
            ExcludePattern::system(
                "/lost+found",
                PatternType::Exact,
                "Lost and found directory",
            ),
            ExcludePattern::system("/swapfile", PatternType::Exact, "Swap file"),
        ]
    }

    /// Load configuration from disk
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path();

        if !config_path.exists() {
            // Return default configuration
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let mut config: Self = toml::from_str(&content)?;

        // Merge with defaults to ensure new defaults are included
        config.merge_defaults();

        Ok(config)
    }

    /// Save configuration to disk
    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::config_path();

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;

        Ok(())
    }

    /// Get the configuration file path
    /// Uses system-wide config because waypoint-helper runs as root
    fn config_path() -> PathBuf {
        PathBuf::from("/etc/waypoint/exclude.toml")
    }

    /// Merge default patterns with current config
    /// Adds any new default patterns that aren't already present
    fn merge_defaults(&mut self) {
        let defaults = Self::default_patterns();

        for default in defaults {
            // Check if this default pattern already exists
            let exists = self
                .patterns
                .iter()
                .any(|p| p.pattern == default.pattern && p.system_default);

            if !exists {
                self.patterns.push(default);
            }
        }
    }

    /// Get all enabled patterns
    pub fn enabled_patterns(&self) -> Vec<&ExcludePattern> {
        self.patterns.iter().filter(|p| p.enabled).collect()
    }

    /// Add a custom pattern
    pub fn add_pattern(&mut self, pattern: ExcludePattern) {
        self.patterns.push(pattern);
    }

    /// Remove a custom pattern (cannot remove system defaults)
    pub fn remove_pattern(&mut self, index: usize) -> bool {
        if index < self.patterns.len() && !self.patterns[index].system_default {
            self.patterns.remove(index);
            true
        } else {
            false
        }
    }

    /// Toggle a pattern's enabled state
    pub fn toggle_pattern(&mut self, index: usize) {
        if index < self.patterns.len() {
            self.patterns[index].enabled = !self.patterns[index].enabled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let pattern =
            ExcludePattern::new("/tmp".to_string(), PatternType::Exact, "test".to_string());

        assert!(pattern.matches(Path::new("/tmp")));
        assert!(!pattern.matches(Path::new("/tmp/foo")));
    }

    #[test]
    fn test_prefix_match() {
        let pattern =
            ExcludePattern::new("/tmp".to_string(), PatternType::Prefix, "test".to_string());

        assert!(pattern.matches(Path::new("/tmp")));
        assert!(pattern.matches(Path::new("/tmp/foo")));
        assert!(pattern.matches(Path::new("/tmp/foo/bar")));
        assert!(!pattern.matches(Path::new("/var/tmp")));
    }

    #[test]
    fn test_glob_match() {
        let pattern = ExcludePattern::new(
            "/home/*/.cache".to_string(),
            PatternType::Glob,
            "test".to_string(),
        );

        assert!(pattern.matches(Path::new("/home/alice/.cache")));
        assert!(pattern.matches(Path::new("/home/bob/.cache")));
        assert!(!pattern.matches(Path::new("/home/.cache")));
        assert!(!pattern.matches(Path::new("/root/.cache")));
    }

    #[test]
    fn test_disabled_pattern() {
        let mut pattern =
            ExcludePattern::new("/tmp".to_string(), PatternType::Prefix, "test".to_string());
        pattern.enabled = false;

        assert!(!pattern.matches(Path::new("/tmp")));
    }
}
