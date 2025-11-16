//! User-specific preferences for snapshots
//!
//! This module manages user-specific metadata like favorites and notes,
//! stored separately from the main snapshot metadata to allow multiple users
//! to have their own preferences for the same snapshots.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;

/// User preferences for a specific snapshot
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotPreferences {
    /// Whether this snapshot is favorited/pinned by this user
    #[serde(default)]
    pub is_favorite: bool,

    /// User's personal note for this snapshot
    #[serde(default)]
    pub note: Option<String>,
}

/// Manager for user-specific snapshot preferences
pub struct UserPreferencesManager {
    preferences_file: PathBuf,
}

impl UserPreferencesManager {
    /// Create a new user preferences manager
    ///
    /// Uses `~/.local/share/waypoint/user-preferences.json` to store user-specific
    /// metadata like favorites and notes.
    pub fn new() -> Result<Self> {
        let preferences_file = if let Some(data_dir) = dirs::data_dir() {
            let waypoint_dir = data_dir.join("waypoint");

            // Ensure directory exists
            fs::create_dir_all(&waypoint_dir)
                .context("Failed to create user preferences directory")?;

            waypoint_dir.join("user-preferences.json")
        } else {
            // Fallback if XDG data dir isn't available
            PathBuf::from("/tmp/waypoint-user-preferences.json")
        };

        Ok(Self { preferences_file })
    }

    /// Load all user preferences
    ///
    /// Returns a HashMap mapping snapshot IDs to their preferences.
    /// Returns empty map if file doesn't exist (not an error).
    pub fn load(&self) -> Result<HashMap<String, SnapshotPreferences>> {
        if !self.preferences_file.exists() {
            return Ok(HashMap::new());
        }

        let mut file = self.locked_file(false)?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .context("Failed to read user preferences")?;
        fs2::FileExt::unlock(&file).ok();

        let prefs: HashMap<String, SnapshotPreferences> =
            serde_json::from_str(&content).context("Failed to parse user preferences")?;

        Ok(prefs)
    }

    /// Save all user preferences
    pub fn save(&self, preferences: &HashMap<String, SnapshotPreferences>) -> Result<()> {
        let content = serde_json::to_string_pretty(preferences)
            .context("Failed to serialize user preferences")?;

        let _lock = self.locked_file(true)?;
        let tmp_path = self.preferences_file.with_extension("tmp");

        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .with_context(|| {
                    format!(
                        "Failed to open temporary preferences file {}",
                        tmp_path.display()
                    )
                })?;
            file.write_all(content.as_bytes())
                .context("Failed to write user preferences")?;
            file.sync_all().context("Failed to sync user preferences")?;
        }

        fs::rename(&tmp_path, &self.preferences_file)
            .with_context(|| format!("Failed to replace {}", self.preferences_file.display()))?;

        Ok(())
    }

    /// Get preferences for a specific snapshot
    pub fn get(&self, snapshot_id: &str) -> Result<SnapshotPreferences> {
        let prefs = self.load()?;
        Ok(prefs.get(snapshot_id).cloned().unwrap_or_default())
    }

    /// Update preferences for a specific snapshot
    pub fn update(&self, snapshot_id: &str, preferences: SnapshotPreferences) -> Result<()> {
        let mut all_prefs = self.load()?;

        // If preferences are default (not favorite, no note), remove the entry to keep file clean
        if !preferences.is_favorite && preferences.note.is_none() {
            all_prefs.remove(snapshot_id);
        } else {
            all_prefs.insert(snapshot_id.to_string(), preferences);
        }

        self.save(&all_prefs)
    }

    /// Toggle favorite status for a snapshot
    pub fn toggle_favorite(&self, snapshot_id: &str) -> Result<bool> {
        let mut prefs = self.get(snapshot_id)?;
        prefs.is_favorite = !prefs.is_favorite;
        let new_state = prefs.is_favorite;
        self.update(snapshot_id, prefs)?;
        Ok(new_state)
    }

    /// Update note for a snapshot
    pub fn update_note(&self, snapshot_id: &str, note: Option<String>) -> Result<()> {
        let mut prefs = self.get(snapshot_id)?;
        prefs.note = note;
        self.update(snapshot_id, prefs)
    }

    fn locked_file(&self, write: bool) -> Result<std::fs::File> {
        let file = OpenOptions::new()
            .read(true)
            .write(write)
            .create(write)
            .open(&self.preferences_file)
            .with_context(|| format!("Failed to open {}", self.preferences_file.display()))?;

        if write {
            fs2::FileExt::lock_exclusive(&file)
                .context("Failed to lock preferences for writing")?;
        } else {
            fs2::FileExt::lock_shared(&file).context("Failed to lock preferences for reading")?;
        }

        Ok(file)
    }
}
