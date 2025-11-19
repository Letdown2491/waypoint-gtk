# Troubleshooting Guide

Common issues and solutions for Waypoint.

## Table of Contents

- [Installation Issues](#installation-issues)
- [Scheduler Service Issues](#scheduler-service-issues)
- [Snapshot Issues](#snapshot-issues)
- [Backup Issues](#backup-issues)
- [Polkit Authentication Issues](#polkit-authentication-issues)
- [Quota Issues](#quota-issues)
- [Performance Issues](#performance-issues)

## Installation Issues

### Setup script fails with "command not found"

**Problem:** `./setup.sh install` fails because required tools are missing.

**Solution:**
```sh
# Install required dependencies
sudo xbps-install -S gtk4 libadwaita rust cargo dbus polkit rsync
```

### Permission denied when running setup.sh

**Problem:** Setup script is not executable.

**Solution:**
```sh
chmod +x setup.sh
./setup.sh install
```

### D-Bus service won't register

**Problem:** After installation, the D-Bus service doesn't appear.

**Solution:**
```sh
# Reload D-Bus configuration
sudo sv reload dbus
```

## Scheduler Service Issues

### Scheduler service shows "Stopped" or "Disabled"

**Problem:** The waypoint-scheduler service is not running.

**Solution:**
```sh
# Check if service exists
sudo sv status waypoint-scheduler

# If service doesn't exist, reinstall
./setup.sh install

# Start the service
sudo sv up waypoint-scheduler

# Check status
sudo sv status waypoint-scheduler
```

### Scheduled snapshots are not being created

**Problem:** Service is running but snapshots aren't created at scheduled times.

**Diagnosis:**
```sh
# Check service logs
sudo sv status waypoint-scheduler
cat /var/log/waypoint-scheduler/current
```

**Common causes:**
1. **Schedule is disabled** - Check Preferences → Schedules and ensure the schedule toggle is enabled
2. **Insufficient disk space** - Waypoint requires 1GB free space minimum
3. **Quota limits reached** - Check Preferences → Quotas
4. **Service crashed** - Check logs for errors

### Service keeps restarting

**Problem:** `sv status waypoint-scheduler` shows service restarting constantly.

**Solution:**
```sh
# Check logs for errors
sudo cat /var/log/waypoint-scheduler/current

# Common issues:
# - Missing snapshots directory: sudo mkdir -p /.snapshots
# - Permission issues: sudo chown root:root /usr/bin/waypoint-scheduler
# - D-Bus connection issues: sudo sv restart dbus
```

## Snapshot Issues

### Snapshots directory (/.snapshots) doesn't exist

**Problem:** Waypoint can't find `/.snapshots` subvolume.

**Solution:**
```sh
# Create snapshots subvolume
sudo btrfs subvolume create /.snapshots

# Verify it exists
sudo btrfs subvolume list / | grep snapshots
```

### "Cannot create snapshot: No space left on device"

**Problem:** Insufficient disk space.

**Solution:**
```sh
# Check disk space
df -h /

# Check quota usage if quotas are enabled
sudo btrfs qgroup show /

# Solutions:
# 1. Delete old snapshots manually
# 2. Enable retention policies (Preferences → Retention)
# 3. Increase quota limit (Preferences → Quotas)
# 4. Free up disk space
```

### Snapshots not appearing in the list

**Problem:** You created snapshots via CLI or scheduler but they don't show in GUI.

**Solution:**
1. Wait for auto-refresh (30 seconds) or press F5 / Ctrl+R to refresh manually
2. Check if search filter is hiding them (clear search box)
3. Check date filter (ensure "All" is selected)

### Rollback fails with "subvolume not found"

**Problem:** Trying to restore a snapshot fails.

**Solution:**
```sh
# Verify the snapshot exists
sudo btrfs subvolume list /.snapshots

# Check if snapshot is corrupted
waypoint-cli verify <snapshot-name>

# If corrupted, you may need to delete and use another snapshot
```

## Backup Issues

### Backup verification fails: "Not a valid btrfs subvolume"

**Problem:** Backup completed but verification reports it's not a valid subvolume.

**Cause:** This is normal for backups to non-Btrfs drives (NTFS, exFAT). Verification checks for directory structure instead of Btrfs subvolumes.

**Solution:** No action needed - this warning is informational for non-Btrfs backups.

### Backups to external drive not starting

**Problem:** Drive is connected but backups remain pending.

**Diagnosis:**
1. Check if drive is mounted: `lsblk` or `findmnt`
2. Check Preferences → Backups to see if drive is detected
3. Wait for auto-scan (every 5 seconds) or restart Waypoint

**Solution:**
```sh
# Manually mount the drive if needed
sudo mount /dev/sdX1 /mnt/backup

# Or check if drive is auto-mounted
ls /run/media/$USER/

# Ensure drive UUID matches configuration
lsblk -o NAME,UUID
```

### "Backup failed: Permission denied"

**Problem:** Can't write to backup destination.

**Solution:**
```sh
# Check permissions on backup destination
ls -ld /path/to/backup

# Fix permissions (for external drives)
sudo chown -R $USER:$USER /run/media/$USER/BackupDrive

# For NTFS drives, ensure ntfs-3g is installed
sudo xbps-install -S ntfs-3g
```

### Incremental backups taking too long

**Problem:** Btrfs send/receive backups are slow.

**Cause:** First backup is always full, subsequent backups are incremental.

**Solution:**
- First backup will be slow (full copy)
- Subsequent backups should be much faster (incremental)
- Use USB 3.0+ drives for better performance
- See [PERFORMANCE_TESTING.md](PERFORMANCE_TESTING.md) for optimization tips

## Polkit Authentication Issues

### Authentication dialog appears repeatedly

**Problem:** Polkit keeps asking for password even after entering it.

**Solution:**
```sh
# Check if Polkit policy is installed correctly
ls /usr/share/polkit-1/actions/tech.geektoshi.waypoint.policy

# Reinstall if missing
./setup.sh install

# Restart Polkit
sudo sv restart polkitd
```

### "Not authorized to perform operation"

**Problem:** User doesn't have permission to perform actions.

**Solution:**
1. Ensure your user is in the `wheel` group:
```sh
groups $USER

# Add to wheel group if missing
sudo usermod -aG wheel $USER
# Log out and back in
```

2. Check Polkit rules:
```sh
cat /usr/share/polkit-1/actions/tech.geektoshi.waypoint.policy
```

### Authentication timeout too short

**Problem:** Polkit authentication expires before operation completes.

**Solution:**
```sh
# Increase timeout (default: 120 seconds)
export WAYPOINT_POLKIT_TIMEOUT=300
waypoint

# Or add to your shell profile (~/.bashrc or ~/.zshrc)
echo 'export WAYPOINT_POLKIT_TIMEOUT=300' >> ~/.bashrc
```

## Quota Issues

### "Cannot enable quotas: already enabled"

**Problem:** Trying to enable quotas when already enabled.

**Solution:**
```sh
# Check quota status
sudo btrfs qgroup show /

# If you want to disable and re-enable:
sudo btrfs quota disable /
sudo btrfs quota enable /
```

### Quota limit prevents snapshot creation

**Problem:** Snapshots fail because quota limit is reached.

**Solution:**
1. Increase quota limit in Preferences → Quotas
2. Enable automatic cleanup (Preferences → Quotas → "Automatically delete old snapshots")
3. Manually delete old snapshots
4. Check quota usage:
```sh
sudo btrfs qgroup show -reF /
```

### Quotas showing "unknown" or incorrect values

**Problem:** Quota data is inconsistent.

**Solution:**
```sh
# Rescan quotas
sudo btrfs quota rescan /

# Wait for completion
sudo btrfs quota rescan -w /

# Restart Waypoint
```

## Performance Issues

### UI is slow with many snapshots (100+)

**Problem:** Snapshot list takes long to load with hundreds of snapshots.

**Cause:** Loading and sizing many snapshots is CPU/IO intensive.

**Solutions:**
1. Enable caching is already active (5-minute TTL for sizes)
2. Use search/date filters to reduce visible snapshots
3. Enable retention policies to keep snapshot count manageable
4. See [PERFORMANCE_TESTING.md](PERFORMANCE_TESTING.md) for optimization tips

### Disk space calculation is slow

**Problem:** "Checking disk space..." takes a long time.

**Cause:** `btrfs fi usage` is slow on large filesystems.

**Solutions:**
- Disk space is cached for 30 seconds, so subsequent checks are fast
- This is a known Btrfs limitation
- Consider using quotas for faster space checks

### Snapshot creation hangs

**Problem:** Creating snapshot freezes the UI.

**Diagnosis:**
```sh
# Check system load
top

# Check for Btrfs issues
sudo dmesg | grep -i btrfs

# Check disk I/O
iostat -x 1
```

**Solutions:**
- Large snapshots take time (this is normal)
- Ensure disk is healthy: `sudo btrfs device stats /`
- Check for filesystem errors: `sudo btrfs scrub start /`

## General Debugging

### Enable debug logging

For any issue, debug logs can help identify the problem:

```sh
# Run Waypoint with debug logging
RUST_LOG=debug waypoint 2>&1 | tee waypoint-debug.log

# For scheduler
RUST_LOG=debug waypoint-scheduler 2>&1 | tee scheduler-debug.log

# For helper (requires root)
sudo RUST_LOG=debug waypoint-helper
```

### Get system information

When reporting issues, include:

```sh
# Waypoint version
waypoint-cli --version

# System info
uname -a
btrfs --version

# Btrfs filesystem info
sudo btrfs fi show /
sudo btrfs fi usage /

# Service status
sudo sv status waypoint-scheduler
```

## Getting Help

If your issue isn't covered here:

1. Check the [FEATURES.md](FEATURES.md) to understand expected behavior
2. Review [SECURITY.md](SECURITY.md) for security-related questions
3. Check [ARCHITECTURE.md](ARCHITECTURE.md) to understand system design
4. Report issues at: https://github.com/Letdown2491/waypoint-gtk/issues

When reporting issues, please include:
- Waypoint version
- System information (Void Linux version, kernel version)
- Steps to reproduce
- Debug logs (RUST_LOG=debug)
- Expected vs actual behavior
