# Waypoint

A GTK-based snapshot and rollback tool for Void Linux.

**Version:** 0.4.0
**Status:** Production-ready with comprehensive features

## Overview

Waypoint provides a simple, user-friendly interface for creating filesystem snapshots (restore points) and managing them. It's designed specifically for Void Linux with Btrfs filesystems, making it easy to:

- Create restore points before system upgrades (manual or automatic)
- Browse and manage existing snapshots
- Roll back to previous system states with one click
- Compare package changes between snapshots
- Track retention policies for automatic cleanup
- Snapshot multiple subvolumes (/home, /var, etc.)

## Features

### Core Snapshot Management

- **One-Click System Rollback**: Full system restore with automatic backup creation
- **Multi-Subvolume Support**: Snapshot root, /home, /var, and other Btrfs subvolumes
- **Package State Tracking**: Automatically records all installed packages with each snapshot
- **Package Diff Viewer**: Visual comparison of package changes between snapshots
- **Browse Snapshots**: Open snapshot directories in your file manager
- **Snapshot Search & Filter**: Find snapshots by name/description and filter by date ranges
- **Statistics Dashboard**: View storage usage, timeline graphs, and snapshot metrics

### Automation & Integration

- **XBPS Hook Integration**: Automatically creates snapshots before system upgrades
- **Retention Policies**: Configurable automatic cleanup based on age and count
- **Preferences Dialog**: Configure which subvolumes to snapshot
- **D-Bus System Service**: Secure privilege-separated architecture

### User Interface

- **Clean GTK4/libadwaita UI**: Modern interface following GNOME HIG
- **Real-time Search**: Instant filtering as you type
- **Date Range Filters**: Quick filters for last 7/30/90 days
- **Confirmation Dialogs**: Native dialogs for all destructive actions
- **Rich Metadata Display**: Shows kernel version, packages, size, and creation date

### Safety & Security

- **Privilege Separation**: GUI runs as user, operations run as privileged helper
- **Polkit Integration**: Secure authentication for privileged operations
- **Safety Checks**: Verifies Btrfs support and available disk space
- **Automatic Backups**: Creates backup before rollback operations
- **Atomic Operations**: All multi-subvolume operations are atomic

## Requirements

### System Requirements

- Void Linux with a Btrfs filesystem (root required, other subvolumes optional)
- GTK4 (>= 4.10)
- libadwaita (>= 1.4)
- polkit (for privilege escalation)
- D-Bus system bus

### Build Requirements

- Rust (>= 1.70)
- cargo
- Development packages:
  ```bash
  sudo xbps-install -S base-devel pkg-config gtk4-devel libadwaita-devel
  ```

## Building

```bash
# Clone the repository
git clone <repository-url>
cd waypoint-gtk

# Build all binaries (waypoint + waypoint-helper)
make release

# Or using cargo directly
cargo build --release
```

## Installation

### Using Make (Recommended)

```bash
# Build and install everything
sudo make install

# This installs:
# - /usr/bin/waypoint (GUI application)
# - /usr/bin/waypoint-helper (privileged D-Bus service)
# - Desktop entry and Polkit policy
# - D-Bus service configuration
# - XBPS pre-upgrade hook
```

### Manual Installation

```bash
cargo build --release

# Install binaries
sudo install -Dm755 target/release/waypoint /usr/bin/waypoint
sudo install -Dm755 target/release/waypoint-helper /usr/bin/waypoint-helper

# Install data files
sudo install -Dm644 data/com.voidlinux.waypoint.desktop /usr/share/applications/com.voidlinux.waypoint.desktop
sudo install -Dm644 data/com.voidlinux.waypoint.policy /usr/share/polkit-1/actions/com.voidlinux.waypoint.policy
sudo install -Dm644 data/dbus-1/com.voidlinux.waypoint.service /usr/share/dbus-1/system-services/com.voidlinux.waypoint.service
sudo install -Dm644 data/dbus-1/com.voidlinux.waypoint.conf /etc/dbus-1/system.d/com.voidlinux.waypoint.conf

# Install XBPS hook (optional)
sudo install -Dm755 hooks/waypoint-pre-upgrade.sh /etc/xbps.d/waypoint-pre-upgrade.sh
sudo install -Dm644 hooks/waypoint.conf /etc/waypoint/waypoint.conf

# Create metadata directory
sudo install -dm755 /var/lib/waypoint
```

### Uninstallation

```bash
sudo make uninstall
```

## Usage

### Running the Application

Launch Waypoint from your application menu or run:

```bash
waypoint
```

The D-Bus helper service will start automatically when needed.

### Creating a Restore Point

1. Click the **"Create Restore Point"** button
2. Optionally enter a description for this snapshot
3. The snapshot will be created for all configured subvolumes
4. Package list is automatically captured
5. The new restore point appears in the list

**Note**: Snapshots are created via the privileged D-Bus service using polkit for authentication.

### Automatic Snapshots

If you've installed the XBPS hook, Waypoint will automatically create snapshots:

- Before running `xbps-install -Su` (system upgrade)
- Before installing new packages (configurable in `/etc/waypoint/waypoint.conf`)
- Named like `pre-upgrade-20251107-143000` for easy identification

### Managing Snapshots

Each snapshot card shows:
- Name and optional description
- Creation timestamp and kernel version
- Number of packages and storage size
- Which subvolumes are included

Available actions:
- **Browse** 📁: Opens the snapshot directory in your file manager
- **Restore** 🔄: One-click system rollback
  - Creates automatic backup before restoring
  - Updates Btrfs default subvolume
  - Prompts for reboot
- **Delete** 🗑️: Remove snapshot with confirmation

### Advanced Features

**Compare Snapshots:**
- Click the "Compare" button in the toolbar
- Select two snapshots to compare
- View added, removed, and updated packages

**Search & Filter:**
- Use the search box to find snapshots by name or description
- Quick filters: Last 7/30/90 days or show all
- Match count shows how many snapshots match

**Statistics:**
- Click the statistics button (📊) to view:
  - Total storage used by snapshots
  - Timeline graph showing snapshot creation
  - Package count trends

**Preferences:**
- Click the settings button (⚙️) to:
  - Choose which subvolumes to snapshot
  - Root (/) is always included
  - Select /home, /var, or other Btrfs subvolumes

## Architecture

Waypoint uses a **privilege-separated architecture** for security:

- **waypoint** (GUI): Runs as regular user, provides the GTK interface
- **waypoint-helper**: Runs as root via D-Bus activation, performs privileged operations
- **D-Bus**: Mediates communication between GUI and helper
- **Polkit**: Handles authentication and authorization

### Technology Stack

- **Rust**: Type-safe systems programming
- **GTK4 + libadwaita**: Modern GNOME-style UI
- **D-Bus (zbus)**: Inter-process communication
- **Btrfs**: Efficient copy-on-write snapshots
- **XBPS**: Package manager integration

### Project Structure

```
waypoint-gtk/
├── waypoint/                  # Main GUI application
│   ├── src/
│   │   ├── main.rs           # Application entry point
│   │   ├── dbus_client.rs    # D-Bus client for talking to helper
│   │   ├── snapshot.rs       # Snapshot metadata management
│   │   ├── packages.rs       # Package tracking and diff logic
│   │   ├── retention.rs      # Retention policy implementation
│   │   ├── subvolume.rs      # Subvolume detection
│   │   └── ui/
│   │       ├── mod.rs        # Main window
│   │       ├── snapshot_row.rs        # Snapshot list item
│   │       ├── create_snapshot_dialog.rs
│   │       ├── package_diff_dialog.rs # Package comparison UI
│   │       ├── statistics_dialog.rs   # Storage statistics
│   │       └── preferences.rs         # Subvolume preferences
│   └── Cargo.toml
├── waypoint-helper/           # Privileged D-Bus service
│   ├── src/
│   │   ├── main.rs           # D-Bus service implementation
│   │   ├── btrfs.rs          # Btrfs operations (requires root)
│   │   └── packages.rs       # Package list capture
│   └── Cargo.toml
├── waypoint-common/           # Shared types and definitions
│   ├── src/lib.rs
│   └── Cargo.toml
├── data/
│   ├── com.voidlinux.waypoint.desktop  # Desktop entry
│   ├── com.voidlinux.waypoint.policy   # Polkit policy
│   └── dbus-1/
│       ├── com.voidlinux.waypoint.service  # D-Bus service file
│       └── com.voidlinux.waypoint.conf     # D-Bus policy
├── hooks/
│   ├── waypoint-pre-upgrade.sh  # XBPS pre-upgrade hook
│   └── waypoint.conf            # Hook configuration
├── Makefile               # Build and installation
└── Cargo.toml            # Workspace definition
```

## Development Status

### ✅ Completed Phases

**Phase 1-2: Foundation** (v0.1-0.2)
- Basic snapshot creation, deletion, and browsing
- GTK4/libadwaita UI with confirmation dialogs
- Safety checks and error handling

**Phase 3: System Rollback** (v0.2.5)
- One-click system restore with automatic backups
- Package state tracking (xbps integration)
- Package diff viewer

**Phase 4: D-Bus Architecture** (v0.3.0)
- Privilege-separated architecture
- D-Bus system service with polkit
- Secure IPC between GUI and helper

**Phase 5: Multi-Subvolume Support** (v0.3.5)
- Snapshot multiple Btrfs subvolumes
- Subvolume preferences dialog
- Atomic multi-subvolume operations

**Phase 6: XBPS Integration** (v0.4.0)
- Pre-upgrade hook for automatic snapshots
- Configurable hook behavior
- Retention policy system

**Phase 7: UI Enhancements** (v0.4.0)
- Search and filter functionality
- Statistics dashboard
- Date range filters
- Enhanced metadata display

### 🚧 Future Enhancements

**Potential Features:**
- [ ] Retention policy GUI editor
- [ ] Scheduled automatic snapshots (cron/timer)
- [ ] Snapshot tagging system
- [ ] File-level diff viewer
- [ ] GRUB integration for boot-time recovery
- [ ] Snapshot export/import
- [ ] Non-Btrfs fallback (rsync-based)

## Known Limitations

- **Btrfs Only**: Currently only supports Btrfs filesystems. Non-Btrfs fallback is a potential future enhancement.
- **Read-Only Snapshots**: Snapshots are created as read-only for safety (by design).
- **Void Linux Focused**: Designed specifically for Void Linux and XBPS package manager. May work on other distros with Btrfs but untested.
- **System Reboot Required**: Rollback requires a reboot to boot into the restored snapshot.
- **No File-Level Restore**: Currently restores entire snapshots, not individual files (you can manually browse and copy files).

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

MIT License - see LICENSE file for details

## Credits

Developed for Void Linux users who want a simple, reliable way to manage system snapshots.
