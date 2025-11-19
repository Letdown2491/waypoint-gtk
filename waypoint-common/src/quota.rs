//! Btrfs quota configuration and management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of quota to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum QuotaType {
    /// Simple quotas (kernel 6.7+) - better performance, less detailed tracking
    #[default]
    Simple,
    /// Traditional qgroups - more detailed, but can impact performance
    Traditional,
}


/// Quota configuration for snapshot management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Whether quotas are enabled
    #[serde(default)]
    pub enabled: bool,

    /// Type of quota system to use
    #[serde(default)]
    pub quota_type: QuotaType,

    /// Total space limit for all snapshots (in bytes)
    /// None means no limit
    #[serde(default)]
    pub total_limit_bytes: Option<u64>,

    /// Per-snapshot space limit (in bytes)
    /// None means no per-snapshot limit
    #[serde(default)]
    pub per_snapshot_limit_bytes: Option<u64>,

    /// Threshold (0.0-1.0) at which to trigger automatic cleanup
    /// Default: 0.9 (90%)
    #[serde(default = "default_cleanup_threshold")]
    pub cleanup_threshold: f64,

    /// Whether to automatically clean up old snapshots when quota is exceeded
    #[serde(default = "default_auto_cleanup")]
    pub auto_cleanup: bool,
}

fn default_cleanup_threshold() -> f64 {
    0.9
}

fn default_auto_cleanup() -> bool {
    true
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            quota_type: QuotaType::default(),
            total_limit_bytes: None,
            per_snapshot_limit_bytes: None,
            cleanup_threshold: default_cleanup_threshold(),
            auto_cleanup: default_auto_cleanup(),
        }
    }
}

impl QuotaConfig {
    /// Get the default path for quota configuration
    pub fn default_path() -> PathBuf {
        PathBuf::from("/etc/waypoint/quota.toml")
    }

    /// Load quota configuration from file
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::default_path();

        if !path.exists() {
            // Return default config if file doesn't exist
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&path)?;
        let config: QuotaConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Save quota configuration to file
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::default_path();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }

    /// Parse a human-readable size string (e.g., "50G", "1T", "500M")
    pub fn parse_size(size_str: &str) -> anyhow::Result<u64> {
        let size_str = size_str.trim().to_uppercase();

        let (num_str, unit) = if size_str.ends_with("TB") || size_str.ends_with("TIB") {
            (&size_str[..size_str.len() - 2], 1024u64.pow(4))
        } else if size_str.ends_with('T') {
            (&size_str[..size_str.len() - 1], 1024u64.pow(4))
        } else if size_str.ends_with("GB") || size_str.ends_with("GIB") {
            (&size_str[..size_str.len() - 2], 1024u64.pow(3))
        } else if size_str.ends_with('G') {
            (&size_str[..size_str.len() - 1], 1024u64.pow(3))
        } else if size_str.ends_with("MB") || size_str.ends_with("MIB") {
            (&size_str[..size_str.len() - 2], 1024u64.pow(2))
        } else if size_str.ends_with('M') {
            (&size_str[..size_str.len() - 1], 1024u64.pow(2))
        } else if size_str.ends_with("KB") || size_str.ends_with("KIB") {
            (&size_str[..size_str.len() - 2], 1024)
        } else if size_str.ends_with('K') {
            (&size_str[..size_str.len() - 1], 1024)
        } else {
            // Assume bytes
            (size_str.as_str(), 1)
        };

        let num: u64 = num_str.trim().parse()?;
        Ok(num * unit)
    }

    /// Format bytes as human-readable size
    pub fn format_size(bytes: u64) -> String {
        crate::format::format_bytes(bytes)
    }
}

/// Quota usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaUsage {
    /// Total referenced bytes (how much data is in snapshots)
    pub referenced: u64,

    /// Exclusive bytes (how much would be freed if deleted)
    pub exclusive: u64,

    /// Limit in bytes (None if no limit set)
    pub limit: Option<u64>,
}

impl QuotaUsage {
    /// Calculate usage percentage (0.0-1.0)
    pub fn usage_percent(&self) -> Option<f64> {
        self.limit.map(|limit| {
            if limit == 0 {
                // No limit set, return 0% usage
                0.0
            } else {
                // Convert to f64 for division
                // Note: u64::MAX fits in f64 with some precision loss at extreme values
                // but this is acceptable for percentage calculations
                let usage = self.referenced as f64;
                let total = limit as f64;
                let percentage = usage / total;

                // Clamp to [0.0, 1.0] range to handle any edge cases
                // (e.g., if usage somehow exceeds limit due to race conditions)
                percentage.clamp(0.0, 1.0)
            }
        })
    }

    /// Check if usage exceeds threshold
    pub fn exceeds_threshold(&self, threshold: f64) -> bool {
        // Validate threshold is in valid range
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            // Invalid threshold, default to false (don't trigger cleanup)
            return false;
        }

        self.usage_percent()
            .map(|pct| pct >= threshold)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(QuotaConfig::parse_size("1024").unwrap(), 1024);
        assert_eq!(QuotaConfig::parse_size("1K").unwrap(), 1024);
        assert_eq!(QuotaConfig::parse_size("1KB").unwrap(), 1024);
        assert_eq!(QuotaConfig::parse_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(QuotaConfig::parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(
            QuotaConfig::parse_size("50G").unwrap(),
            50 * 1024 * 1024 * 1024
        );
        assert_eq!(QuotaConfig::parse_size("1T").unwrap(), 1024u64.pow(4));
        assert!(QuotaConfig::parse_size("1.5").is_err()); // No decimals in number
    }

    #[test]
    fn test_format_size() {
        assert_eq!(QuotaConfig::format_size(1024), "1.00 KiB");
        assert_eq!(QuotaConfig::format_size(1024 * 1024), "1.00 MiB");
        assert_eq!(QuotaConfig::format_size(1024 * 1024 * 1024), "1.00 GiB");
        assert_eq!(
            QuotaConfig::format_size(50 * 1024 * 1024 * 1024),
            "50.00 GiB"
        );
    }

    #[test]
    fn test_quota_usage_percent() {
        let usage = QuotaUsage {
            referenced: 50 * 1024 * 1024 * 1024,   // 50 GB
            exclusive: 10 * 1024 * 1024 * 1024,    // 10 GB
            limit: Some(100 * 1024 * 1024 * 1024), // 100 GB
        };

        assert_eq!(usage.usage_percent(), Some(0.5));
        assert!(usage.exceeds_threshold(0.4));
        assert!(!usage.exceeds_threshold(0.6));
    }

    #[test]
    fn test_default_config() {
        let config = QuotaConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.quota_type, QuotaType::Simple);
        assert_eq!(config.total_limit_bytes, None);
        assert_eq!(config.cleanup_threshold, 0.9);
        assert!(config.auto_cleanup);
    }

    #[test]
    fn test_quota_overflow_protection() {
        // Test extreme values near u64::MAX
        let usage = QuotaUsage {
            referenced: u64::MAX - 1000,
            exclusive: u64::MAX - 1000,
            limit: Some(u64::MAX),
        };

        // Should not panic and should return a valid percentage
        let pct = usage.usage_percent();
        assert!(pct.is_some());
        assert!(pct.unwrap() > 0.99); // Very close to 100%
        assert!(pct.unwrap() <= 1.0); // Clamped to max 100%
    }

    #[test]
    fn test_exceeds_threshold_with_extreme_usage() {
        // Test when usage exceeds limit (shouldn't happen in practice but handle gracefully)
        let usage = QuotaUsage {
            referenced: 150 * 1024 * 1024 * 1024, // 150 GB
            exclusive: 10 * 1024 * 1024 * 1024,
            limit: Some(100 * 1024 * 1024 * 1024), // 100 GB limit
        };

        // Should clamp to 100% and detect threshold exceeded
        assert_eq!(usage.usage_percent(), Some(1.0)); // Clamped to 100%
        assert!(usage.exceeds_threshold(0.9));
    }

    #[test]
    fn test_exceeds_threshold_with_invalid_threshold() {
        let usage = QuotaUsage {
            referenced: 50 * 1024 * 1024 * 1024,
            exclusive: 10 * 1024 * 1024 * 1024,
            limit: Some(100 * 1024 * 1024 * 1024),
        };

        // Invalid thresholds should return false (safe default)
        assert!(!usage.exceeds_threshold(f64::NAN));
        assert!(!usage.exceeds_threshold(f64::INFINITY));
        assert!(!usage.exceeds_threshold(-0.5));
        assert!(!usage.exceeds_threshold(1.5));
    }
}
