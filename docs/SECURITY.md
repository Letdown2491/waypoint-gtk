# Security Configuration Guide

This document provides guidance for securing Waypoint in production and security-sensitive environments.

## Authentication & Authorization

### Polkit Configuration

Waypoint uses Polkit for privilege escalation. The default policy (`tech.geektoshi.waypoint.policy`) uses `auth_admin_keep` which:
- Requires administrator authentication
- Caches credentials for convenience
- Has a default timeout of 5 minutes (system-dependent)

#### For Security-Sensitive Environments

For environments requiring stricter authentication:

**Option 1: Disable credential caching** (require auth each time)

Create `/etc/polkit-1/rules.d/50-waypoint-no-cache.rules`:
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

**Option 2: Passwordless for specific users** (automated systems)

Create `/etc/polkit-1/rules.d/50-waypoint-automation.rules`:
```javascript
polkit.addRule(function(action, subject) {
    if (action.id.indexOf("tech.geektoshi.waypoint") == 0) {
        // Allow waypoint-scheduler to run without password
        if (subject.user == "waypoint-scheduler") {
            return polkit.Result.YES;
        }
    }
});
```

**Option 3: Restrict to specific operations**

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
  "username": "martin",
  "pid": 12345,
  "operation": "snapshot_create",
  "resource": "pre-upgrade",
  "result": "success"
}
```

### Viewing Logs

```bash
# View waypoint operations
journalctl -u waypoint-helper

# Monitor in real-time
journalctl -u waypoint-helper -f

# Filter for audit events
journalctl -u waypoint-helper -t audit
```

### Logged Events

- Authorization checks (success/failure with reason)
- Snapshot creation/deletion (with user and timestamp)
- Restore operations (with snapshot name)
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

For additional production hardening with runit:

1. **Use chpst for resource limits** in `/etc/sv/waypoint-helper/run`:
```bash
#!/bin/sh
exec 2>&1
# Limit memory to 1GB and set nice priority
exec chpst -m 1073741824 -n 5 waypoint-helper
```

2. **Monitor resource usage**:
```bash
# Check service status
sv status waypoint-helper

# View logs
tail -f /var/log/waypoint-helper/current
```

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
- Default configuration is appropriate
- `auth_admin_keep` provides good UX balance

### Server/Automated
- Use Polkit rules for passwordless operation
- Set up dedicated service account
- Monitor logs for suspicious activity
- Configure resource limits via chpst in runit service

### Multi-User System
- Disable credential caching
- Enable detailed audit logging
- Review Polkit rules carefully
- Consider per-user quotas

### Security-Critical
- Require authentication for every operation
- Disable `auth_admin_keep`
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
