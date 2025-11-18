# Waypoint

NOTE: WAYPOINT IS CURRENTLY IN EARLY BETA. FEEL FREE TO USE IT, BUT KNOW THERE WILL STILL BE SOME BUGS.

Waypoint is a GTK/libadwaita snapshot and rollback tool with a built in scheduling service for Btrfs filesystems on Void Linux.

For Void Linux users, Waypoint integration is available on [Nebula](https://github.com/Letdown2491/nebula-gtk) >= 1.3.0 to automatically create system snapshots when performing system upgrades.

## Screenshots

<p align="center">
  <img src="assets/screenshots/1-waypoint.png" alt="Waypoint UI" width="49%">
  <img src="assets/screenshots/2-waypointservice.png" alt="Waypoint Snapshot Scheduling" width="49%">
</p>

## Features

### Snapshot Management
- One-click system rollback with automatic safety backup creation
- Rollback preview showing package changes, kernel version comparison, and affected subvolumes
- Snapshot integrity verification via D-Bus
- Multi-subvolume support (/, /home, /var, etc.) with atomic operations
- Individual file/directory restoration without full system rollback
- Snapshot comparison with file-level diff visualization
- Per-user snapshot notes and pinned favorites
- Search and filter with real-time text search and date ranges
- Browse snapshots in file manager via xdg-open

### Backup & Recovery
- Automatic backup to external drives with real-time progress tracking
- Incremental backups to Btrfs drives using btrfs send/receive
- Full backups to non-Btrfs drives (NTFS, exFAT, network shares) via rsync
- Automatic backup destination discovery and mount monitoring
- Backup verification with file count, size comparison, and SHA256 checksum validation
- Pending backup queue with automatic retry when destinations become available
- Restore snapshots from external backup
- Drive health statistics (space usage, backup count, timestamps)

### Package Management
- Automatic XBPS package state tracking on snapshot creation
- Package diff viewer with side-by-side comparison
- Package change categorization (added, removed, upgraded, downgraded)
- Version change tracking with visual indicators

### Scheduling & Automation
- Multiple concurrent snapshot schedules (hourly, daily, weekly, monthly)
- Per-schedule configuration with custom prefixes, descriptions, and subvolume selection
- Schedule-specific retention policies
- Runit service integration with live status monitoring
- Quick presets and live schedule preview
- Desktop notifications for scheduled snapshot creation

### Retention & Cleanup
- Timeline-based retention with configurable hourly, daily, weekly, monthly, and yearly buckets
- Global retention policies with max snapshots, max age, and minimum count protection
- Per-schedule retention policies for fine-grained control
- Keep patterns for pinned snapshots
- Real-time preview of snapshots to be deleted
- Dry-run mode for cleanup operations

### Analytics & Insights
- Snapshot analytics dashboard with overview statistics
- Space usage trends and growth analysis
- Actionable insights and recommendations
- Visual size comparison of largest snapshots

### Quota Management
- Enable/disable Btrfs quotas (simple quotas or traditional qgroups)
- Quota limit configuration with human-readable sizes
- Real-time quota usage monitoring
- Automatic quota-based cleanup triggers
- Disk space warnings before snapshot creation

### User Interface
- Modern GTK4 + libadwaita interface following GNOME HIG
- Theme switcher (system/light/dark mode)
- Real-time disk space monitoring with color-coded warnings
- Toast notifications for in-app feedback
- Desktop notifications for all major operations
- Auto-refresh UI every 30 seconds
- Command-line interface for scripting and automation

### Security & Performance
- Privilege-separated architecture with D-Bus and Polkit integration
- Structured JSON audit logging for all security events
- Rate limiting to prevent DoS attacks
- Input validation preventing command injection and path traversal
- Filesystem query caching with TTL for performance
- Parallel computation for snapshot size calculations
- Background threading for all blocking operations

## Integration

Other Void Linux tools can talk to the privileged helper over D-Bus to trigger snapshots, retention, scheduler changes, and more. See [API.md](API.md) for the complete interface used by Nebula and [ARCHITECTURE.md](ARCHITECTURE.md) for a high-level overview of the components and on-disk layout.

## Requirements

- Void Linux with Btrfs filesystem
- GTK 4.10+ and libadwaita 1.4+ runtimes
- Rust 1.70 or newer
- DBus and Polkit
- Rsync for snapshot backups to non-Btrfs drives
- @snapshots subvolume mounted at `/.snapshots`

## Quick Start

```sh
cargo run --bin waypoint
```

## Production Build

```sh
cargo build --release
```

The optimized binaries are written to `target/release/`. Use `cargo run --release` to execute the release build directly after compiling.

## System Install

```sh
./setup.sh install
```

The helper script builds the release binaries, installs them into `/usr/bin`, registers the desktop entry, D-Bus service, and Polkit policies. Use `sudo ./setup.sh uninstall` to remove those assets.

## Command Line

The CLI tool provides scriptable snapshot management:

### Snapshot Operations

```sh
# List all snapshots
waypoint-cli list
waypoint-cli list --verbose  # Show detailed info

# Show detailed information about a snapshot
waypoint-cli show "snapshot-name"

# Compare two snapshots
waypoint-cli diff "snapshot1" "snapshot2"

# Create a snapshot
waypoint-cli create "snapshot-name" "Optional description"
waypoint-cli create "snapshot-name" "Description" "/,/home"  # Multiple subvolumes

# Restore a snapshot
waypoint-cli restore "snapshot-name"

# Preview restore changes before committing
waypoint-cli preview-restore "snapshot-name"

# Verify snapshot integrity
waypoint-cli verify "snapshot-name"
waypoint-cli verify "snapshot-name" --json  # JSON output

# Delete a snapshot
waypoint-cli delete "snapshot-name"

# Apply retention policy cleanup
waypoint-cli cleanup
waypoint-cli cleanup --schedule-based  # Use per-schedule retention
waypoint-cli cleanup --dry-run  # Preview what would be deleted
```

### Backup Operations

```sh
# Scan for available backup destinations
waypoint-cli scan-destinations
waypoint-cli scan-destinations --json  # JSON output

# Create backup to external drive
waypoint-cli backup "snapshot-name" "/mnt/backup-drive"
waypoint-cli backup "snapshot-name" "/mnt/backup-drive" "parent-snapshot"  # Incremental

# List backups on destination
waypoint-cli list-backups "/mnt/backup-drive"
waypoint-cli list-backups "/mnt/backup-drive" --json  # JSON output

# Verify backup integrity
waypoint-cli verify-backup "/mnt/backup-drive/waypoint-backups/snapshot-name" \
  "snapshot-name" "/mnt/backup-drive"

# Restore from external backup
waypoint-cli restore-backup "/mnt/backup-drive/waypoint-backups/snapshot-name"

# Show drive statistics
waypoint-cli drive-stats "/mnt/backup-drive"
```

### File Operations

```sh
# Restore individual files from snapshot
waypoint-cli restore-files "snapshot-name" "/etc/fstab"
waypoint-cli restore-files "snapshot-name" "/etc/fstab,/etc/hosts"  # Multiple files
waypoint-cli restore-files "snapshot-name" "/etc/fstab" --target /tmp/recovered
waypoint-cli restore-files "snapshot-name" "/etc/fstab" --overwrite  # Overwrite existing
```

### Quota Management

```sh
# Enable quotas
waypoint-cli quota enable
waypoint-cli quota enable --simple  # Use simple quotas

# Disable quotas
waypoint-cli quota disable

# Show quota status
waypoint-cli quota status
waypoint-cli quota status --json  # JSON output

# Set quota limit
waypoint-cli quota set-limit 50G  # Human-readable size
waypoint-cli quota set-limit 1T   # Supports K, M, G, T
```

## Scheduler Service

Enable automatic periodic snapshots:

```sh
# Enable the scheduler service
sudo ln -s /etc/sv/waypoint-scheduler /var/service/

# Configure via GUI or edit /etc/waypoint/scheduler.conf
```
