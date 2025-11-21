# Security Configuration Guide

This document provides guidance for securing Waypoint in production and security-sensitive environments.

## Authentication & Authorization

### Polkit Configuration

Waypoint uses Polkit for privilege escalation with two built-in rule files:

#### Built-in Polkit Rules

**50-waypoint-automated.rules** (Automation Policy)
- Allows root user to run all waypoint operations without authentication
- Enables the scheduler service to create snapshots automatically
- Located in `/etc/polkit-1/rules.d/50-waypoint-automated.rules`

**51-waypoint-desktop.rules** (Desktop-Friendly Policy)
- Applies to users in the `wheel` group in local sessions only
- Passwordless: Snapshot creation and system configuration
- Ask once per session (~5 min cache): Snapshot deletion
- Always ask (no cache): Restore operations
- Located in `/etc/polkit-1/rules.d/51-waypoint-desktop.rules`

The base policy (`tech.geektoshi.waypoint.policy`) defines four actions:
- `tech.geektoshi.waypoint.create-snapshot` - Create snapshots and backups
- `tech.geektoshi.waypoint.delete-snapshot` - Delete snapshots
- `tech.geektoshi.waypoint.restore-snapshot` - Restore system from snapshot
- `tech.geektoshi.waypoint.configure-system` - Modify schedules, quotas, and exclusions

**Read-only operations** (no authentication required):
- List snapshots (`list_snapshots`)
- Compare snapshots (`compare_snapshots`)
- Scan backup destinations (`scan_backup_destinations`)
- Get quota usage (`get_quota_usage`)
- Verify snapshots (`verify_snapshot`)
- Get scheduler status (`get_scheduler_status`)

#### For Security-Sensitive Environments

For environments requiring stricter authentication, you can override the built-in rules by creating higher-priority rules (60-99 range). The built-in rules can also be removed if needed.

**Option 1: Override with stricter rules** (require auth each time)

Create `/etc/polkit-1/rules.d/60-waypoint-strict.rules`:
```javascript
polkit.addRule(function(action, subject) {
    if (action.id.indexOf("tech.geektoshi.waypoint") == 0) {
        // Require auth every time, no caching
        if (subject.isInGroup("wheel") || subject.user == "root") {
            return polkit.Result.AUTH_ADMIN;
        }
    }
});
```

**Option 2: Remove built-in desktop-friendly policy**

If you want to require authentication for all operations:
```bash
sudo rm /etc/polkit-1/rules.d/51-waypoint-desktop.rules
```

This will fall back to the base policy which requires `auth_admin_keep` for all operations.

**Option 3: Customize for specific users**

```javascript
polkit.addRule(function(action, subject) {
    // Allow snapshots without password for admins
    if (action.id == "tech.geektoshi.waypoint.create-snapshot") {
        if (subject.isInGroup("wheel")) {
            return polkit.Result.YES;
        }
    }

    // But require password for restores (more dangerous)
    if (action.id == "tech.geektoshi.waypoint.restore-snapshot") {
        return polkit.Result.AUTH_ADMIN;
    }
});
```

### Authentication Timeouts

Waypoint enforces a **2-minute timeout** on Polkit authentication calls to prevent indefinite hangs. This is in addition to Polkit's own timeouts.

To configure Polkit's authentication dialog timeout, edit `/etc/polkit-1/polkit.conf`:
```ini
[Configuration]
# Timeout for authentication dialogs (in seconds)
# Default is 300 seconds (5 minutes)
AuthenticationAgentTimeout=60
```

## Input Validation

Waypoint performs extensive input validation:

### Snapshot Names
- Maximum 255 characters
- No path separators (`/`)
- No parent directory references (`..`)
- No null bytes
- Cannot start with `-` or `.`

### Snapshot Prefixes (for scheduled snapshots)
- Maximum 50 characters
- Only alphanumeric, dash, and underscore allowed
- Cannot start with `-` or `.`

### Path Validation
- All paths validated before use
- Path traversal attempts blocked
- Component-level validation for non-existent paths

### Symlink Security
Waypoint implements comprehensive symlink validation during file restoration and snapshot operations:

- **Absolute symlinks**: Validated to point within snapshot directory boundaries
- **Relative symlinks**: Resolved relative to symlink's parent directory, validated against escape attempts
- **Non-existent targets**: Manual path resolution using component analysis to detect escape attempts
- **Action on unsafe symlinks**: Logged and skipped during restoration operations
- **Logging**: All rejected symlinks logged to audit trail with reason
- **Exclusion protection**: Symlinks explicitly skipped during exclusion deletion to prevent deleting content outside snapshot boundaries.

### Configuration Files
- TOML files parsed and validated before saving
- Size limits enforced (10KB for legacy configs)
- Null byte detection

## Filesystem Security

### Snapshot Isolation

Snapshots are stored in `/.snapshots/` with:
- Read-only after creation
- Path boundary validation
- Symlink escape prevention

### Backup Security

#### Destination Filtering
Backup destinations are restricted through multiple layers:

- **System directory exclusions**: Prevents backups to /, /home, /boot, /var/*, /tmp/*, /sys/*, /proc/*, /dev/*
- **Snapshot exclusions**: Excludes /.snapshots and subvolumes with labels starting with "snapshot-"
- **Auto-mounted backup exclusions**: Filters out waypoint's own backup subvolumes
- **Trusted destination validation**: Only destinations from `scan_backup_destinations()` are accepted

#### Path Restrictions
External backups to non-Btrfs drives:
- Must be under `waypoint-backups/` directory on trusted destination
- Destination path canonicalized to resolve symlinks before validation
- Restore operations validate destination matches configured snapshot directory
- No arbitrary filesystem access allowed

## Quota Management

### Overflow Protection

Quota calculations use:
- Percentage clamping to prevent overflow (usage_percent clamped to 0.0-1.0 range)
- Validation of threshold values (rejects NaN, infinity)
- Checked arithmetic with safe defaults on invalid thresholds

### Limits

- Maximum quota value: 2^64-1 bytes (~18 exabytes)
- Warnings logged when approaching numerical limits
- Threshold values validated (0.0 to 1.0, finite)

## D-Bus Security

### Service Configuration

The D-Bus service configuration (`data/dbus-1/tech.geektoshi.waypoint.conf`) restricts:
- Service ownership to root only
- Method call permissions via Polkit
- Signal emissions

### Best Practices

1. **Don't modify D-Bus policy** without understanding implications
2. **Monitor logs** for authorization failures
3. **Use process identity verification** (PID + start time) - already implemented

### Command Injection Prevention

Waypoint prevents shell injection attacks through:

- **No shell execution**: All external commands use `Command::new()` with `.arg()` method
- **No string concatenation**: Arguments passed separately, never concatenated into shell strings
- **No `shell=true`**: Direct process execution without shell interpreter
- **Example**: `Command::new("btrfs").arg("subvolume").arg("snapshot").arg(&path)` instead of shell strings

## Audit Logging

Waypoint logs all privileged operations to syslog via the `log` crate with structured JSON format.

### Log Format

All security-relevant events are logged as JSON with:
- **Timestamp**: ISO 8601 format
- **User ID and username**: Attempted lookup from system
- **Process ID**: Caller's PID
- **Operation**: Action being performed
- **Resource**: Target (snapshot name, config type, etc.)
- **Result**: Success, failure, or denied
- **Details**: Optional error messages or additional context

Example log entry:
```json
{
  "timestamp": "2025-01-18T12:34:56Z",
  "user_id": 1000,
  "username": "user",
  "pid": 12345,
  "operation": "snapshot_create",
  "resource": "pre-upgrade",
  "result": "success"
}
```

### Viewing Logs

**Note:** waypoint-helper is D-Bus activated on-demand and does not run as a persistent service. Logs are primarily available from the scheduler service.

```bash
# View scheduler logs (runit/svlogd)
sudo tail -f /var/log/waypoint-scheduler/current

# View recent scheduler activity
sudo tail -100 /var/log/waypoint-scheduler/current

# Check scheduler service status
sudo sv status waypoint-scheduler
```

### Logged Events

- Authorization checks (success/failure with reason)
- Snapshot creation/deletion (with user and timestamp)
- Restore operations (with snapshot name)
- Writable snapshot cleanup (orphaned copies deleted during restore)
- Configuration changes (schedules, quotas, exclusions, snapshot metadata)
- Backup operations (creation, deletion, retention cleanup)
- Unsafe symlink rejections (with paths)
- Quota warnings (threshold breaches)
- Rate limiting triggers (user and operation)

## Rate Limiting

Waypoint implements per-user, per-operation rate limiting with a 5-second cooldown window to prevent DoS attacks via expensive snapshot operations.

### Monitoring
- **Mutex poisoning detection**: Global counter tracks mutex poisoning events in rate limiter
- **Alert threshold**: Critical log entry after 10 poisoning events
- **Purpose**: Detects potential bugs or malicious attacks causing mutex corruption
- **Action**: Review logs if CRITICAL messages appear for mutex poisoning

For additional production hardening with runit, you can configure resource limits for the scheduler service in `/etc/sv/waypoint-scheduler/run`:

```bash
#!/bin/sh
exec 2>&1
# Limit memory to 1GB and set nice priority
exec chpst -m 1073741824 -n 5 waypoint-scheduler
```

**Note:** `waypoint-helper` is D-Bus activated on-demand and cannot be managed as a persistent runit service. Resource limits for D-Bus activated services are not configurable through runit on Void Linux.

## TOCTOU Mitigation

Waypoint implements multiple defenses against Time-Of-Check-Time-Of-Use (TOCTOU) race conditions:

### Backup Restore Operations
- Paths canonicalized before use to resolve symlinks
- **Inode verification**: Captures inode number and device ID after canonicalization
- **Re-verification**: Inode checked again immediately before use to detect path swaps
- Explicit validation that destination matches configured snapshot directory
- Prevents malicious path swaps between check and use via inode tracking

### Recursive Directory Copy
- Each directory entry validated during iteration
- Ensures all entries remain within snapshot root during copy operation
- Prevents TOCTOU attacks where directory contents are swapped mid-operation
- Security check: `if !source_path.starts_with(snapshot_root)` on every entry

### Metadata Validation
- Snapshot names from metadata file validated before path resolution
- Invalid entries filtered out during load
- Prevents malicious metadata from causing path traversal

## Error Message Sanitization

Waypoint sanitizes error messages before displaying them to unprivileged clients to prevent information disclosure:

### Path Redaction
System paths are redacted from error messages:
- `/home/` → `<home>/`
- `/etc/` → `<etc>/`
- `/root/` → `<root>/`
- `/usr/` → `<usr>/`
- `/var/` → `<var>/`
- `/tmp/` → `<tmp>/`

### Message Truncation
- Error messages truncated to 500 characters maximum
- Prevents excessive information leakage
- Full errors logged server-side for administrators

### Purpose
- Prevents exposure of system layout details
- Protects against reconnaissance attacks
- Maintains audit trail with full details for authorized administrators

## Restore Integrity Verification

Waypoint automatically verifies the integrity of restored snapshots to detect corruption or incomplete restores:

### Verification Steps
- **Existence check**: Verifies restored path exists and is a directory
- **Subvolume validation**: For btrfs restores, validates proper subvolume structure
- **File count comparison**: Compares file counts between backup source and restored snapshot (must match exactly)
- **Size comparison**: Compares total sizes with 5% tolerance for filesystem overhead
- **Read access test**: Verifies restored data is readable

### Failure Detection
- File count mismatches indicate incomplete restore
- Size differences > 5% indicate potential data corruption
- Unreadable directories indicate permission or filesystem issues
- All failures logged with detailed diagnostic information

### Implementation
- Integrated into both btrfs send/receive and rsync restore operations
- Verification runs automatically after successful restore
- Failed verification returns detailed error preventing use of corrupted data

## Resource Cleanup

Waypoint implements comprehensive resource cleanup in error paths to prevent resource leaks:

### Cleanup Verification
- **Snapshot creation failures**: Automatically deletes partially created subvolumes
- **Restore failures**: Cleans up failed restore subvolumes before returning error
- **Metadata save failures**: Removes orphaned snapshots that aren't tracked in metadata
- **Comprehensive logging**: All cleanup operations logged with success/failure counts

### Implementation
- `cleanup_failed_snapshot()` function with detailed logging
- Cleanup triggered automatically on all error paths
- Prevents orphaned subvolumes and disk space leaks

## Network Security

Waypoint does not make network connections. All communication is local via:
- D-Bus (Unix domain sockets)
- Filesystem operations
- Local command execution

## Recommendations by Deployment Type

### Desktop/Workstation
- Default built-in rules (`51-waypoint-desktop.rules`) are appropriate
- Passwordless snapshots and backups provide seamless automatic operation
- Restore operations still require explicit confirmation for safety

### Server/Automated
- Built-in `50-waypoint-automated.rules` enables passwordless operation for root/scheduler
- Monitor logs for suspicious activity
- Configure resource limits via chpst in runit service
- Consider creating dedicated service user if needed (instead of root)

### Multi-User System
- Remove `51-waypoint-desktop.rules` to require authentication for all operations
- Enable detailed audit logging
- Review and customize Polkit rules carefully
- Consider per-user quotas

### Security-Critical
- Remove both built-in rule files to require authentication for every operation
- Create custom strict rules in `/etc/polkit-1/rules.d/60-waypoint-strict.rules`
- Set short authentication timeouts
- Enable comprehensive audit logging
- Review all operations in logs
- Consider AppArmor/SELinux profiles (not currently provided)

## Reporting Security Issues

If you discover a security vulnerability:

1. Open a public issue on Github
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

## Security Checklist

Before deploying Waypoint in production:

- [ ] Review and customize Polkit policy
- [ ] Configure appropriate authentication timeouts
- [ ] Set up audit logging
- [ ] Test backup and restore procedures
- [ ] Configure quota limits
- [ ] Review runit service configuration and resource limits
- [ ] Document incident response procedures
- [ ] Test privilege escalation paths
- [ ] Verify snapshot isolation
- [ ] Test input validation with malicious inputs

## Version Information

This security guide applies to:
- Waypoint version: 1.0.0 (early beta)
- Last updated: 2025-01-18

Refer to `CHANGELOG.md` for security-related updates in newer versions.
