# Waypoint

Waypoint is a GTK/libadwaita snapshot and rollback tool with a built in scheduling service for Btrfs filesystems on Void Linux.

For Void Linux users, Waypoint integration is available on [Nebula](https://github.com/Letdown2491/nebula-gtk) >= 1.3.0 to automatically create system snapshots when performing system upgrades.

## Screenshots

<p align="center">
  <img src="assets/screenshots/1-waypoint.png" alt="Waypoint UI" width="49%">
  <img src="assets/screenshots/2-waypointservice.png" alt="Waypoint Snapshot Scheduling" width="49%">
</p>
<p align="center">
  <img src="assets/screenshots/3-waypointquotas.png" alt="Waypoint Snapshot Quotas" width="49%">
  <img src="assets/screenshots/4-waypointbackups.png" alt="Waypoint Snapshot Backups" width="49%">
</p>

## Features

- **System Snapshots & Rollback** - Create, restore, compare, and verify snapshots with rollback preview showing package changes and affected subvolumes
- **Backup to External Drives** - Automatic backups with incremental support for Btrfs drives and full backups for NTFS/exFAT/network shares
- **Flexible Scheduling** - Multiple concurrent schedules (hourly, daily, weekly, monthly) with timeline-based retention policies
- **Package Tracking** - Automatic XBPS package state tracking with version comparison and visual diff viewer
- **Analytics Dashboard** - Snapshot statistics, space usage trends, and actionable insights
- **Quota Management** - Configure Btrfs quotas with automatic cleanup triggers and disk space warnings
- **Modern Interface** - GTK4 + libadwaita UI with theme switching, real-time monitoring, and command-line tools
- **Security First** - Privilege-separated architecture with D-Bus, Polkit, audit logging, and input validation

[See full feature list →](docs/FEATURES.md)

## Integration

Other Void Linux tools can talk to the privileged helper over D-Bus to trigger snapshots, retention, scheduler changes, and more. See [API.md](docs/API.md) for the complete interface used by Nebula and [ARCHITECTURE.md](docs/ARCHITECTURE.md) for a high-level overview of the components and on-disk layout.

## Requirements

- Void Linux with Btrfs filesystem
- GTK 4.10+ and libadwaita 1.4+ runtimes
- Rust 1.70 or newer
- DBus and Polkit
- Rsync for snapshot backups to non-Btrfs drives
- @snapshots subvolume mounted at `/.snapshots`

## Installation

```sh
./setup.sh install
```

The helper script builds the release binaries, installs them into `/usr/bin`, registers the desktop entry, D-Bus service, and Polkit policies. Use `sudo ./setup.sh uninstall` to remove those assets.

## Quick Start

```sh
cargo run --bin waypoint
```

## Production Build

```sh
cargo build --release
```

The optimized binaries are written to `target/release/`. Use `cargo run --release` to execute the release build directly after compiling.

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
