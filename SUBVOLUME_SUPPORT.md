# Multi-Subvolume Support - Implementation Plan

## Current State vs Proposed

### Current (Phase 4):
```
Snapshot: /  (root only)
```

**Limitations:**
- Only snapshots root filesystem
- Can't separately manage /home
- Can't snapshot other subvolumes

---

### Proposed (Phase 5):
```
Snapshot Set:
  ├─ / (root)           [always included]
  ├─ /home (optional)   [user data]
  ├─ /var (optional)    [logs, databases]
  └─ ... (other subvolumes)
```

**Benefits:**
- Snapshot multiple subvolumes atomically
- Restore only what you need
- Keep /home separate from system snapshots
- More flexible than current approach

---

## Use Cases

### 1. **System-Only Snapshots** (Default)
```
Before system upgrade:
  Snapshot: /
  Skip: /home (preserve user data)
```

**Why:** If upgrade breaks system, rollback doesn't lose personal files created after snapshot.

---

### 2. **Full System Snapshots**
```
Before major changes:
  Snapshot: / + /home
```

**Why:** Complete system state for disaster recovery.

---

### 3. **Data Snapshots** (Future)
```
Regular backups:
  Snapshot: /home only
  Frequency: Daily
```

**Why:** Protect user data without snapshotting system.

---

## Technical Implementation

### Architecture

#### Current:
```rust
fn create_snapshot(name: &str) {
    btrfs::snapshot("/", "/@snapshots/{name}")
}
```

#### Proposed:
```rust
struct SnapshotConfig {
    subvolumes: Vec<SubvolumeSelection>,
}

struct SubvolumeSelection {
    path: PathBuf,          // e.g., "/"
    name: String,           // e.g., "@"
    enabled: bool,          // Include in snapshot?
    always_include: bool,   // Root is always included
}

fn create_snapshot(name: &str, config: &SnapshotConfig) {
    for subvol in config.subvolumes.iter().filter(|s| s.enabled) {
        btrfs::snapshot(
            &subvol.path,
            &format!("/@snapshots/{}/{}", name, subvol.name)
        )
    }
}
```

---

## Implementation Phases

### Phase 5.1: Basic Multi-Subvolume Support (2-3 hours)

#### Features:
1. **Detect available subvolumes**
   ```rust
   fn list_btrfs_subvolumes() -> Vec<Subvolume> {
       // Parse `btrfs subvolume list /`
   }
   ```

2. **UI: Settings dialog**
   ```
   ┌─────────────────────────────────┐
   │  Snapshot Settings              │
   ├─────────────────────────────────┤
   │                                 │
   │  Include in snapshots:          │
   │                                 │
   │  [✓] / (root)      *required    │
   │  [ ] /home         optional     │
   │  [ ] /var          optional     │
   │                                 │
   │           [Cancel]  [Save]      │
   └─────────────────────────────────┘
   ```

3. **Atomic snapshot creation**
   - Snapshot all selected subvolumes together
   - Same timestamp for all
   - All or nothing (if one fails, clean up)

4. **Metadata updates**
   ```json
   {
     "name": "waypoint-20250107-143000",
     "timestamp": "2025-01-07T14:30:00Z",
     "subvolumes": [
       {
         "path": "/",
         "snapshot_path": "/@snapshots/waypoint-20250107-143000/@"
       },
       {
         "path": "/home",
         "snapshot_path": "/@snapshots/waypoint-20250107-143000/@home"
       }
     ],
     "packages": [...]
   }
   ```

5. **Rollback updates**
   - Restore all subvolumes that were snapshotted
   - Set default for each subvolume
   - Clear warnings if restoring /home

---

### Phase 5.2: Advanced Features (Future)

#### Independent Subvolume Snapshots
- Snapshot /home separately from /
- Different schedules per subvolume
- Different retention policies

#### Selective Restore
```
┌─────────────────────────────────┐
│  Restore Snapshot               │
├─────────────────────────────────┤
│  waypoint-20250107-143000       │
│                                 │
│  Restore:                       │
│  [✓] / (root system)            │
│  [ ] /home (user data)          │
│                                 │
│  ⚠️  Restoring only / will keep │
│      current /home data         │
│                                 │
│           [Cancel]  [Restore]   │
└─────────────────────────────────┘
```

#### Subvolume Browser
- Browse files in each subvolume separately
- Compare /home changes independently

---

## UI Design

### Main Window - Snapshot List

**Current:**
```
┌─────────────────────────────────┐
│ System snapshot 2025-01-07      │
│ 2025-01-07 14:30 • 500 packages │
│                    [📁][↻][🗑️]  │
└─────────────────────────────────┘
```

**Proposed:**
```
┌─────────────────────────────────┐
│ System snapshot 2025-01-07      │
│ 2025-01-07 14:30 • 500 packages │
│ Includes: / • /home             │ ← New line
│                    [📁][↻][🗑️]  │
└─────────────────────────────────┘
```

---

### Settings Dialog (New!)

Access via: Menu → Preferences

```
┌──────────────────────────────────────────┐
│ Waypoint Preferences                     │
├──────────────────────────────────────────┤
│                                          │
│  Subvolumes                              │
│  ───────────────────────────────────     │
│                                          │
│  Select which subvolumes to include in   │
│  system snapshots:                       │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │ [✓] / (root)                       │  │
│  │     System files and applications  │  │
│  │     Required, cannot be disabled   │  │
│  │                                    │  │
│  │ [ ] /home                          │  │
│  │     User files and settings        │  │
│  │     Size: 50 GB                    │  │
│  │                                    │  │
│  │ [ ] /var                           │  │
│  │     Logs and databases             │  │
│  │     Size: 10 GB                    │  │
│  └────────────────────────────────────┘  │
│                                          │
│  💡 Tip: Excluding /home makes snapshots │
│     smaller and faster. Your personal    │
│     files won't be affected by rollback. │
│                                          │
│                      [Cancel]  [Save]    │
└──────────────────────────────────────────┘
```

---

### Restore Dialog - Enhanced

**When snapshot includes /home:**
```
┌──────────────────────────────────────────┐
│ Restore System Snapshot?                 │
├──────────────────────────────────────────┤
│                                          │
│ ⚠️  CRITICAL WARNING ⚠️                  │
│                                          │
│ This snapshot includes /home and will:   │
│                                          │
│ ✓ Restore system to 2025-01-07          │
│ ✗ REPLACE YOUR HOME DIRECTORY           │
│ ✗ You will lose:                         │
│   • Documents created after snapshot     │
│   • Downloads                            │
│   • Settings changes                     │
│   • Desktop files                        │
│                                          │
│ Files created since 2025-01-07 will be   │
│ permanently lost!                        │
│                                          │
│              [Cancel]  [Restore Anyway]  │
└──────────────────────────────────────────┘
```

**When snapshot is root-only:**
```
┌──────────────────────────────────────────┐
│ Restore System Snapshot?                 │
├──────────────────────────────────────────┤
│                                          │
│ ⚠️  WARNING ⚠️                           │
│                                          │
│ This will restore system to 2025-01-07   │
│                                          │
│ ✓ Restore system files                  │
│ ✓ Your /home will NOT be affected       │
│ ✓ Personal files are safe               │
│                                          │
│              [Cancel]  [Restore]         │
└──────────────────────────────────────────┘
```

---

## Code Structure

### New Files:

#### 1. `waypoint/src/subvolume.rs` (New)
```rust
use std::path::PathBuf;

/// Information about a Btrfs subvolume
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subvolume {
    pub path: PathBuf,
    pub name: String,
    pub id: u64,
    pub size: Option<u64>,
}

/// Detect all Btrfs subvolumes
pub fn list_subvolumes() -> Result<Vec<Subvolume>> {
    // Parse `btrfs subvolume list /`
}

/// Configuration for which subvolumes to snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubvolumeConfig {
    pub subvolumes: Vec<SubvolumeSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubvolumeSelection {
    pub path: PathBuf,
    pub name: String,
    pub enabled: bool,
    pub always_include: bool,  // Root is always included
}

impl SubvolumeConfig {
    /// Default config: root only
    pub fn default() -> Self {
        Self {
            subvolumes: vec![
                SubvolumeSelection {
                    path: PathBuf::from("/"),
                    name: "@".to_string(),
                    enabled: true,
                    always_include: true,
                }
            ]
        }
    }

    /// Load from config file
    pub fn load() -> Result<Self> {
        // ~/.config/waypoint/subvolumes.json
    }

    /// Save to config file
    pub fn save(&self) -> Result<()> {
        // ~/.config/waypoint/subvolumes.json
    }
}
```

---

#### 2. `waypoint/src/ui/preferences.rs` (New)
```rust
/// Show preferences dialog
pub fn show_preferences_dialog(
    window: &adw::ApplicationWindow,
) {
    let dialog = adw::PreferencesWindow::new();
    dialog.set_title(Some("Preferences"));

    // Subvolumes page
    let subvol_page = create_subvolume_page();
    dialog.add(&subvol_page);

    dialog.present();
}

fn create_subvolume_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.set_title("Subvolumes");
    page.set_icon_name(Some("drive-harddisk-symbolic"));

    // Group for subvolume selection
    let group = adw::PreferencesGroup::new();
    group.set_title("Include in Snapshots");
    group.set_description(Some(
        "Select which subvolumes to include in system snapshots"
    ));

    // Load current config
    let config = SubvolumeConfig::load().unwrap_or_default();
    let subvolumes = list_subvolumes().unwrap_or_default();

    for subvol in subvolumes {
        let row = create_subvolume_row(&subvol, &config);
        group.add(&row);
    }

    page.add(&group);
    page
}

fn create_subvolume_row(
    subvol: &Subvolume,
    config: &SubvolumeConfig
) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&format!("{}", subvol.path.display()));

    // Subtitle with description
    let subtitle = match subvol.path.to_str() {
        Some("/") => "System files and applications",
        Some("/home") => "User files and settings",
        Some("/var") => "Logs and databases",
        _ => "Subvolume",
    };
    row.set_subtitle(subtitle);

    // Switch to enable/disable
    let switch = gtk::Switch::new();
    switch.set_valign(gtk::Align::Center);

    let is_enabled = config.subvolumes
        .iter()
        .find(|s| s.path == subvol.path)
        .map(|s| s.enabled)
        .unwrap_or(false);

    switch.set_active(is_enabled);

    // Disable switch for root (always included)
    if subvol.path == PathBuf::from("/") {
        switch.set_sensitive(false);
        row.set_subtitle("Required, cannot be disabled");
    }

    // Connect switch to save config
    let subvol_path = subvol.path.clone();
    switch.connect_active_notify(move |sw| {
        let mut config = SubvolumeConfig::load().unwrap_or_default();

        if let Some(sel) = config.subvolumes.iter_mut()
            .find(|s| s.path == subvol_path)
        {
            sel.enabled = sw.is_active();
            let _ = config.save();
        }
    });

    row.add_suffix(&switch);
    row.set_activatable_widget(Some(&switch));

    row
}
```

---

### Modified Files:

#### `waypoint-helper/src/btrfs.rs`
```rust
/// Create snapshots for multiple subvolumes atomically
pub fn create_multi_snapshot(
    name: &str,
    description: Option<&str>,
    subvolumes: &[SubvolumeSelection],
    packages: Vec<Package>,
) -> Result<()> {
    let snapshot_base = format!("/@snapshots/{}", name);
    fs::create_dir_all(&snapshot_base)?;

    // Create snapshot for each subvolume
    let mut created = Vec::new();

    for subvol in subvolumes.iter().filter(|s| s.enabled) {
        let dest = format!("{}/{}", snapshot_base, subvol.name);

        match create_single_snapshot(&subvol.path, &dest) {
            Ok(_) => created.push(dest),
            Err(e) => {
                // Cleanup on failure
                for path in created {
                    let _ = delete_snapshot(&path);
                }
                return Err(e);
            }
        }
    }

    // Save metadata
    save_multi_snapshot_metadata(name, subvolumes, packages)?;

    Ok(())
}

fn create_single_snapshot(source: &Path, dest: &str) -> Result<()> {
    let output = Command::new("btrfs")
        .arg("subvolume")
        .arg("snapshot")
        .arg("-r")
        .arg(source)
        .arg(dest)
        .output()?;

    if !output.status.success() {
        bail!("Failed to snapshot {}", source.display());
    }

    Ok(())
}
```

---

#### `waypoint/src/ui/mod.rs`
```rust
// Add preferences button to header
let menu_button = gtk::MenuButton::new();
menu_button.set_icon_name("open-menu-symbolic");

let menu = gio::Menu::new();
menu.append(Some("Preferences"), Some("app.preferences"));
menu.append(Some("About"), Some("app.about"));
menu_button.set_menu_model(Some(&menu));

header.pack_end(&menu_button);

// Register action
let app = app.clone();
let window = window.clone();
let preferences_action = gio::SimpleAction::new("preferences", None);
preferences_action.connect_activate(move |_, _| {
    preferences::show_preferences_dialog(&window);
});
app.add_action(&preferences_action);
```

---

## Migration Strategy

### For Existing Users:

1. **Default behavior unchanged**
   - Only root (/) is snapshotted by default
   - Existing snapshots work as before

2. **Opt-in to multi-subvolume**
   - User opens Preferences
   - Enables /home or other subvolumes
   - Future snapshots include selected subvolumes

3. **Backward compatibility**
   - Old snapshots (root-only) still work
   - New metadata format is superset of old
   - Old clients can read new snapshots (ignore extra subvolumes)

---

## Storage Considerations

### Disk Space Impact:

**Current (root only):**
```
Snapshot size: ~500 MB - 2 GB (system only)
10 snapshots: ~5-20 GB
```

**With /home included:**
```
Snapshot size: ~500 MB + /home size
If /home = 50 GB:
  First snapshot: ~50 GB (full copy)
  Later snapshots: ~500 MB + changes (COW)
10 snapshots: ~55-70 GB
```

**Recommendation:**
- Default: Root only (small, fast)
- Optional: Include /home for full system backup
- Warning in UI about disk space

---

## Rollback Behavior

### Scenario 1: Root-only snapshot
```
Restore → Only / is rolled back
Result: System state restored, /home unchanged
```

**Safe:** User data preserved

---

### Scenario 2: Root + /home snapshot
```
Restore → Both / and /home are rolled back
Result: System AND user data from snapshot time
```

**Warning:** Recent user files will be lost!

---

### UI Flow for Multi-Subvolume Restore:

```
┌─────────────────────────────────┐
│ 1. User clicks "Restore"        │
└─────────────────────────────────┘
                ↓
┌─────────────────────────────────┐
│ 2. Check snapshot contents      │
│    If includes /home:            │
│      → Show SEVERE WARNING       │
│      → Explain data loss         │
│      → Require extra confirmation│
└─────────────────────────────────┘
                ↓
┌─────────────────────────────────┐
│ 3. Create pre-rollback backup   │
│    (of ALL subvolumes)           │
└─────────────────────────────────┘
                ↓
┌─────────────────────────────────┐
│ 4. Set default for each subvol  │
│    btrfs subvolume set-default   │
│    (once per subvolume)          │
└─────────────────────────────────┘
                ↓
┌─────────────────────────────────┐
│ 5. Reboot prompt                │
└─────────────────────────────────┘
```

---

## Testing Plan

### Test Cases:

1. **Single subvolume (current behavior)**
   - ✓ Create snapshot of /
   - ✓ Restore /
   - ✓ Verify /home untouched

2. **Multiple subvolumes**
   - ✓ Enable /home in preferences
   - ✓ Create snapshot of / + /home
   - ✓ Verify both snapshotted
   - ✓ Check metadata includes both

3. **Atomic snapshot creation**
   - ✓ Simulate failure midway
   - ✓ Verify cleanup (no partial snapshots)

4. **Rollback with /home**
   - ✓ Create test files in /home
   - ✓ Create snapshot
   - ✓ Add more files to /home
   - ✓ Rollback
   - ✓ Verify new files gone, old files restored

5. **Mixed snapshots**
   - ✓ Create root-only snapshot
   - ✓ Enable /home
   - ✓ Create root+home snapshot
   - ✓ Verify both types coexist
   - ✓ Restore each type correctly

---

## Implementation Estimate

### Phase 5.1 (Basic Multi-Subvolume):
- **Subvolume detection**: 30 minutes
- **Preferences UI**: 1 hour
- **Multi-snapshot creation**: 1 hour
- **Metadata updates**: 30 minutes
- **Rollback updates**: 1 hour
- **Testing**: 1 hour
- **Documentation**: 30 minutes

**Total: ~5-6 hours**

---

### Phase 5.2 (Advanced):
- **Selective restore UI**: 2 hours
- **Independent snapshots**: 2 hours
- **Per-subvolume scheduling**: 3 hours
- **Testing**: 2 hours

**Total: ~9 hours** (future)

---

## Risks & Mitigations

### Risk 1: User includes /home, loses data
**Mitigation:**
- Clear warnings in UI
- Extra confirmation dialog
- Explain consequences before enabling /home

### Risk 2: Atomic snapshot fails partway
**Mitigation:**
- Cleanup on failure
- All-or-nothing approach
- Log failures clearly

### Risk 3: Disk space exhaustion
**Mitigation:**
- Show size estimates in preferences
- Warn if enabling large subvolumes
- Check free space before snapshot

### Risk 4: Confusion about what's included
**Mitigation:**
- Show subvolume list in snapshot row
- Clear labels ("Includes: / • /home")
- Preferences easily accessible

---

## Summary

**Question:** Can we support multiple subvolumes like Snapper?

**Answer:** **YES!** It's definitely possible and not too complex.

**Effort:** ~5-6 hours for basic implementation

**Value:**
- More flexible than current approach
- Can separate system from user data
- Better matches Snapper's capabilities
- Still simpler to use than Snapper

**Recommendation:** Implement Phase 5.1 (basic multi-subvolume support) as next feature after Phase 4.

---

## Next Steps

Want me to implement this? I can start with:

1. ✅ Subvolume detection
2. ✅ Preferences UI
3. ✅ Multi-subvolume snapshot creation
4. ✅ Rollback support

Let me know! 🚀
