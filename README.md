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
- **Rollback Preview**: See package changes before restoring (added, removed, upgraded, downgraded)
- **Snapshot Integrity Verification**: Verify snapshot health with btrfs subvolume checks
- **Multi-Subvolume Support**: Snapshot root, /home, /var, and other Btrfs subvolumes
- **Package State Tracking**: Automatically records all installed packages with each snapshot
- **Package Diff Viewer**: Visual comparison of package changes between snapshots
- **Browse Snapshots**: Open snapshot directories in your file manager
- **Snapshot Search & Filter**: Find snapshots by name/description and filter by date ranges
- **Statistics Dashboard**: View storage usage, timeline graphs, and snapshot metrics

### Automation & Integration

- **Manual Snapshots**: Create snapshots before upgrades using waypoint-cli
- **Scheduled Snapshots**: Automatic periodic snapshots via runit service
  - Quick presets: Daily at 2 AM, Daily at Midnight, Weekly on Sunday
  - Live preview shows next snapshot time as you configure
- **Retention Policies**: Configurable automatic cleanup based on age and count
- **Preferences Dialog**: Configure which subvolumes to snapshot
- **D-Bus System Service**: Secure privilege-separated architecture
- **Environment Variable Configuration**: Override paths via WAYPOINT_SNAPSHOT_DIR, WAYPOINT_METADATA_FILE, etc.

### User Interface

- **Clean GTK4/libadwaita UI**: Modern interface following GNOME HIG
- **Real-time Disk Space Monitoring**: Header bar shows available space with color-coded warnings
  - Green (>20% free), Yellow (10-20%), Red (<10%)
  - Automatic updates every 30 seconds
- **Real-time Search**: Instant filtering as you type
- **Date Range Filters**: Quick filters for last 7/30/90 days
- **Confirmation Dialogs**: Native dialogs for all destructive actions
- **Rich Metadata Display**: Shows kernel version, packages, size, and creation date
- **Non-blocking Operations**: All expensive filesystem queries run in background threads

### Safety & Security

- **Privilege Separation**: GUI runs as user, operations run as privileged helper
- **Polkit Integration**: Secure authentication for privileged operations
- **Input Validation**: Comprehensive snapshot name validation prevents injection attacks
- **Path Validation**: Restricts file browser to allowed directories, prevents directory traversal
- **Automatic fstab Backup**: Creates timestamped backups before system modifications
- **Safety Checks**: Verifies Btrfs support and available disk space
- **Automatic Backups**: Creates backup before rollback operations
- **Atomic Operations**: All multi-subvolume operations are atomic

### Performance

- **TTL-based Caching**: Reduces expensive filesystem queries
  - 5-minute cache for snapshot size calculations
  - 30-second cache for available disk space
  - Thread-safe with automatic expiration
- **Memory Optimization**: Reference counting (Rc) for large data structures
  - Reduces snapshot cloning overhead from ~500KB to ~40 bytes (12,500x improvement)
  - Particularly beneficial with 500+ package snapshots

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
# - Runit scheduler service (optional, enable manually)
```

### Required: Mount Snapshots Directory

**Important:** Waypoint requires the snapshots subvolume to be mounted at `/.snapshots` to store and manage snapshots.

#### Why is this needed?

Waypoint stores snapshots in a dedicated Btrfs subvolume (typically `@snapshots`). This subvolume needs to be mounted at `/.snapshots` for Waypoint to create and access snapshots.

#### Setup Instructions

1. **Create the snapshots subvolume (if it doesn't exist):**
   ```bash
   # Find your Btrfs device
   DEVICE=$(findmnt -n -o SOURCE /)

   # Mount the Btrfs root temporarily
   sudo mkdir -p /mnt/btrfs-temp
   sudo mount -o subvolid=5 "$DEVICE" /mnt/btrfs-temp

   # Create the @snapshots subvolume if it doesn't exist
   sudo btrfs subvolume create /mnt/btrfs-temp/@snapshots

   # Unmount temporary mount
   sudo umount /mnt/btrfs-temp
   sudo rmdir /mnt/btrfs-temp
   ```

2. **Add to /etc/fstab:**
   ```bash
   # Replace /dev/mapper/VoidLinux with your actual device
   # You can find it with: findmnt -t btrfs /
   /dev/mapper/VoidLinux  /.snapshots  btrfs  subvol=/@snapshots,noatime  0 0
   ```

3. **Create mount point and mount:**
   ```bash
   sudo mkdir -p /.snapshots
   sudo mount /.snapshots
   ```

4. **Verify:**
   ```bash
   ls -la /.snapshots/
   # Should show an empty directory ready for snapshots
   findmnt /.snapshots
   # Should show the snapshots subvolume mounted
   ```

**Note:** Waypoint will store all snapshots in `/.snapshots/`. You can browse them directly from your file manager.

### Optional: Fallback to Btrfs Root Mount

If you prefer not to mount `/.snapshots` separately, Waypoint can fall back to using `/mnt/btrfs-root/@snapshots`. However, the `/.snapshots` mount is recommended for better organization and easier access.

<details>
<summary>Click to see fallback setup instructions</summary>

1. **Create mount point:**
   ```bash
   sudo mkdir -p /mnt/btrfs-root
   ```

2. **Mount the Btrfs root:**
   ```bash
   # Replace /dev/mapper/VoidLinux with your actual Btrfs device
   sudo mount -o subvolid=5,noatime /dev/mapper/VoidLinux /mnt/btrfs-root
   ```

3. **Make it permanent (add to /etc/fstab):**
   ```bash
   echo '/dev/mapper/VoidLinux  /mnt/btrfs-root  btrfs  subvolid=5,noatime  0 0' | sudo tee -a /etc/fstab
   ```

4. **Verify:**
   ```bash
   ls /mnt/btrfs-root/
   # You should see your subvolumes: @, @home, @snapshots, etc.
   ```

</details>

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

### Manual Pre-Upgrade Snapshots

Before upgrading your system, create a snapshot manually:

```bash
# Create a snapshot before upgrading
waypoint-cli create "before-upgrade" "Snapshot before system upgrade"

# Then run your upgrade
sudo xbps-install -Syu
```

You can also use the GUI to create a snapshot with a custom name and description.

### Scheduled Automatic Snapshots

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

### Environment Variables

Waypoint supports environment variable overrides for advanced configuration:

- **`WAYPOINT_SNAPSHOT_DIR`**: Override default snapshot directory (default: `/.snapshots`)
- **`WAYPOINT_METADATA_FILE`**: Custom metadata file location (default: `/var/lib/waypoint/snapshots.json`)
- **`WAYPOINT_SCHEDULER_CONFIG`**: Custom scheduler config path (default: `/etc/waypoint/scheduler.conf`)
- **`WAYPOINT_SERVICE_DIR`**: Custom service directory (default: `/etc/sv/waypoint-scheduler`)
- **`WAYPOINT_MIN_FREE_SPACE`**: Minimum free space threshold in bytes (default: 1GB)

**Example:**
```bash
# Use alternative snapshot directory
export WAYPOINT_SNAPSHOT_DIR="/mnt/backup/.snapshots"
waypoint

# Or for the CLI
WAYPOINT_SNAPSHOT_DIR="/mnt/backup/.snapshots" waypoint-cli list
```

These environment variables provide flexibility for non-standard setups, testing environments, or systems with custom Btrfs layouts.

### Managing Snapshots

Each snapshot card shows:
- Name and optional description
- Creation timestamp and kernel version
- Number of packages and storage size
- Which subvolumes are included

Available actions:
- **Browse**: Opens the snapshot directory in your file manager
- **Verify**: Check snapshot integrity with btrfs subvolume verification
  - Validates all subvolumes in the snapshot
  - Reports any errors or warnings
  - Helps identify corrupted or incomplete snapshots
- **Restore**: One-click system rollback
  - Shows preview of package changes before restoring
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
- **Void Linux Focused**: Designed specifically for Void Linux. May work on other distros with Btrfs but would require code updates due to runit integration.
- **System Reboot Required**: Rollback requires a reboot to boot into the restored snapshot.
- **No File-Level Restore**: Currently restores entire snapshots, not individual files (you can manually browse and copy files).

## Troubleshooting

### Cannot Create Snapshots

If you're unable to create snapshots, check the following:

1. **Verify snapshots directory is mounted (MOST COMMON ISSUE):**
   ```bash
   # Check if /.snapshots is mounted
   findmnt /.snapshots

   # Should show the @snapshots subvolume mounted
   # If not mounted, see "Required: Mount Snapshots Directory" section above

   # Alternative: check if fallback location exists
   ls -d /mnt/btrfs-root/@snapshots 2>/dev/null
   ```

2. **Verify D-Bus service is running:**
   ```bash
   ps aux | grep waypoint-helper
   ```

3. **Check D-Bus configuration:**
   ```bash
   # The config file should allow standard D-Bus interfaces
   cat /etc/dbus-1/system.d/tech.geektoshi.waypoint.conf
   ```

4. **Restart D-Bus (Void Linux with runit):**
   ```bash
   sudo pkill waypoint-helper
   sudo sv reload dbus
   ```

5. **Test D-Bus connection:**
   ```bash
   gdbus introspect --system --dest tech.geektoshi.waypoint --object-path /tech/geektoshi/waypoint
   ```

6. **Check polkit is running:**
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
