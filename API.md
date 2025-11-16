# Waypoint D-Bus API

Waypoint exposes a privileged D-Bus helper so GUI applications, schedulers, or external tools such as Nebula can perform snapshot work without running as root. This document captures the contract shipped by the `waypoint-helper` service so third-party Void tools can integrate safely.

## Bus Overview

- **Bus**: system
- **Service name**: `tech.geektoshi.waypoint`
- **Object path**: `/tech/geektoshi/waypoint`
- **Interface**: `tech.geektoshi.waypoint.Helper`
- **Activation**: systemd/runit launches `/usr/bin/waypoint-helper` on demand through `data/dbus-1/tech.geektoshi.waypoint.service`

The helper always runs as root and enforces authorization by delegating to Polkit. Callers connect over the system bus (`zbus::ConnectionBuilder::system()` in `waypoint-helper/src/main.rs`) and the service stays idle until a client invokes it.

## Authorization

Most write operations require one of four Polkit actions exposed in `data/tech.geektoshi.waypoint.policy`:

| Action ID | Permission scope | Example methods |
| --- | --- | --- |
| `tech.geektoshi.waypoint.create-snapshot` | Create/backup snapshot data | `CreateSnapshot`, `BackupSnapshot`, `ListBackups` |
| `tech.geektoshi.waypoint.delete-snapshot` | Delete snapshots | `DeleteSnapshot`, `CleanupSnapshots` |
| `tech.geektoshi.waypoint.restore-snapshot` | Roll back or read snapshot contents | `RestoreSnapshot`, `RestoreFiles`, `CompareSnapshots`, `GetQuotaUsage`, `RestoreFromBackup` |
| `tech.geektoshi.waypoint.configure-system` | Scheduler/quota configuration | `UpdateSchedulerConfig`, `SaveSchedulesConfig`, `RestartScheduler`, `EnableQuotas`, `DisableQuotas`, `SetQuotaLimit`, `SaveQuotaConfig` |

Read-only helpers such as `ListSnapshots`, `VerifySnapshot`, `GetSchedulerStatus`, and `ScanBackupDestinations` do not require authentication. For write calls, Polkit may display a password prompt depending on local policy. The helper identifies callers via `org.freedesktop.DBus.GetConnectionUnixProcessID` plus `/proc/$PID/stat` start times (see `check_authorization` in `waypoint-helper/src/main.rs`).

## Signals

The interface currently emits one signal:

- `SnapshotCreated(string snapshot_name, string created_by)`
  - Fired when `CreateSnapshot` completes successfully.
  - `created_by` is either `"gui"` or `"scheduler"` depending on the caller’s bus name.

## Methods

All method names here are camel-cased in code but appear Capitalized on the bus because of zbus’ default mapping (e.g., `create_snapshot` → `CreateSnapshot`). Return tuples follow `(bool success, string message)` unless otherwise noted. JSON payloads are covered in [JSON Payloads](#json-payloads).

### Snapshot lifecycle

- **CreateSnapshot** `(s name, s description, as subvolumes) → (b success, s message)`  
  Creates read-only Btrfs snapshots for the requested mount points. Requires `create-snapshot`. Emits `SnapshotCreated` on success.

- **DeleteSnapshot** `(s name) → (b, s)`  
  Removes the snapshot directories. Requires `delete-snapshot`.

- **RestoreSnapshot** `(s name) → (b, s)`  
  Configures the system to boot into a snapshot, automatically creating a safety snapshot first. Requires `restore-snapshot`. A reboot is mandatory for changes to apply.

- **ListSnapshots** `() → s json`  
  Returns a JSON array of `SnapshotInfo` objects. No authentication required.

- **VerifySnapshot** `(s name) → s json`  
  Returns a `VerificationResult` JSON document summarizing any integrity errors or warnings. Read-only.

- **PreviewRestore** `(s name) → (b success, s json)`  
  Produces a `RestorePreview` JSON document describing package, kernel, and subvolume changes that a rollback would introduce. Requires `restore-snapshot`.

- **CleanupSnapshots** `(b schedule_based) → (b, s)`  
  Applies retention based on either per-schedule policies (`true`) or global legacy settings (`false`). Requires `delete-snapshot`.

### Scheduler configuration (runit)

- **UpdateSchedulerConfig** `(s legacy_conf) → (b, s)`  
  Writes the legacy `/etc/waypoint/scheduler.conf`. The modern GUI uses `SaveSchedulesConfig`, but the method remains for compatibility. Requires `configure-system`.

- **SaveSchedulesConfig** `(s schedules_toml) → (b, s)`  
  Persists the structured `schedules.toml` file (see `WaypointConfig::schedules_config`). Requires `configure-system`.

- **RestartScheduler** `() → (b, s)`  
  Runs `sv restart waypoint-scheduler`. Requires `configure-system`.

- **GetSchedulerStatus** `() → s status`  
  Returns `"running"`, `"stopped"`, `"disabled"`, or `"unknown"` by inspecting `/var/service/waypoint-scheduler` and `sv status`. No authentication required.

### File operations and diffing

- **RestoreFiles** `(s snapshot_name, as file_paths, s target_directory, b overwrite) → (b, s)`  
  Restores individual files or directories from a snapshot to their original paths (empty `target_directory`) or a custom directory. Requires `restore-snapshot`.

- **CompareSnapshots** `(s old_snapshot, s new_snapshot) → (b, s json)`  
  Runs `btrfs send --no-data` internally and returns a JSON list of changed files. Large comparisons may exceed the 25 s D-Bus timeout in zbus 4.x. Requires `restore-snapshot`.

### Quotas

- **EnableQuotas** `(b use_simple) → (b, s)`  
  Enables simple quotas (`true`) or traditional qgroups (`false`). Requires `configure-system`.

- **DisableQuotas** `() → (b, s)`  
  Disables quotas entirely. Requires `configure-system`.

- **GetQuotaUsage** `() → (b, s json)`  
  Returns serialized `QuotaUsage` metrics. Requires `restore-snapshot`.

- **SetQuotaLimit** `(t limit_bytes) → (b, s)`  
  Updates the total snapshot space limit. Requires `configure-system`.

- **SaveQuotaConfig** `(s quota_toml) → (b, s)`  
  Writes `/etc/waypoint/quota.toml`. Requires `configure-system`.

### Scheduler-aware retention & package metadata

- **SaveSchedulesConfig** and **RestartScheduler** allow GUI tools to push new TOML schedules and bounce the runit unit without shelling out as root.
- `CleanupSnapshots` uses current schedules when invoked with `schedule_based = true`, matching how the scheduler service enforces per-policy retention.

### Backup and external media

- **ScanBackupDestinations** `() → (b, s json)`  
  Lists mounted Btrfs destinations (USB, network, etc.) as `BackupDestination` JSON structures. Read-only.

- **BackupSnapshot** `(s snapshot_path, s destination_mount, s parent_snapshot) → (b success, s result, t size_bytes)`  
  Runs `btrfs send|receive` into `<destination>/waypoint-backups`. `parent_snapshot` may be empty for full backups. On success `result` is the new backup path; on failure it contains an error string. Requires `create-snapshot`.

- **ListBackups** `(s destination_mount) → (b, s json)`  
  Returns a JSON array of absolute subvolume paths below `<destination>/waypoint-backups`. Requires `create-snapshot`.

- **RestoreFromBackup** `(s backup_path, s snapshots_dir) → (b, s)`  
  Receives a backup into the live snapshots directory. Requires `restore-snapshot`.

### Miscellaneous

- **UpdateSchedulerConfig**, **SaveSchedulesConfig**, **SaveQuotaConfig** all create parent directories if missing, so callers just supply the full serialized file contents.
- Most `(b, s)` calls keep `success=false` paired with a human-readable error message; callers should treat a returned `Err` as transport failure and inspect `success` otherwise.

## JSON Payloads

Waypoint serializes several structs directly; consumers should deserialize them to work with the response data.

- **SnapshotInfo** (from `waypoint-common/src/lib.rs`)

```json
{
  "name": "2025-11-01T12-00-00",
  "timestamp": "2025-11-01T12:00:00Z",
  "description": "Before xbps-install",
  "package_count": 1023,
  "packages": [{"name": "foo", "version": "1.2.3"}, "..."],
  "subvolumes": ["/", "/home", "/var"]
}
```

- **VerificationResult**

```json
{
  "is_valid": true,
  "errors": [],
  "warnings": ["@home subvolume missing metadata (legacy snapshot)"]
}
```

- **RestorePreview**

```json
{
  "snapshot_name": "pre-upgrade",
  "snapshot_timestamp": "2025-11-01T12:00:00Z",
  "snapshot_description": "Before xbps-install",
  "current_kernel": "6.8.7_1",
  "snapshot_kernel": "6.7.10_1",
  "affected_subvolumes": ["/", "/home"],
  "packages_to_add": [{"name": "foo", "current_version": null, "snapshot_version": "1.0", "change_type": "add"}],
  "packages_to_remove": [],
  "packages_to_upgrade": [],
  "packages_to_downgrade": [],
  "total_package_changes": 1
}
```

- **QuotaUsage** (from `waypoint-common/src/quota.rs`)

```json
{
  "referenced": 2147483648,
  "exclusive": 536870912,
  "limit": 8589934592
}
```

- **BackupDestination** (from `waypoint-helper/src/backup.rs`)

```json
{
  "mount_point": "/media/usb",
  "label": "usb-disk",
  "drive_type": "removable",
  "uuid": "1234-ABCD"
}
```

Other methods that return JSON (e.g., `ListBackups`, `CompareSnapshots`) serialize either arrays of strings or method-specific structures; refer to the helper sources if you need the exact schema.

## Calling Examples

Use `busctl` or any D-Bus binding that can talk to the system bus. Examples:

```sh
# Create a snapshot that includes / and /home (authorizes via Polkit)
busctl call --system tech.geektoshi.waypoint /tech/geektoshi/waypoint tech.geektoshi.waypoint.Helper \
  CreateSnapshot ssaas "pre-upgrade" "Before xbps-install" 2 "/" "/home"

# Fetch snapshot metadata
busctl call --system tech.geektoshi.waypoint /tech/geektoshi/waypoint tech.geektoshi.waypoint.Helper \
  ListSnapshots
```

The helper follows standard D-Bus introspection, so `busctl introspect tech.geektoshi.waypoint /tech/geektoshi/waypoint` always reflects the live method list.
