use anyhow::{Context, Result};
use chrono::{Duration, Utc};

use crate::snapshot::Snapshot;

/// Retention policy for snapshots
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Maximum number of snapshots to keep (0 = unlimited)
    pub max_snapshots: usize,
    /// Maximum age in days (0 = unlimited)
    pub max_age_days: u32,
    /// Keep at least this many snapshots regardless of age
    pub min_snapshots: usize,
    /// Patterns for snapshots to always keep (never auto-delete)
    pub keep_patterns: Vec<String>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_snapshots: 10,      // Keep last 10 snapshots
            max_age_days: 30,       // Keep snapshots for 30 days
            min_snapshots: 3,       // Always keep at least 3
            keep_patterns: vec![],  // No pinned patterns by default
        }
    }
}

impl RetentionPolicy {
    /// Load retention policy from config file
    pub fn load() -> Result<Self> {
        let config_path = dirs::config_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .map(|d| d.join("waypoint").join("retention.json"))
            .context("Failed to determine config directory")?;

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let policy: RetentionPolicy = serde_json::from_str(&content)?;
        Ok(policy)
    }

    /// Save retention policy to config file
    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let config_dir = dirs::config_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .map(|d| d.join("waypoint"))
            .context("Failed to determine config directory")?;

        std::fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("retention.json");

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Check if a snapshot should be kept based on patterns
    fn should_keep_by_pattern(&self, snapshot: &Snapshot) -> bool {
        if self.keep_patterns.is_empty() {
            return false;
        }

        for pattern in &self.keep_patterns {
            if snapshot.name.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Determine which snapshots should be deleted based on this policy
    pub fn apply(&self, snapshots: &[Snapshot]) -> Vec<String> {
        let mut to_delete = Vec::new();

        // Sort by timestamp (oldest first)
        let mut sorted = snapshots.to_vec();
        sorted.sort_by_key(|s| s.timestamp);

        // Never delete if we're under min_snapshots
        if sorted.len() <= self.min_snapshots {
            return to_delete;
        }

        let now = Utc::now();

        // Apply retention rules
        for (idx, snapshot) in sorted.iter().enumerate() {
            // Always keep minimum number of most recent snapshots
            let keep_recent = sorted.len() - idx <= self.min_snapshots;
            if keep_recent {
                continue;
            }

            // Check pattern-based keeps (pinned snapshots)
            if self.should_keep_by_pattern(snapshot) {
                continue;
            }

            let mut should_delete = false;

            // Check max_snapshots (keep only the N most recent)
            if self.max_snapshots > 0 {
                let position_from_end = sorted.len() - idx;
                if position_from_end > self.max_snapshots {
                    should_delete = true;
                }
            }

            // Check max_age_days
            if self.max_age_days > 0 && !should_delete {
                let age = now.signed_duration_since(snapshot.timestamp);
                let max_age = Duration::days(self.max_age_days as i64);
                if age > max_age {
                    should_delete = true;
                }
            }

            if should_delete {
                to_delete.push(snapshot.name.clone());
            }
        }

        to_delete
    }
}

// Make RetentionPolicy serializable
use serde::{Deserialize, Serialize};

impl Serialize for RetentionPolicy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("RetentionPolicy", 4)?;
        state.serialize_field("max_snapshots", &self.max_snapshots)?;
        state.serialize_field("max_age_days", &self.max_age_days)?;
        state.serialize_field("min_snapshots", &self.min_snapshots)?;
        state.serialize_field("keep_patterns", &self.keep_patterns)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for RetentionPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RetentionPolicyHelper {
            max_snapshots: usize,
            max_age_days: u32,
            min_snapshots: usize,
            keep_patterns: Vec<String>,
        }

        let helper = RetentionPolicyHelper::deserialize(deserializer)?;
        Ok(RetentionPolicy {
            max_snapshots: helper.max_snapshots,
            max_age_days: helper.max_age_days,
            min_snapshots: helper.min_snapshots,
            keep_patterns: helper.keep_patterns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_snapshot(name: &str, days_ago: i64) -> Snapshot {
        let timestamp = Utc::now() - Duration::days(days_ago);
        Snapshot {
            id: name.to_string(),
            name: name.to_string(),
            timestamp,
            path: PathBuf::from(format!("/@snapshots/{}", name)),
            description: None,
            kernel_version: None,
            package_count: Some(0),
            size_bytes: None,
            packages: std::rc::Rc::new(vec![]),
            subvolumes: std::rc::Rc::new(vec![PathBuf::from("/")]),
        }
    }

    #[test]
    fn test_max_snapshots_policy() {
        let policy = RetentionPolicy {
            max_snapshots: 3,
            max_age_days: 0,
            min_snapshots: 1,
            keep_patterns: vec![],
        };

        let snapshots = vec![
            create_test_snapshot("snapshot-1", 5),
            create_test_snapshot("snapshot-2", 4),
            create_test_snapshot("snapshot-3", 3),
            create_test_snapshot("snapshot-4", 2),
            create_test_snapshot("snapshot-5", 1),
        ];

        let to_delete = policy.apply(&snapshots);

        // Should keep 3 most recent (snapshots 3, 4, 5)
        // Should delete snapshots 1 and 2
        assert_eq!(to_delete.len(), 2);
        assert!(to_delete.contains(&"snapshot-1".to_string()));
        assert!(to_delete.contains(&"snapshot-2".to_string()));
    }

    #[test]
    fn test_max_age_policy() {
        let policy = RetentionPolicy {
            max_snapshots: 0,
            max_age_days: 7,
            min_snapshots: 1,
            keep_patterns: vec![],
        };

        let snapshots = vec![
            create_test_snapshot("snapshot-old", 10),
            create_test_snapshot("snapshot-recent", 3),
        ];

        let to_delete = policy.apply(&snapshots);

        // Should delete snapshot older than 7 days
        assert_eq!(to_delete.len(), 1);
        assert!(to_delete.contains(&"snapshot-old".to_string()));
    }

    #[test]
    fn test_min_snapshots_protection() {
        let policy = RetentionPolicy {
            max_snapshots: 2,
            max_age_days: 1,
            min_snapshots: 3,
            keep_patterns: vec![],
        };

        let snapshots = vec![
            create_test_snapshot("snapshot-1", 10),
            create_test_snapshot("snapshot-2", 9),
            create_test_snapshot("snapshot-3", 8),
        ];

        let to_delete = policy.apply(&snapshots);

        // Should keep all 3 because of min_snapshots
        assert_eq!(to_delete.len(), 0);
    }

    #[test]
    fn test_keep_patterns() {
        let policy = RetentionPolicy {
            max_snapshots: 2,
            max_age_days: 0,
            min_snapshots: 1,
            keep_patterns: vec!["pre-upgrade".to_string()],
        };

        let snapshots = vec![
            create_test_snapshot("snapshot-1", 5),
            create_test_snapshot("pre-upgrade-20251107", 4),
            create_test_snapshot("snapshot-2", 3),
            create_test_snapshot("snapshot-3", 2),
        ];

        let to_delete = policy.apply(&snapshots);

        // Should keep: pre-upgrade (pattern), snapshot-2, snapshot-3 (most recent)
        // Should delete: snapshot-1
        assert_eq!(to_delete.len(), 1);
        assert!(to_delete.contains(&"snapshot-1".to_string()));
        assert!(!to_delete.contains(&"pre-upgrade-20251107".to_string()));
    }
}
