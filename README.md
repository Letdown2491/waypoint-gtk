# Waypoint

A GTK-based snapshot and rollback tool for Btrfs filesystems on Void Linux.

## Overview

Waypoint provides a simple, user-friendly interface for creating filesystem snapshots (restore points) and managing them. It's designed for Btrfs filesystems, making it easy to:

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

- Linux system with Btrfs filesystem (root required, other subvolumes optional)
- GTK4 (>= 4.10)
- libadwaita (>= 1.4)
- polkit (for privilege escalation)
- D-Bus system bus
- Tested on Void Linux with runit

### Build Requirements

- Rust (>= 1.70)
- cargo
- Development packages:
  ```bash
  sudo xbps-install -S base-devel pkg-config gtk4-devel libadwaita-devel
  ```

## Installation

### Production Installation

```bash
# Clone the repository
git clone <repository-url>
cd waypoint-gtk

# Install everything (builds and installs system-wide)
./setup.sh install

# This installs:
# - /usr/bin/waypoint (GUI application)
# - /usr/bin/waypoint-helper (privileged D-Bus service)
# - /usr/bin/waypoint-cli (command-line interface)
# - Desktop entry and Polkit policy
# - D-Bus service configuration
# - XBPS pre-upgrade hook
# - Runit scheduler service (optional, enable manually)
```

### Development Setup

```bash
# Install dependencies (Void Linux)
sudo xbps-install -S base-devel pkg-config gtk4-devel libadwaita-devel

# Build debug version
cargo build

# Build release version
cargo build --release

# Run directly (without installing)
cargo run
```

### Uninstallation

```bash
# Remove Waypoint from the system
./setup.sh uninstall
```

## Usage

### Running the Application

Launch Waypoint from your application menu or run:

```bash
waypoint
```

The D-Bus helper service will start automatically when needed.

### Command Line Interface

Waypoint includes a command-line tool for scriptable snapshot management:

```bash
# List all snapshots
waypoint-cli list

# Create a snapshot
waypoint-cli create "my-backup" "Optional description"

# Delete a snapshot
waypoint-cli delete "snapshot-name"

# Restore a snapshot (rollback system)
waypoint-cli restore "snapshot-name"

# Show help
waypoint-cli help
```

**Note**: All CLI operations require authentication via Polkit, just like the GUI.

**Examples:**

```bash
# Create a backup before making system changes
waypoint-cli create "before-kernel-update" "Backup before updating kernel"

# List snapshots with jq for pretty output
waypoint-cli list | jq

# Automated backups in scripts
waypoint-cli create "daily-$(date +%Y%m%d)" "Automated daily backup"
```

### Creating a Restore Point

1. Click the **"Create Restore Point"** button
2. Optionally enter a description for this snapshot
3. The snapshot will be created for all configured subvolumes
4. Package list is automatically captured
5. The new restore point appears in the list

**Note**: Snapshots are created via the privileged D-Bus service using polkit for authentication.

### Automatic Snapshots

Waypoint supports two types of automatic snapshots:

**1. Pre-Upgrade Snapshots (XBPS Hook)**

If you've installed the XBPS hook, snapshots are created:
- Before running `xbps-install -Su` (system upgrade)
- Before installing new packages (configurable in `/etc/waypoint/waypoint.conf`)
- Named like `pre-upgrade-20251107-143000` for easy identification

**2. Scheduled Snapshots (Runit Service)**

Waypoint includes an optional scheduler service for periodic snapshots. You can configure it through the GUI (click the alarm icon in the toolbar) or manually edit the configuration file.

**GUI Configuration:**
- Click the alarm clock icon in the Waypoint toolbar
- Choose frequency (Hourly, Daily, Weekly, Custom)
- Set time and day for scheduled snapshots
- Configure snapshot name prefix
- View service status in real-time
- Save button will update config and restart the service automatically

**Manual Setup:**
```bash
# Enable the scheduler service
sudo ln -s /etc/sv/waypoint-scheduler /var/service/

# Check service status
sudo sv status waypoint-scheduler

# View logs
sudo tail -f /var/log/waypoint-scheduler/current

# Disable the scheduler
sudo sv stop waypoint-scheduler
sudo rm /var/service/waypoint-scheduler
```

**Configuration** (`/etc/waypoint/scheduler.conf`):
- **SCHEDULE_FREQUENCY**: `hourly`, `daily`, `weekly`, or `custom`
- **SCHEDULE_TIME**: Time of day for daily/weekly snapshots (e.g., `02:00`)
- **SCHEDULE_DAY**: Day of week for weekly (0=Sunday)
- **SNAPSHOT_PREFIX**: Prefix for snapshot names (default: `auto`)

**Examples:**
```bash
# Daily snapshots at 2 AM (default)
SCHEDULE_FREQUENCY="daily"
SCHEDULE_TIME="02:00"

# Weekly snapshots on Sunday at 3 AM
SCHEDULE_FREQUENCY="weekly"
SCHEDULE_DAY="0"
SCHEDULE_TIME="03:00"

# Hourly snapshots
SCHEDULE_FREQUENCY="hourly"
```

After editing the configuration, restart the service:
```bash
sudo sv restart waypoint-scheduler
```

### Managing Snapshots

Each snapshot card shows:
- Name and optional description
- Creation timestamp and kernel version
- Number of packages and storage size
- Which subvolumes are included

Available actions:
- **Browse**: Opens the snapshot directory in your file manager
- **Restore**: One-click system rollback
  - Creates automatic backup before restoring
  - Updates Btrfs default subvolume
  - Prompts for reboot
- **Delete**: Remove snapshot with confirmation

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

## Known Limitations

- **Btrfs Only**: Currently only supports Btrfs filesystems. Non-Btrfs fallback is a potential future enhancement.
- **Read-Only Snapshots**: Snapshots are created as read-only for safety (by design).
- **Void Linux Focused**: Designed specifically for Void Linux and XBPS package manager. May work on other distros with Btrfs but would require code updates due to XBPS and runit integrations.
- **System Reboot Required**: Rollback requires a reboot to boot into the restored snapshot.
- **No File-Level Restore**: Currently restores entire snapshots, not individual files (you can manually browse and copy files).

## Troubleshooting

### Cannot Create Snapshots

If you're unable to create snapshots, check the following:

1. **Verify D-Bus service is running:**
   ```bash
   ps aux | grep waypoint-helper
   ```

2. **Check D-Bus configuration:**
   ```bash
   # The config file should allow standard D-Bus interfaces
   cat /etc/dbus-1/system.d/tech.geektoshi.waypoint.conf
   ```

3. **Restart D-Bus (Void Linux with runit):**
   ```bash
   sudo pkill waypoint-helper
   sudo sv reload dbus
   ```

4. **Test D-Bus connection:**
   ```bash
   gdbus introspect --system --dest tech.geektoshi.waypoint --object-path /tech/geektoshi/waypoint
   ```

5. **Check polkit is running:**
   ```bash
   ps aux | grep polkitd
   ```

### Permission Denied Errors

If you get "Authorization failed" or "Permission denied" errors:

- Ensure polkit is installed and running
- Check that the polkit policy file is installed:
  ```bash
  ls -l /usr/share/polkit-1/actions/tech.geektoshi.waypoint.policy
  ```
- Verify your user has admin privileges
