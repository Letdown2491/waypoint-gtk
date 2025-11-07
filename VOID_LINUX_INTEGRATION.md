# Void Linux Integration: What Makes Waypoint Special

## TL;DR

**Waypoint is the ONLY Btrfs snapshot tool designed specifically for Void Linux with native XBPS integration.**

Other tools (Snapper, Timeshift, btrbk) are generic and don't understand XBPS or Void Linux's package management.

---

## Unique Features (Not Found in Other Tools)

### 1. **XBPS Pre-Upgrade Hook** ⚡
**What:** Automatically creates snapshots BEFORE system upgrades
**Where:** `/etc/xbps.d/waypoint-pre-upgrade.sh`

When you run `xbps-install -Su`, Waypoint automatically:
1. Detects you're about to upgrade packages
2. Creates a snapshot: `waypoint-pre-upgrade-20251107-143000`
3. Lets the upgrade proceed
4. If upgrade breaks something → Roll back in seconds via GUI

**Example:**
```bash
$ sudo xbps-install -Su
...
===============================================
Waypoint: Creating pre-upgrade snapshot...
===============================================
✓ Snapshot created: waypoint-pre-upgrade-20251107-143000
  You can roll back if the upgrade causes issues.
===============================================
...
[packages being upgraded]
```

**Configuration:** `/etc/waypoint/waypoint.conf`
```bash
# Enable/disable automatic snapshots
WAYPOINT_AUTO_SNAPSHOT=1

# Keep last 5 auto-snapshots, delete older ones
WAYPOINT_MAX_AUTO_SNAPSHOTS=5

# Require at least 2 GB free space
WAYPOINT_MIN_FREE_SPACE_GB=2
```

**Comparison:**
| Tool | Auto-Snapshot Before Upgrades | Void Linux Support |
|------|-------------------------------|-------------------|
| **Waypoint** | ✅ Yes (XBPS hook) | ✅ Native |
| Snapper | ❌ No (RPM/DEB only) | ❌ Generic |
| Timeshift | ❌ No | ❌ Generic |
| btrbk | ❌ No | ❌ Generic |

---

### 2. **XBPS Package Tracking** 📦
**What:** Every snapshot includes a complete list of installed packages with versions

When Waypoint creates a snapshot, it runs:
```bash
xbps-query -l
```

And stores the complete package list in the snapshot metadata.

**Implementation:** `waypoint/src/packages.rs`
```rust
pub fn get_installed_packages() -> Result<Vec<Package>> {
    let output = Command::new("xbps-query")
        .arg("-l")
        .output()?;

    // Parse: "ii firefox-120.0_1 Description"
    // Extract: name="firefox", version="120.0_1"
    ...
}
```

**Example Snapshot Metadata:**
```json
{
  "name": "waypoint-20251107-143000",
  "timestamp": "2025-11-07T14:30:00Z",
  "package_count": 847,
  "packages": [
    {"name": "firefox", "version": "120.0_1"},
    {"name": "rust", "version": "1.75.0_1"},
    {"name": "base-system", "version": "0.114_1"},
    ...
  ]
}
```

**Why This Matters:**
- See EXACTLY what was installed when snapshot was created
- Compare two snapshots to see what changed
- Understand why a snapshot might work/not work

**Comparison:**
| Tool | Package Tracking | Package Manager |
|------|------------------|-----------------|
| **Waypoint** | ✅ Complete list with versions | XBPS (Void) |
| Snapper | ✅ Yes | RPM (OpenSUSE/Fedora) |
| Timeshift | ❌ No | None |
| btrbk | ❌ No | None |

---

### 3. **Visual Package Diff** 🔍
**What:** GUI to compare packages between two snapshots

Click "Compare Snapshots" → Select two snapshots → See visual diff:

```
┌────────────────────────────────────────┐
│ Package Comparison                     │
├────────────────────────────────────────┤
│ Comparing: Before → After              │
│ 47 changes: 12 added, 5 removed, 30 updated
│                                        │
│ 📦 Packages Added                      │
│   • docker-24.0.7_1                    │
│   • docker-compose-2.23.0_1            │
│   • containerd-1.7.10_1                │
│                                        │
│ 📦 Packages Removed                    │
│   • python2-2.7.18_1                   │
│   • gtk+2-2.24.33_1                    │
│                                        │
│ 📦 Packages Updated                    │
│   • firefox: 119.0_1 → 120.0_1         │
│   • rust: 1.74.0_1 → 1.75.0_1          │
│   • linux6.6: 6.6.1_1 → 6.6.5_1        │
│   ...                                  │
└────────────────────────────────────────┘
```

**Use Cases:**
1. **Debugging:** "What package caused my system to break?"
2. **Audit:** "What changed during that upgrade?"
3. **Learning:** "What dependencies got pulled in?"

**Implementation:** `waypoint/src/ui/package_diff_dialog.rs`
- Compares two package lists
- Shows added, removed, and updated packages
- Color-coded: green (added), red (removed), blue (updated)

**Comparison:**
| Tool | Visual Package Diff | GUI |
|------|---------------------|-----|
| **Waypoint** | ✅ Yes (full GUI) | ✅ GTK4/libadwaita |
| Snapper | ⚠️ Yes (CLI only) | ❌ No GUI |
| Timeshift | ❌ No | ⚠️ Basic GTK3 |
| btrbk | ❌ No | ❌ No GUI |

---

### 4. **XBPS-Aware Package Parsing** 🔧
**What:** Correctly handles Void's unique package naming

Void Linux uses specific package naming:
```
package-name-version_revision
firefox-120.0_1
lib64-glibc-2.38_1
```

Waypoint's parser:
```rust
fn split_package_name_version(pkg: &str) -> Option<(&str, &str)> {
    // Finds the LAST dash followed by a digit
    // Handles: "lib64-glibc-2.38_1"
    //   name:  "lib64-glibc"
    //   version: "2.38_1"
}
```

**Why This Matters:**
- Other tools might parse "lib64-glibc-2.38_1" incorrectly
- Generic tools don't understand the `_revision` suffix
- Waypoint was built FOR Void Linux

---

## Complete Workflow Example

### Scenario: System Upgrade Goes Wrong

**Step 1: User upgrades system**
```bash
$ sudo xbps-install -Su
...
Waypoint: Creating pre-upgrade snapshot...
✓ Snapshot created: waypoint-pre-upgrade-20251107-143000
...
[upgrades 47 packages]
```

**Step 2: Something breaks after reboot**
```bash
$ waypoint
# Opens GUI, shows new snapshot automatically created
```

**Step 3: Compare snapshots to find culprit**
```
Click "Compare Snapshots"
Select: "Before upgrade" vs "After upgrade"

See visual diff:
📦 Packages Updated
  • xorg-server: 21.1.9_1 → 21.1.10_1
  • mesa: 23.3.0_1 → 23.3.1_1
  • nvidia: 545.29.06_1 → 550.40.07_1  ← Aha! Driver update!
```

**Step 4: Roll back**
```
Click restore button on "waypoint-pre-upgrade-20251107-143000"
Confirm → Reboot
System restored to working state!
```

**Step 5: Fix the issue**
```bash
# Block problematic package temporarily
$ sudo xbps-pkgdb -m hold nvidia

# Upgrade again (without nvidia)
$ sudo xbps-install -Su
# Waypoint creates ANOTHER pre-upgrade snapshot automatically!
```

---

## Why This Integration Matters

### For Void Linux Users

1. **Peace of Mind**
   - Every system upgrade automatically protected
   - No need to remember to create snapshots manually

2. **Debugging Made Easy**
   - See exactly what changed between working and broken states
   - Package-level granularity

3. **Native Integration**
   - Feels like a built-in Void tool
   - Works seamlessly with XBPS workflow

### Compared to Generic Tools

**Snapper (OpenSUSE/Fedora focus):**
- Designed for RPM-based systems
- No XBPS integration
- Would need manual hook configuration
- No understanding of Void package naming

**Timeshift (Ubuntu/Debian focus):**
- No package tracking at all
- No automatic snapshots
- Generic Btrfs operations only

**btrbk (Server focus):**
- Cron-based, not event-based
- No GUI
- No package awareness
- Designed for scheduled backups, not system upgrades

---

## Technical Implementation

### Files Involved

**XBPS Hook:**
- `hooks/waypoint-pre-upgrade.sh` - Shell script run by XBPS
- `hooks/waypoint.conf` - User-configurable settings
- Installed to: `/etc/xbps.d/` and `/etc/waypoint/`

**Package Tracking:**
- `waypoint/src/packages.rs` - XBPS query logic
- `waypoint-helper/src/packages.rs` - Root-level package access
- Metadata stored in: `/var/lib/waypoint/snapshots.json`

**Package Diff UI:**
- `waypoint/src/ui/package_diff_dialog.rs` - Visual diff dialog
- Uses native GTK4/libadwaita widgets
- Color-coded, scrollable, searchable (future)

### How the Hook Works

```
┌─────────────────────┐
│ User runs:          │
│ xbps-install -Su    │
└──────────┬──────────┘
           │
           v
┌─────────────────────────────┐
│ XBPS checks hooks dir       │
│ /etc/xbps.d/                │
└──────────┬──────────────────┘
           │
           v
┌─────────────────────────────┐
│ Finds & runs:               │
│ waypoint-pre-upgrade.sh     │
└──────────┬──────────────────┘
           │
           v
┌─────────────────────────────┐
│ Hook checks:                │
│ - Is Btrfs?                 │
│ - Is root?                  │
│ - Enough space?             │
│ - Is enabled in config?     │
└──────────┬──────────────────┘
           │
           v (all checks pass)
┌─────────────────────────────┐
│ Create snapshot:            │
│ btrfs subvolume snapshot    │
│   -r / /@snapshots/name     │
└──────────┬──────────────────┘
           │
           v
┌─────────────────────────────┐
│ XBPS proceeds with upgrade  │
│ (even if snapshot failed)   │
└─────────────────────────────┘
```

**Key Design Decision:**
Hook always exits with code 0, even on failure. This ensures system upgrades are never blocked by snapshot failures.

---

## Installation

The Void Linux integration is installed automatically:

```bash
sudo make install
```

This installs:
- ✅ XBPS hook: `/etc/xbps.d/waypoint-pre-upgrade.sh`
- ✅ Config: `/etc/waypoint/waypoint.conf`
- ✅ Binaries: `/usr/bin/waypoint`, `/usr/bin/waypoint-helper`
- ✅ D-Bus service, Polkit policy, desktop file

**Configuration:**
```bash
# Edit configuration
sudo nano /etc/waypoint/waypoint.conf

# Test the hook manually (as root)
sudo XBPS_TARGET_PHASE=pre /etc/xbps.d/waypoint-pre-upgrade.sh

# Disable auto-snapshots temporarily
sudo sed -i 's/WAYPOINT_AUTO_SNAPSHOT=1/WAYPOINT_AUTO_SNAPSHOT=0/' /etc/waypoint/waypoint.conf
```

---

## Future Enhancements

### Planned for Phase 6

1. **Hook Notification Integration**
   - Show desktop notification when auto-snapshot is created
   - "✓ Pre-upgrade snapshot created: waypoint-pre-upgrade-..."

2. **Post-Upgrade Success Tracking**
   - Mark snapshots as "Safe" after successful boot
   - Auto-delete very old "safe" snapshots
   - Keep "unsafe" snapshots longer

3. **Package Rollback Hints**
   - "Last known working version of nvidia: 545.29.06_1"
   - `xbps-install -f nvidia-545.29.06_1` suggestion

4. **XBPS Transaction Integration**
   - Show which packages will be upgraded BEFORE creating snapshot
   - Smarter snapshot naming: "upgrade-firefox-120" vs generic timestamp

---

## Summary: Why Waypoint is Special

| Feature | Waypoint | Snapper | Timeshift | btrbk |
|---------|----------|---------|-----------|-------|
| **Void Linux Support** | ✅ Native | ❌ No | ❌ No | ❌ No |
| **XBPS Integration** | ✅ Full | ❌ No | ❌ No | ❌ No |
| **Auto-Snapshot on Upgrade** | ✅ Yes | ⚠️ RPM only | ❌ No | ❌ No |
| **Package Tracking** | ✅ XBPS | ⚠️ RPM | ❌ No | ❌ No |
| **Visual Package Diff** | ✅ GUI | ⚠️ CLI only | ❌ No | ❌ No |
| **GUI** | ✅ Modern GTK4 | ❌ No | ⚠️ Basic | ❌ No |
| **Target Users** | 🎯 Void Linux Desktop | OpenSUSE/Fedora | Ubuntu/Mint | Servers |

**Bottom Line:**
Waypoint is the **ONLY** Btrfs snapshot tool built **specifically for Void Linux users** with **native XBPS integration** and a **modern GUI**.

Other tools are generic and require manual configuration. Waypoint works out of the box and understands Void Linux's package management.

🎯 **Built by Void users, for Void users.** 🎯
