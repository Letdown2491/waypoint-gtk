# Phase 4: Testing Guide

## ✅ What Was Completed

All UI handlers have been integrated with the D-Bus client:

1. **Create Snapshot** - Uses `WaypointHelperClient::create_snapshot()`
2. **Delete Snapshot** - Uses `WaypointHelperClient::delete_snapshot()`
3. **Restore Snapshot** - Uses `WaypointHelperClient::restore_snapshot()`

All direct `btrfs::` calls that required sudo have been replaced with D-Bus calls that will trigger password prompts via Polkit.

---

## 🧪 Testing Instructions

### Step 1: Install

```bash
cd ~/Projects/waypoint-gtk
cargo build --release
sudo make install
```

This installs:
- `/usr/bin/waypoint` (GUI)
- `/usr/bin/waypoint-helper` (D-Bus service)
- `/usr/share/dbus-1/system-services/com.voidlinux.waypoint.service`
- `/etc/dbus-1/system.d/com.voidlinux.waypoint.conf`
- Polkit policy
- XBPS hook

### Step 2: Reload D-Bus

```bash
sudo systemctl restart dbus
```

This loads the new D-Bus service configuration.

### Step 3: Test Without Sudo

```bash
# Launch GUI WITHOUT sudo
waypoint
```

**Expected behavior:**
- Application launches successfully
- No "requires sudo" error

### Step 4: Test Create Snapshot

1. Click "Create Restore Point"
2. **EXPECTED**: Password prompt appears (Polkit authentication dialog)
3. Enter your password
4. **EXPECTED**: Snapshot is created, success message appears

**What's happening behind the scenes:**
```
Click button
  → D-Bus call to com.voidlinux.waypoint
  → D-Bus activates waypoint-helper as root
  → Polkit triggers password prompt
  → Helper creates snapshot
  → Result returned to GUI
```

### Step 5: Test Delete Snapshot

1. Select a snapshot
2. Click "Delete"
3. Confirm deletion
4. **EXPECTED**: Password prompt (if not cached from previous operation)
5. **EXPECTED**: Snapshot deleted successfully

### Step 6: Test Restore Snapshot

1. Select a snapshot
2. Click "Restore"
3. Read warnings, click "Restore and Reboot"
4. **EXPECTED**: Password prompt
5. **EXPECTED**: Success dialog with reboot prompt

**DON'T** actually reboot during testing unless you're prepared!

---

## 🐛 Troubleshooting

### Error: "Failed to connect to snapshot service"

**Cause**: D-Bus service not installed or D-Bus not reloaded

**Fix**:
```bash
# Check if service file exists
ls -l /usr/share/dbus-1/system-services/com.voidlinux.waypoint.service

# Check if policy exists
ls -l /etc/dbus-1/system.d/com.voidlinux.waypoint.conf

# Reload D-Bus
sudo systemctl restart dbus

# Check D-Bus logs
sudo journalctl -u dbus -f
```

### Error: "Authentication Required" (password prompt never appeared)

**Possible causes**:
1. Polkit not installed: `sudo xbps-install -S polkit`
2. Polkit agent not running (desktop environment issue)
3. D-Bus policy issue

**Check Polkit:**
```bash
# Check if Polkit is running
ps aux | grep polkit

# Check Polkit policy installed
ls -l /usr/share/polkit-1/actions/com.voidlinux.waypoint.policy
```

### Error: "Permission Denied"

**If password prompt appeared but operation failed:**

Check helper logs:
```bash
# Manual test of helper
sudo /usr/bin/waypoint-helper
# Leave running in terminal

# In another terminal, use GUI
waypoint
# Try creating a snapshot

# Watch output in first terminal
```

### D-Bus Connection Test

```bash
# List D-Bus services
busctl --system list | grep waypoint

# Should show: com.voidlinux.waypoint (when activated)

# Manual D-Bus call test
busctl --system call \
    com.voidlinux.waypoint \
    /com/voidlinux/waypoint \
    com.voidlinux.waypoint.Helper \
    ListSnapshots

# Should return JSON array (may ask for password)
```

---

## ✅ Success Criteria

Phase 4 is complete if:

- [x] GUI launches without sudo ✅
- [ ] Password prompt appears when creating snapshot
- [ ] Snapshot is created successfully
- [ ] Password prompt appears when deleting (if not cached)
- [ ] Snapshot is deleted successfully
- [ ] Password prompt appears when restoring
- [ ] Rollback operation succeeds
- [ ] No "requires sudo" errors anywhere

---

## 🎯 Expected User Experience

### Before Phase 4:
```
$ waypoint
Error: This operation requires root privileges. Please run with sudo.

$ sudo waypoint
[GUI opens but requires terminal]
```

### After Phase 4:
```
$ waypoint
[GUI opens from application menu]
[User clicks "Create Restore Point"]
[Password dialog appears]
[User enters password]
[✓ Snapshot created!]
```

**No terminal required! Full desktop integration!**

---

## 📝 Notes

### Password Caching

Polkit may cache authentication for a few minutes (default: 5 min). This means:
- First operation: Password prompt
- Subsequent operations: No prompt (cached)
- After timeout: Password prompt again

This is normal and expected behavior.

### Debugging Tips

1. **Check D-Bus activation**:
   ```bash
   # Watch for service activation
   sudo journalctl -f | grep waypoint
   ```

2. **Manual helper test**:
   ```bash
   # Run helper manually to see logs
   sudo /usr/bin/waypoint-helper
   # Then use GUI in another window
   ```

3. **Check permissions**:
   ```bash
   ls -l /usr/bin/waypoint-helper
   # Should be: -rwxr-xr-x root root
   ```

4. **Test D-Bus connection**:
   ```bash
   dbus-monitor --system | grep waypoint
   # Shows D-Bus traffic
   ```

---

## 🎉 When Everything Works

You should be able to:
1. Launch Waypoint from application menu (no terminal)
2. Create snapshots with just your user password
3. Delete snapshots with password prompt
4. Restore system with password prompt
5. All operations work seamlessly without sudo

**This is the full desktop application experience!**

---

Next step: Try it! Report any errors you encounter.
