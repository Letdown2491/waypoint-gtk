# Waypoint Recovery Guide

This guide helps you recover from boot failures or restore issues.

## Table of Contents

- [System Won't Boot After Restore](#system-wont-boot-after-restore)
- [Manual Subvolume Management](#manual-subvolume-management)
- [Accessing Snapshots from Live USB](#accessing-snapshots-from-live-usb)
- [Restoring from External Backup](#restoring-from-external-backup)
- [Emergency Rollback](#emergency-rollback)

---

## System Won't Boot After Restore

If your system fails to boot after restoring a snapshot, try these steps:

### 1. Boot from Live USB

1. Create a Void Linux live USB on another system
2. Boot from the USB
3. Open a terminal and become root: `sudo -i`

### 2. Mount Your Btrfs Filesystem

```bash
# Find your root partition
lsblk

# Mount it (replace /dev/sdXN with your partition)
mkdir /mnt/system
mount /dev/sdXN /mnt/system

# List available snapshots
btrfs subvolume list /mnt/system
```

### 3. Check Default Subvolume

```bash
# See which subvolume is set as default
btrfs subvolume get-default /mnt/system

# This should show the snapshot you tried to restore
# If it looks wrong, you can change it (see Manual Subvolume Management below)
```

### 4. Inspect the Snapshot's fstab

```bash
# Check if the snapshot's fstab exists and is valid
cat /mnt/system/.snapshots/<snapshot-name>/root/etc/fstab

# Or for writable copies:
cat /mnt/system/.snapshots/<snapshot-name>/root-writable/etc/fstab

# Verify that mount points are correct and reference the snapshot paths
```

---

## Manual Subvolume Management

If you need to manually change which snapshot boots:

### Change Default Subvolume

```bash
# Mount your root filesystem
mount /dev/sdXN /mnt/system

# List all subvolumes and find the ID you want
btrfs subvolume list /mnt/system

# Example output:
# ID 256 gen 123 top level 5 path @
# ID 257 gen 124 top level 5 path @snapshots/my-snapshot/root

# Set the desired subvolume as default (use the ID number)
btrfs subvolume set-default 256 /mnt/system

# Unmount and reboot
umount /mnt/system
reboot
```

### Restore to Original System

If you want to boot back to your original (non-snapshot) system:

```bash
# Mount filesystem
mount /dev/sdXN /mnt/system

# List subvolumes to find your original root (usually ID 256 or named @)
btrfs subvolume list /mnt/system

# Set original root as default
btrfs subvolume set-default 256 /mnt/system  # Use your original root ID

# Reboot
umount /mnt/system
reboot
```

### Use a Safety Snapshot

Waypoint creates automatic safety snapshots before each restore (named `waypoint-pre-rollback-*`):

```bash
# Mount filesystem
mount /dev/sdXN /mnt/system

# List snapshots to find the safety backup
ls -la /mnt/system/.snapshots/

# Find the safety snapshot (waypoint-pre-rollback-YYYYMMDD-HHMMSS)
# Get its subvolume ID
btrfs subvolume list /mnt/system | grep waypoint-pre-rollback

# Set it as default
btrfs subvolume set-default <ID> /mnt/system

# Reboot
umount /mnt/system
reboot
```

---

## Accessing Snapshots from Live USB

You can browse and copy files from any snapshot:

```bash
# Mount your filesystem
mount /dev/sdXN /mnt/system

# Navigate to snapshots
cd /mnt/system/.snapshots

# List available snapshots
ls -la

# Browse a specific snapshot
cd <snapshot-name>/root

# Copy files if needed
cp /mnt/system/.snapshots/<snapshot-name>/root/etc/fstab /tmp/
```

---

## Restoring from External Backup

If you have backups on external drives:

### 1. Connect External Drive

```bash
# List available drives
lsblk

# Mount the backup drive
mkdir /mnt/backup
mount /dev/sdXN /mnt/backup

# Navigate to backups
cd /mnt/backup/waypoint-backups
ls -la
```

### 2. Restore Using waypoint-cli

From a booted system (or Live USB with waypoint installed):

```bash
# Restore a backup (will create new snapshot in /.snapshots)
waypoint-cli restore-backup /mnt/backup/waypoint-backups/<snapshot-name> /.snapshots

# Verify the restoration
waypoint-cli verify <snapshot-name>

# If good, restore it
waypoint-cli restore <snapshot-name>
```

### 3. Manual Btrfs Receive (Advanced)

If waypoint-cli isn't available:

```bash
# For Btrfs backups, use btrfs receive
mount /dev/sdXN /mnt/system
mkdir -p /mnt/system/.snapshots

# Receive the backup
btrfs send /mnt/backup/waypoint-backups/<snapshot-name> | \
  btrfs receive /mnt/system/.snapshots/

# Set as default and reboot
btrfs subvolume set-default <new-snapshot-id> /mnt/system
reboot
```

---

## Emergency Rollback

If Waypoint won't start but your system boots:

### Using CLI

```bash
# List snapshots
waypoint-cli list

# Restore to a working snapshot
waypoint-cli restore <snapshot-name>

# System will reboot
```

### Without Waypoint (Manual)

```bash
# Become root
sudo -i

# List subvolumes
btrfs subvolume list /

# Find a good snapshot and get its ID
btrfs subvolume list / | grep <snapshot-name>

# Set as default
btrfs subvolume set-default <ID> /

# Reboot
reboot
```

---

## Common Issues

### Error: "fstab validation failed"

The snapshot's `/etc/fstab` is malformed. From Live USB:

1. Mount your filesystem
2. Edit the snapshot's fstab: `nano /mnt/system/.snapshots/<name>/root-writable/etc/fstab`
3. Fix mount points to reference snapshot paths correctly
4. Save and reboot

### Error: "Cannot restore multi-subvolume snapshot: /etc/fstab not found"

The snapshot is missing `/etc/fstab`. Solutions:

1. **Use a different snapshot** that has fstab
2. **Copy fstab from another snapshot**:
   ```bash
   cp /mnt/system/.snapshots/<good-snapshot>/root/etc/fstab \
      /mnt/system/.snapshots/<broken-snapshot>/root/etc/fstab
   ```
3. **Restore only root subvolume** (not multi-subvolume)

### Filesystem Won't Mount

```bash
# Check filesystem for errors
btrfs check --readonly /dev/sdXN

# If errors found, you may need repair (DANGEROUS - backup first!)
# btrfs check --repair /dev/sdXN
```

---

## Prevention Tips

1. **Always verify snapshots** before restoring: `waypoint-cli verify <name>`
2. **Keep multiple working snapshots** - don't rely on just one
3. **Test restores in VM first** if possible
4. **Maintain external backups** for critical data
5. **Document your Btrfs layout** (subvolume structure, device names)

---

## Getting Help

If you're still stuck:

1. Check [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
2. File an issue: [https://github.com/Letdown2491/waypoint-gtk/issues](https://github.com/Letdown2491/waypoint-gtk/issues)
3. Join the Void Linux Reddit community and request help: [https://www.reddit.com/r/voidlinux/](https://www.reddit.com/r/voidlinux/)

---

**Remember:** Waypoint creates automatic safety snapshots before each restore. You can always boot from Live USB and manually change the default subvolume back to a known-good state.
