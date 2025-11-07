# Waypoint vs Snapper - What's the Difference?

## TL;DR

**Snapper** is a powerful, mature CLI tool with enterprise features and complex configuration.

**Waypoint** is a simple, desktop-focused GUI app that prioritizes ease of use over features.

---

## Feature Comparison

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Primary Interface** | GUI (GTK4/libadwaita) | CLI (with optional GUI frontends) |
| **Target Audience** | Desktop users | Power users, sysadmins, enterprises |
| **Configuration** | Minimal (just works) | Complex YAML/XML configs |
| **Authentication** | Polkit (GUI password prompt) | sudo/root required |
| **Snapshot Creation** | Manual + pre-upgrade hook | Automatic scheduling + pre/post hooks |
| **Package Tracking** | Built-in (xbps) | Via plugins/integration |
| **Package Diff View** | Visual GUI dialog | CLI output or web interface |
| **Rollback** | One-click with warnings | Multi-step CLI commands |
| **Snapshot Scheduling** | No (manual only) | Yes (hourly, daily, weekly, etc.) |
| **Retention Policies** | No | Yes (complex cleanup rules) |
| **Multiple Subvolumes** | No (root only) | Yes (home, /var, etc.) |
| **Quota Management** | No | Yes |
| **Distribution Integration** | Void Linux | openSUSE (primary), many others |
| **Maturity** | New (2025) | Mature (2011+) |
| **Lines of Code** | ~2,500 | ~30,000+ |

---

## Design Philosophy

### Snapper: "Power and Flexibility"

**Philosophy:**
- Give administrators complete control
- Support complex enterprise scenarios
- Maximum configurability
- CLI-first, automation-friendly

**Typical Use Case:**
```bash
# Create config for root filesystem
snapper -c root create-config /

# Set retention limits
snapper -c root set-config "NUMBER_LIMIT=50"
snapper -c root set-config "TIMELINE_CREATE=yes"

# Manual snapshot
snapper -c root create --description "Before upgrade"

# List snapshots
snapper -c root list

# Rollback (multi-step)
snapper -c root rollback 42
reboot
snapper -c root rollback  # Confirm after testing
```

**Complexity:** High - requires understanding configs, subvolume IDs, multiple commands

---

### Waypoint: "Simple and Safe"

**Philosophy:**
- Just works out of the box
- GUI-first for desktop users
- Safety over flexibility
- Clear visual feedback

**Typical Use Case:**
```
1. Launch Waypoint from app menu
2. Click "Create Restore Point"
3. Enter password when prompted
4. [Snapshot created!]

Later, if something breaks:
1. Open Waypoint
2. Select snapshot
3. Click "Restore"
4. Confirm warnings
5. Enter password
6. Reboot when prompted
7. [System restored!]
```

**Complexity:** Low - GUI guides you through each step

---

## Detailed Comparison

### 1. User Interface

#### Snapper:
- **CLI-first**: `snapper list`, `snapper create`, `snapper rollback`
- **Optional GUIs**:
  - `snapper-gui` (Qt-based, basic)
  - YaST module (openSUSE only)
  - Web interface (snapper-web)
- **Power user friendly**: Great for scripts and automation
- **Steep learning curve**: Must understand Btrfs concepts

#### Waypoint:
- **GUI-only**: Modern GNOME app with GTK4 + libadwaita
- **Visual design**:
  - Card-based snapshot list
  - Color-coded buttons (blue=create, red=delete)
  - Toast notifications for feedback
- **Desktop-first**: Designed for casual users
- **Easy to learn**: If you can use GNOME Settings, you can use Waypoint

---

### 2. Snapshot Creation

#### Snapper:
```bash
# Automatic snapshots (if configured)
snapper -c root create --type pre --description "zypper install"
# ... run package manager ...
snapper -c root create --type post --description "zypper install"

# Manual snapshots
snapper -c root create --description "My snapshot"

# Timeline snapshots (automatic)
# Creates snapshots every hour by default
```

**Features:**
- ✅ Pre/post transaction pairs
- ✅ Automatic timeline snapshots
- ✅ Multiple snapshot types
- ✅ Full automation

**Complexity:** Must configure timeline, limits, cleanup

#### Waypoint:
```
Click button → Enter password → Done!
```

**Features:**
- ✅ Manual snapshots
- ✅ Pre-upgrade hook (XBPS integration)
- ✅ Package list automatically captured
- ❌ No automatic scheduling
- ❌ No timeline snapshots

**Complexity:** Zero configuration needed

---

### 3. Package Tracking

#### Snapper:
- **Indirect**: Works with RPM database on openSUSE
- **Comparison**: `snapper diff 1..2` shows file changes
- **Not built-in**: Requires distribution integration
- **Focus**: File-level changes, not package-level

**Example:**
```bash
$ snapper diff 1..2
+... /usr/bin/firefox  # Changed file
-... /etc/old-config   # Removed file
```

#### Waypoint:
- **Direct**: Captures `xbps-query -l` output for each snapshot
- **Visual**: GUI dialog shows:
  - ✅ Added packages (green)
  - ❌ Removed packages (red)
  - 🔄 Updated packages with versions (blue)
- **Built-in**: Always works, no configuration

**Example:**
```
Compare Snapshots: 2025-01-06 → 2025-01-07

📦 Packages Added:
• firefox-122.0_1
• thunderbird-115.7_1

📦 Packages Removed:
• old-pkg-1.0_1

📦 Packages Updated:
• linux-6.6.54 → linux-6.7.1
• gtk4-4.12.0 → gtk4-4.12.1
```

---

### 4. Rollback Process

#### Snapper:
**Multi-step process:**

```bash
# 1. Find the snapshot
snapper list

# 2. Understand what changed
snapper status 1..0

# 3. Test rollback first (read-only)
snapper rollback 42

# 4. Reboot and test

# 5. If good, make permanent
snapper rollback

# 6. If bad, rollback the rollback
btrfs subvolume set-default ...
```

**Features:**
- ✅ Transactional rollback (can test then commit)
- ✅ Fine-grained control
- ✅ Can rollback specific subvolumes
- ❌ Complex, easy to make mistakes
- ❌ Requires Btrfs knowledge

#### Waypoint:
**One-click process:**

```
1. Select snapshot
2. Click "Restore"
3. Read warnings
4. Click "Restore and Reboot"
5. Enter password
6. Choose "Reboot Now"
7. [System restored after reboot]
```

**Features:**
- ✅ Automatic pre-rollback backup created
- ✅ Clear warnings shown upfront
- ✅ Single operation
- ❌ All-or-nothing (no testing first)
- ❌ Whole system only (not individual subvolumes)

---

### 5. Configuration

#### Snapper:
**Location:** `/etc/snapper/configs/root`

**Example Config:**
```ini
SUBVOLUME="/"
FSTYPE="btrfs"

# Timeline snapshots
TIMELINE_CREATE="yes"
TIMELINE_CLEANUP="yes"

# Retention limits
TIMELINE_MIN_AGE="1800"
TIMELINE_LIMIT_HOURLY="10"
TIMELINE_LIMIT_DAILY="10"
TIMELINE_LIMIT_WEEKLY="0"
TIMELINE_LIMIT_MONTHLY="10"
TIMELINE_LIMIT_YEARLY="10"

# Number limits
NUMBER_CLEANUP="yes"
NUMBER_MIN_AGE="1800"
NUMBER_LIMIT="50"
NUMBER_LIMIT_IMPORTANT="10"

# Disk space management
EMPTY_PRE_POST_CLEANUP="yes"
EMPTY_PRE_POST_MIN_AGE="1800"
```

**Complexity:** Must understand all options and their interactions

#### Waypoint:
**Location:** `/etc/waypoint/waypoint.conf`

**Config:**
```bash
# Enable automatic snapshots before system upgrades
WAYPOINT_AUTO_SNAPSHOT=1

# Maximum number of auto-created snapshots to keep
WAYPOINT_MAX_AUTO_SNAPSHOTS=5

# Minimum free space required (GB)
WAYPOINT_MIN_FREE_SPACE_GB=2
```

**Complexity:** 3 simple on/off settings

---

### 6. Integration

#### Snapper:

**openSUSE (native):**
- ✅ YaST integration
- ✅ Zypper integration (automatic pre/post snapshots)
- ✅ GRUB integration (boot from snapshot)
- ✅ Rollback from GRUB menu
- ✅ Deep system integration

**Other distros:**
- Partial support
- Requires manual setup
- May not integrate with package manager

#### Waypoint:

**Void Linux (designed for):**
- ✅ XBPS hook (pre-upgrade snapshots)
- ✅ Polkit integration
- ✅ D-Bus activation
- ❌ No GRUB integration (yet)
- ❌ No boot-time selection (yet)

**Other distros:**
- Would need porting (package manager integration)

---

## Use Cases

### When to Use Snapper:

✅ **Server environments** - Need automation and scheduling
✅ **Enterprise deployments** - Require complex retention policies
✅ **Multiple subvolumes** - Want to snapshot /home separately
✅ **openSUSE users** - Native integration is excellent
✅ **Power users** - Comfortable with CLI and configuration
✅ **Automated systems** - Integration with CI/CD pipelines
✅ **Need quota management** - Disk space limits per config
✅ **Timeline snapshots** - Want automatic hourly/daily backups

**Example Scenario:**
> "I run a fleet of servers and need automatic snapshots every hour with a 30-day retention policy. I want to rollback /home independently from root."

---

### When to Use Waypoint:

✅ **Desktop users** - Want a simple GUI experience
✅ **Void Linux users** - Native XBPS integration
✅ **Beginners** - Don't understand Btrfs internals
✅ **Casual use** - Just want system restore points before upgrades
✅ **GNOME desktop** - Fits perfectly with modern GNOME design
✅ **Visual feedback** - Want to see package differences in GUI
✅ **Minimal config** - Don't want to manage complex settings
✅ **One-click operations** - Value simplicity over features

**Example Scenario:**
> "I'm running Void Linux on my laptop and want an easy way to create restore points before system upgrades, with a GUI that looks like other GNOME apps."

---

## Architecture

### Snapper:

```
┌─────────────────────────────────────────┐
│           snapper CLI                   │
├─────────────────────────────────────────┤
│         libsnapper (C++)                │
│  ┌──────────────────────────────────┐   │
│  │ Config Management                │   │
│  │ Timeline Logic                   │   │
│  │ Cleanup Algorithms               │   │
│  │ Comparison Engine                │   │
│  │ Quota Management                 │   │
│  └──────────────────────────────────┘   │
├─────────────────────────────────────────┤
│          Btrfs Operations               │
├─────────────────────────────────────────┤
│        Linux Kernel (Btrfs)             │
└─────────────────────────────────────────┘

Optional GUIs:
┌──────────┐  ┌──────────┐  ┌───────────┐
│ YaST     │  │snapper-gui│  │snapper-web│
│ (Qt/ncurses)│  (Qt)     │  (Web)      │
└──────────┘  └──────────┘  └───────────┘
```

**Characteristics:**
- C++ core library
- Designed as a service
- Multiple frontends possible
- Complex codebase (~30k+ LOC)

---

### Waypoint:

```
┌─────────────────────────────────────────┐
│     Waypoint GUI (GTK4/libadwaita)      │
│              User Space                 │
└──────────────┬──────────────────────────┘
               │ D-Bus
               │ (with Polkit auth)
               ↓
┌─────────────────────────────────────────┐
│     waypoint-helper (privileged)        │
│              Root Space                 │
├─────────────────────────────────────────┤
│  • Btrfs snapshot operations            │
│  • Package list capture (xbps-query)    │
│  • Metadata management                  │
├─────────────────────────────────────────┤
│        Linux Kernel (Btrfs)             │
└─────────────────────────────────────────┘

Architecture:
┌────────────┐   D-Bus    ┌───────────────┐
│  Waypoint  │ ─────────→ │ Helper (root) │
│   (user)   │ ←───────── │   (D-Bus)     │
└────────────┘  Results   └───────────────┘
```

**Characteristics:**
- Rust implementation
- Privilege separation (GUI runs as user)
- Single integrated interface
- Small codebase (~2.5k LOC)

---

## Performance

### Snapper:
- **Snapshot speed**: Instant (Btrfs COW)
- **List speed**: Can be slow with many snapshots
- **Comparison**: File-by-file diff can be slow
- **Cleanup**: Background process manages retention
- **Overhead**: Higher due to features and timeline

### Waypoint:
- **Snapshot speed**: Instant (Btrfs COW)
- **List speed**: Fast (simple JSON metadata)
- **Comparison**: Fast (package list diff, not file diff)
- **Cleanup**: Manual deletion only
- **Overhead**: Minimal (no background processes)

---

## Documentation & Learning Curve

### Snapper:
**Learning Curve:** Steep

**Must Learn:**
- Btrfs subvolume concepts
- Snapper config file format
- CLI commands and options
- Snapshot types (pre/post/timeline)
- Cleanup algorithms
- Rollback procedures

**Time to Productivity:** Hours to days

**Documentation:**
- Man pages (`man snapper`)
- openSUSE wiki (extensive)
- Arch wiki (community docs)

---

### Waypoint:
**Learning Curve:** Gentle

**Must Learn:**
- How to click buttons 😊
- Read confirmation dialogs

**Time to Productivity:** Minutes

**Documentation:**
- Tooltips in app
- Visual UI guides the way
- This comparison doc!

---

## Real-World Example

### Scenario: "System upgrade broke WiFi"

#### With Snapper:

```bash
# 1. SSH in from another machine (WiFi broken!)
ssh user@laptop

# 2. List snapshots
snapper list
# NUMBER | TYPE   | DATE                | DESCRIPTION
# 42     | single | 2025-01-06 14:30:00 | Before upgrade

# 3. Check what changed
snapper status 42..0
# +..... /lib/firmware/wifi-driver.bin

# 4. Do rollback
snapper rollback 42

# 5. Reboot
reboot

# 6. Test if WiFi works

# 7. If good, make permanent
snapper rollback

# 8. If bad, rollback again
# (more commands...)
```

**Time:** 10-15 minutes + debugging
**Skill Required:** Must know CLI, Btrfs concepts, troubleshooting

---

#### With Waypoint:

```
1. Plug in ethernet cable (WiFi broken!)
2. Open Waypoint from app menu
3. See snapshot: "waypoint-pre-upgrade-20250106"
4. Click "Restore"
5. Read warning
6. Click "Restore and Reboot"
7. Enter password
8. Click "Reboot Now"
9. [System reboots]
10. WiFi works again!
```

**Time:** 2-3 minutes
**Skill Required:** Can use GNOME desktop

---

## Code Complexity

### Snapper (C++):
```
snapper/
├── client/        (~2000 LOC)
├── server/        (~3000 LOC)
├── snapper/       (~15000 LOC)
├── dbus/          (~2000 LOC)
├── scripts/       (~1000 LOC)
└── ...
Total: ~30,000+ lines
```

**Dependencies:**
- libxml2
- libboost
- dbus-1
- libmount
- libbtrfs
- PAM

---

### Waypoint (Rust):
```
waypoint/
├── waypoint/        (~800 LOC)  # GUI
├── waypoint-helper/ (~500 LOC)  # Privileged ops
├── waypoint-common/ (~100 LOC)  # Shared types
└── ...
Total: ~2,500 lines
```

**Dependencies:**
- gtk4
- libadwaita
- zbus (D-Bus)
- serde (JSON)

---

## Future Roadmap

### Snapper (mature):
- ✅ Feature-complete
- Ongoing maintenance
- Bug fixes
- Distribution-specific improvements

### Waypoint (new):
- 🚧 GRUB integration (boot from snapshot)
- 🚧 Scheduled snapshots (optional)
- 🚧 ext4 support (rsync-based)
- 🚧 File-level diffs
- 🚧 More Linux distros

---

## Summary Table

| Aspect | Snapper | Waypoint |
|--------|---------|----------|
| **Best For** | Servers, power users | Desktop users |
| **Complexity** | High | Low |
| **Features** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ |
| **Ease of Use** | ⭐⭐ | ⭐⭐⭐⭐⭐ |
| **GUI Quality** | ⭐⭐ | ⭐⭐⭐⭐⭐ |
| **Automation** | ⭐⭐⭐⭐⭐ | ⭐ |
| **Configuration** | Complex | Minimal |
| **Learning Curve** | Steep | Gentle |
| **Maturity** | Very mature | New |
| **Integration** | openSUSE⭐⭐⭐⭐⭐ | Void Linux⭐⭐⭐⭐ |

---

## Conclusion

**Snapper** and **Waypoint** solve the same problem (Btrfs snapshots) but target different audiences:

### Choose Snapper if you:
- Run servers or multi-user systems
- Need automatic scheduling and retention policies
- Want fine-grained control over snapshot management
- Use openSUSE (native integration)
- Prefer CLI tools
- Need to manage multiple subvolumes
- Want quota management

### Choose Waypoint if you:
- Run a desktop with GNOME
- Want a simple, visual interface
- Use Void Linux
- Don't need automatic snapshots (manual is fine)
- Value ease of use over features
- Want one-click rollback
- Prefer GUI tools that "just work"

---

**Bottom Line:**

> Snapper is a **Swiss Army knife** - powerful but complex.
>
> Waypoint is a **single-purpose tool** - does one thing really well.

Both are excellent tools, just for different use cases! 🚀
