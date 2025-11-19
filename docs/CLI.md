# Command Line Interface

Complete reference for `waypoint-cli`, the command-line tool for scriptable snapshot management.

## Table of Contents

- [Overview](#overview)
- [Installation](#installation)
- [Global Options](#global-options)
- [Snapshot Operations](#snapshot-operations)
- [Backup Operations](#backup-operations)
- [File Operations](#file-operations)
- [Quota Management](#quota-management)
- [Examples](#examples)

## Overview

`waypoint-cli` provides a command-line interface to Waypoint's snapshot management features. It communicates with the privileged `waypoint-helper` service via D-Bus, allowing scriptable automation of snapshots, backups, and restoration.

**Key features:**
- JSON output for scripting and automation
- Non-interactive operation for cron jobs and scripts
- Full access to all Waypoint functionality
- Detailed error messages and exit codes

## Installation

The CLI tool is installed automatically when running the setup script:

```sh
./setup.sh install
```

This installs `waypoint-cli` to `/usr/bin/waypoint-cli`.

## Global Options

Options that apply to all commands:

```sh
waypoint-cli --help              # Show help for all commands
waypoint-cli <command> --help    # Show help for specific command
waypoint-cli --version           # Show version information
```

Many commands support `--json` output for machine-readable results:

```sh
waypoint-cli list --json
waypoint-cli quota status --json
waypoint-cli scan-destinations --json
```

## Snapshot Operations

### List Snapshots

List all snapshots with optional details:

```sh
# Basic list (names only)
waypoint-cli list

# Verbose list with timestamps, descriptions, sizes
waypoint-cli list --verbose

# JSON output for scripting
waypoint-cli list --json
```

**Output format (verbose):**
```
snapshot-name | 2025-11-18 14:00:00 | Before upgrade | 2.5 GB
```

**JSON output:**
```json
[
  {
    "name": "snapshot-name",
    "timestamp": "2025-11-18T14:00:00Z",
    "description": "Before upgrade",
    "size_bytes": 2684354560,
    "subvolumes": ["/", "/home"],
    "pinned": false
  }
]
```

### Show Snapshot Details

Display detailed information about a specific snapshot:

```sh
waypoint-cli show "snapshot-name"
```

**Output:**
```
Name: snapshot-name
Created: 2025-11-18 14:00:00
Description: Before system upgrade
Size: 2.5 GB
Subvolumes: /, /home
Packages: 1,234
Pinned: No
```

### Create Snapshot

Create a new snapshot with optional description and subvolumes:

```sh
# Basic snapshot (root only)
waypoint-cli create "my-snapshot"

# With description
waypoint-cli create "my-snapshot" "Before installing new software"

# Multiple subvolumes
waypoint-cli create "my-snapshot" "Full backup" "/,/home,/var"

# JSON output
waypoint-cli create "my-snapshot" "Description" "/" --json
```

**Exit codes:**
- `0` - Success
- `1` - Creation failed (insufficient space, invalid name, etc.)

### Delete Snapshot

Remove a snapshot:

```sh
waypoint-cli delete "snapshot-name"
```

**Warning:** This is permanent and cannot be undone unless you have backups.

### Compare Snapshots

Show differences between two snapshots:

```sh
waypoint-cli diff "snapshot1" "snapshot2"
```

**Output shows:**
- Package changes (added, removed, upgraded, downgraded)
- File differences
- Subvolume changes

### Verify Snapshot

Check snapshot integrity:

```sh
# Human-readable output
waypoint-cli verify "snapshot-name"

# JSON output
waypoint-cli verify "snapshot-name" --json
```

**Output:**
```
Snapshot: snapshot-name
Status: Valid
Subvolumes verified: 2/2
Issues: None
```

### Restore Snapshot

Restore system to a previous snapshot:

```sh
# Preview changes before restoring
waypoint-cli preview-restore "snapshot-name"

# Perform actual restore (requires confirmation)
waypoint-cli restore "snapshot-name"
```

**Warning:** This will reboot your system after creating a safety backup.

### Apply Retention Policy

Manually trigger retention policy cleanup:

```sh
# Dry run (show what would be deleted)
waypoint-cli cleanup --dry-run

# Apply global retention policy
waypoint-cli cleanup

# Use per-schedule retention policies
waypoint-cli cleanup --schedule-based
```

**Output:**
```
Snapshots to delete: 5
- hourly-20251110-0800 (15 days old, 1.2 GB)
- hourly-20251110-0900 (15 days old, 800 MB)
...
```

## Backup Operations

### Scan for Backup Destinations

Detect available backup drives and network shares:

```sh
# Human-readable output
waypoint-cli scan-destinations

# JSON output
waypoint-cli scan-destinations --json
```

**Output:**
```
Found 2 backup destinations:
- /run/media/user/BackupDrive (Btrfs, 500 GB free)
- /mnt/nas/backups (ext4, 1.5 TB free)
```

### Create Backup

Backup a snapshot to an external drive:

```sh
# Full backup
waypoint-cli backup "snapshot-name" "/mnt/backup-drive"

# Incremental backup (Btrfs only)
waypoint-cli backup "snapshot-name" "/mnt/backup-drive" "parent-snapshot"
```

**Progress output:**
```
Backing up snapshot-name to /mnt/backup-drive...
[████████████████████████████----] 85% (2.1 GB / 2.5 GB) 45 MB/s
```

**Exit codes:**
- `0` - Backup successful
- `1` - Backup failed (destination unavailable, permission denied, etc.)

### List Backups

Show backups stored on a destination:

```sh
# Human-readable list
waypoint-cli list-backups "/mnt/backup-drive"

# JSON output
waypoint-cli list-backups "/mnt/backup-drive" --json
```

**Output:**
```
Backups on /mnt/backup-drive:
- snapshot-1 (2.5 GB, 2025-11-18 14:00)
- snapshot-2 (1.8 GB, 2025-11-17 14:00)
Total: 4.3 GB
```

### Verify Backup

Check backup integrity:

```sh
waypoint-cli verify-backup \
  "/mnt/backup-drive/waypoint-backups/snapshot-name" \
  "snapshot-name" \
  "/mnt/backup-drive"
```

**Output:**
```
Verifying backup: snapshot-name
File count: OK (12,345 files)
Total size: OK (2.5 GB)
Checksum validation: OK (100 files sampled)
Result: Backup is valid
```

### Restore from Backup

Restore a snapshot from an external backup:

```sh
waypoint-cli restore-backup "/mnt/backup-drive/waypoint-backups/snapshot-name"
```

This imports the backup back to `/.snapshots/` and makes it available for restoration.

### Drive Statistics

Show statistics for a backup destination:

```sh
waypoint-cli drive-stats "/mnt/backup-drive"
```

**Output:**
```
Drive: /mnt/backup-drive
Filesystem: btrfs
Total space: 500 GB
Available: 450 GB
Used by backups: 35 GB
Backup count: 12
Last backup: 2025-11-18 14:00:00
```

## File Operations

### Restore Individual Files

Restore specific files from a snapshot without full system rollback:

```sh
# Restore single file to original location
waypoint-cli restore-files "snapshot-name" "/etc/fstab"

# Restore multiple files (comma-separated)
waypoint-cli restore-files "snapshot-name" "/etc/fstab,/etc/hosts"

# Restore to custom location
waypoint-cli restore-files "snapshot-name" "/etc/fstab" --target /tmp/recovered

# Overwrite existing files
waypoint-cli restore-files "snapshot-name" "/etc/fstab" --overwrite

# Restore entire directory
waypoint-cli restore-files "snapshot-name" "/home/user/Documents"
```

**Exit codes:**
- `0` - Restoration successful
- `1` - File not found in snapshot or restoration failed

**Examples:**

```sh
# Recover accidentally deleted file
waypoint-cli restore-files "daily-20251117" "/home/user/important-file.txt"

# Restore old configuration
waypoint-cli restore-files "before-upgrade" "/etc/X11/xorg.conf" --overwrite

# Recover entire project directory
waypoint-cli restore-files "yesterday" "/home/user/Projects/myproject" \
  --target /tmp/recovered-project
```

## Quota Management

### Enable Quotas

Enable Btrfs quotas for snapshot space management:

```sh
# Enable traditional qgroups
waypoint-cli quota enable

# Enable simple quotas (recommended, better performance)
waypoint-cli quota enable --simple
```

**Note:** Enabling quotas may take several minutes on large filesystems.

### Disable Quotas

Disable Btrfs quotas:

```sh
waypoint-cli quota disable
```

### Show Quota Status

Display current quota usage:

```sh
# Human-readable output
waypoint-cli quota status

# JSON output
waypoint-cli quota status --json
```

**Output:**
```
Quota Status: Enabled
Mode: Simple quotas
Usage: 45 GB / 100 GB (45%)
Snapshots using space: 45 GB
Limit: 100 GB
Auto-cleanup: Enabled
```

**JSON output:**
```json
{
  "enabled": true,
  "mode": "simple",
  "used_bytes": 48318382080,
  "limit_bytes": 107374182400,
  "percentage": 45.0,
  "auto_cleanup": true
}
```

### Set Quota Limit

Configure maximum space for snapshots:

```sh
# Human-readable sizes
waypoint-cli quota set-limit 50G
waypoint-cli quota set-limit 100G
waypoint-cli quota set-limit 1T

# Supports K, M, G, T suffixes
waypoint-cli quota set-limit 512M
waypoint-cli quota set-limit 2048G
```

## Examples

### Daily Backup Script

Create a cron job for daily snapshots:

```sh
#!/bin/bash
# /etc/cron.daily/waypoint-backup

# Create daily snapshot
SNAPSHOT_NAME="daily-$(date +%Y%m%d)"
waypoint-cli create "$SNAPSHOT_NAME" "Automatic daily backup" "/,/home"

# Apply retention policy
waypoint-cli cleanup --schedule-based

# Backup to external drive if connected
if [ -d "/run/media/backup/BackupDrive" ]; then
    waypoint-cli backup "$SNAPSHOT_NAME" "/run/media/backup/BackupDrive"
fi
```

### Pre-Upgrade Snapshot

Create a snapshot before system upgrade:

```sh
#!/bin/bash
# Create snapshot with timestamp
SNAPSHOT_NAME="before-upgrade-$(date +%Y%m%d-%H%M)"

echo "Creating snapshot: $SNAPSHOT_NAME"
waypoint-cli create "$SNAPSHOT_NAME" "Before XBPS upgrade" "/,/home"

if [ $? -eq 0 ]; then
    echo "Snapshot created successfully"
    # Proceed with upgrade
    sudo xbps-install -Syu
else
    echo "Failed to create snapshot, aborting upgrade"
    exit 1
fi
```

### Monitor Quota Usage

Check if quota limit is approaching:

```sh
#!/bin/bash
# Alert if quota usage > 80%

QUOTA_JSON=$(waypoint-cli quota status --json)
PERCENTAGE=$(echo "$QUOTA_JSON" | jq -r '.percentage')

if (( $(echo "$PERCENTAGE > 80" | bc -l) )); then
    echo "WARNING: Snapshot quota usage is at ${PERCENTAGE}%"
    echo "Running cleanup..."
    waypoint-cli cleanup --dry-run
fi
```

### Automated File Recovery

Restore a file that gets corrupted daily:

```sh
#!/bin/bash
# Restore configuration file from yesterday's snapshot

YESTERDAY=$(date -d "yesterday" +daily-%Y%m%d)

waypoint-cli restore-files "$YESTERDAY" "/etc/myapp/config.conf" --overwrite

if [ $? -eq 0 ]; then
    echo "Configuration restored from $YESTERDAY"
    sudo sv restart myapp
else
    echo "Failed to restore configuration"
    exit 1
fi
```

### List Large Snapshots

Find snapshots using the most space:

```sh
#!/bin/bash
# Find top 5 largest snapshots

waypoint-cli list --json | \
  jq -r 'sort_by(.size_bytes) | reverse | .[:5] | .[] | "\(.name): \(.size_bytes / 1073741824 | round)GB"'
```

**Output:**
```
full-backup-20251115: 25GB
monthly-202510: 18GB
before-reinstall: 15GB
daily-20251110: 12GB
weekly-20251103: 10GB
```

### Verify All Snapshots

Check integrity of all snapshots:

```sh
#!/bin/bash
# Verify all snapshots and report issues

echo "Verifying all snapshots..."

waypoint-cli list --json | jq -r '.[].name' | while read -r snapshot; do
    echo -n "Checking $snapshot... "

    RESULT=$(waypoint-cli verify "$snapshot" --json)
    STATUS=$(echo "$RESULT" | jq -r '.status')

    if [ "$STATUS" = "Valid" ]; then
        echo "✓ OK"
    else
        echo "✗ FAILED"
        echo "$RESULT" | jq .
    fi
done
```

### Backup All Snapshots

Backup all snapshots to external drive:

```sh
#!/bin/bash
# Backup all snapshots when drive is connected

BACKUP_DEST="/run/media/backup/BackupDrive"

if [ ! -d "$BACKUP_DEST" ]; then
    echo "Backup drive not connected"
    exit 1
fi

echo "Backing up all snapshots to $BACKUP_DEST..."

waypoint-cli list --json | jq -r '.[].name' | while read -r snapshot; do
    echo "Backing up $snapshot..."
    waypoint-cli backup "$snapshot" "$BACKUP_DEST"
done

echo "Backup complete"
waypoint-cli drive-stats "$BACKUP_DEST"
```

## Exit Codes

All `waypoint-cli` commands use standard exit codes:

- `0` - Success
- `1` - General error (operation failed)
- `2` - Invalid arguments (wrong syntax, invalid options)
- `127` - Command not found (waypoint-helper not running)

## Error Handling

Common errors and solutions:

**"Failed to connect to D-Bus service"**
- Ensure `waypoint-helper` service is running
- Check D-Bus configuration: `sudo sv status dbus`

**"Not authorized to perform operation"**
- User may not be in `wheel` group
- Check Polkit policy: `/usr/share/polkit-1/actions/tech.geektoshi.waypoint.policy`

**"Insufficient disk space"**
- Free up disk space or increase quota limit
- Run cleanup: `waypoint-cli cleanup`

**"Snapshot not found"**
- Verify snapshot name: `waypoint-cli list`
- Check for typos in snapshot name

## See Also

- [USER_GUIDE.md](USER_GUIDE.md) - Complete user guide for Waypoint
- [API.md](API.md) - D-Bus API reference for integration
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues and solutions
