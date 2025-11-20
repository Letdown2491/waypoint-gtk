# Waypoint Architecture

This document explains how the Waypoint client, helper daemon, scheduler, and supporting crates fit together, plus where Waypoint stores data on disk. Read this alongside [API.md](API.md) if you need the D-Bus interface details.

## Component Overview

| Component | Path | Responsibility |
| --- | --- | --- |
| GTK/libadwaita app | `waypoint/` | Presents the UI, drives user workflows, shows diffs, configures schedules/quotas. Talks to the helper through `WaypointHelperClient` (D-Bus). Includes `BackupManager` for automatic backup orchestration and queue management. |
| CLI | `waypoint-cli` | Comprehensive Bash-based CLI wrapper around D-Bus calls, supporting all snapshot operations, backup management, file restore, quota control, and more. Used by both users and the scheduler. |
| Privileged helper | `waypoint-helper/` | Owns the `tech.geektoshi.waypoint` D-Bus name, performs all Btrfs/snapshot/backup operations as root, emits the `SnapshotCreated` and `BackupProgress` signals, enforces Polkit. |
| Scheduler | `waypoint-scheduler/` | Rust runit service that reads `schedules.toml` and invokes `waypoint-cli` to create snapshots + cleanup on timers. |
| Shared crate | `waypoint-common/` | Provides config paths, snapshot/backup/quota types, validation helpers, and serialization used by both helper and clients. |
| Services & policy | `data/dbus-1/`, `data/tech.geektoshi.waypoint.policy`, `services/waypoint-scheduler/`, `system/polkit/` | Describe how the helper is activated on the system bus, which Polkit actions gate each method, and how runit starts the scheduler/log service. |

```
GUI / CLI / Scheduler
        │
        ▼
   tech.geektoshi.waypoint (D-Bus, system bus)
        │
        ▼
 waypoint-helper (root)
        │
        ├─ Btrfs subvolume ops
        ├─ Metadata (snapshots.json)
        ├─ Backups & quotas
        └─ Signals, notifications
```

## Snapshot Lifecycle

1. **Trigger**: The GTK app, CLI, or scheduler asks `WaypointHelperClient`/`waypoint-cli` to call `CreateSnapshot` (or other methods) on the helper.
2. **Authorization**: The helper inspects the caller’s PID via `org.freedesktop.DBus.GetConnectionUnixProcessID`, builds a Polkit subject, and calls `org.freedesktop.PolicyKit1.Authority.CheckAuthorization`. Polkit uses the action IDs defined in `waypoint-common/src/lib.rs`.
3. **Filesystem work**: `waypoint-helper/src/btrfs.rs` snapshots each configured subvolume (default `/`), applies exclude patterns from `ExcludeConfig`, sets them read-only, and records metadata.
4. **Metadata bookkeeping**: Each snapshot has an entry inside `/var/lib/waypoint/snapshots.json` (path taken from `WaypointConfig`). The entry stores display name, timestamp, description, kernel version, enabled subvolumes, and a package list captured via `xbps-query`.
5. **Signals + UI refresh**: On success the helper emits `SnapshotCreated`, which the GTK app listens for to refresh state. CLI users just receive the `(bool, string)` result.

Other operations (restore, verify, retention, quota, backups) follow the same handshake: userland component calls the helper via D-Bus, helper authorizes then executes the privileged work.

## On-Disk Layout

Waypoint keeps data in a few standard locations; all paths are configurable through `WaypointConfig` or environment variables (`WAYPOINT_*`):

| Path | Contents |
| --- | --- |
| `/.snapshots/<name>/root` (and siblings) | Actual Btrfs snapshots for each configured mount point. Subvolume names map `"/"` → `root`, `"/home"` → `home`, `"/var/log"` → `var_log`, etc. |
| `/.snapshots/<name>/root-writable` | Temporary writable copy created during multi-subvolume restores to modify fstab. Automatically cleaned up after restore completes. |
| `/var/lib/waypoint/snapshots.json` | Array of snapshot metadata as defined by `waypoint-helper::btrfs::Snapshot`. This drives the UI list, favorites, package diffs, etc. |
| `/etc/waypoint/schedules.toml` | Structured definition of runit schedules, prefixes, retention knobs (`waypoint-common::schedules`). |
| `/etc/waypoint/quota.toml` | Serialized `QuotaConfig`, consumed by D-Bus `GetQuotaUsage`, `SaveQuotaConfig`, etc. |
| `/etc/waypoint/exclude.toml` | Snapshot exclusion patterns. Defines which files/directories to exclude from snapshots (e.g., caches, temporary files). |
| `~/.config/waypoint/backup-config.toml` | Per-user backup destinations, filters, pending backups, and backup history. Managed by `BackupManager` in the GUI. |
| `~/.local/share/waypoint/user-preferences.json` | Per-user snapshot preferences (favorites, notes). |
| `/var/log/waypoint-scheduler/` | Managed by `svlogd` through `services/waypoint-scheduler/log/run`. |
| `/etc/dbus-1/system.d/tech.geektoshi.waypoint.conf` + `/usr/share/dbus-1/system-services/tech.geektoshi.waypoint.service` | D-Bus policy + activation. Installed by `setup.sh install`. |
| `/usr/share/polkit-1/actions/tech.geektoshi.waypoint.policy` and `system/polkit/*.rules` | Desktop prompts + optional auto-approval rules. |

Snapshot names must pass `validate_snapshot_name` to avoid traversal; scheduling prefixes reuse the same validator so generated names remain safe on disk.

## Scheduler & Retention

- `waypoint-scheduler` runs under runit via `services/waypoint-scheduler/run`. It loads `schedules.toml`, calculates the next due job, and when time arrives it shells out to `waypoint-cli create ...`.
- After each run it executes `waypoint-cli cleanup --schedule-based`, which calls `CleanupSnapshots(true)` so each schedule’s retention policy is enforced server-side in the helper.
- Users can edit schedules through the GTK dialog (which writes TOML over D-Bus) or by hand; once saved they restart the service via the helper (`RestartScheduler`) and the runit unit reloads automatically.

## Quotas, Backups, and File Restore

The helper centralizes all privileged operations that need filesystem access:

- Quota management wraps `btrfs quota enable/disable`, `btrfs qgroup show`, and writes `quota.toml`.
- Backups live in `waypoint-helper/src/backup.rs`, using `btrfs send | btrfs receive` to copy snapshots into `<destination>/waypoint-backups`, plus metadata for USB/network detection.
- File-level restore, snapshot diffing, package previews, and verification logic sit in `btrfs.rs` and expose JSON payloads back to the GUI.

Keeping these in the helper keeps the GTK app completely unprivileged and makes it safe to expose the same capabilities over the CLI and scheduler.

### BackupManager

The GUI includes a `BackupManager` component (`waypoint/src/backup_manager.rs`) that orchestrates automatic backups:

- **Configuration**: Manages per-user backup destinations with filters (All, Favorites, LastN, Critical), retention policies, and trigger settings (on snapshot creation, on drive mount).
- **Queue Management**: Tracks pending backups when destinations are unmounted. When drives reconnect, pending backups are automatically processed in chronological order (oldest first) to maintain proper parent relationships for incremental backups.
- **Live Progress**: Subscribes to `BackupProgress` D-Bus signals and displays real-time transfer status in the UI.
- **Status Monitoring**: Polls mounted destinations, counts pending/failed backups, and displays a footer status summary (healthy, pending, failed, disconnected).
- **History Tracking**: Records completed backups with timestamps, sizes, parent snapshots, and verification status.

The BackupManager bridges the gap between the privileged helper (which performs actual `btrfs send|receive` operations) and the unprivileged GUI (which manages user preferences and workflow).

## CLI & External Integration

- `waypoint-cli` is a comprehensive Bash-based command-line interface that wraps all D-Bus methods exposed by the helper. It supports snapshot operations (create, list, delete, restore, cleanup, verify), backup management (backup, list-backups, verify-backup, restore-backup, scan-destinations), file restore, quota control, and more. It validates inputs, formats output (with optional JSON), and provides helpful error messages. Used by both end-users for scripting and by `waypoint-scheduler` for automated operations.
- `waypoint-scheduler` shells out to `waypoint-cli` and inherits its Polkit prompts if run interactively; when managed by runit on Void the provided Polkit rules allow password-less operation for the runit user.
- Other Void tools (Nebula, future importers) use the same `tech.geektoshi.waypoint` D-Bus API documented in [API.md](API.md). The helper emits `SnapshotCreated` and `BackupProgress` signals so they can react to events without polling.

## Extending the Architecture

When adding new features:

1. Add data types/paths to `waypoint-common` so both helper and GUI agree on serialization.
2. Implement the privileged logic in `waypoint-helper` and expose it via D-Bus. Gate it behind an existing or new Polkit action.
3. Update `API.md` and, if the data is persisted, describe the storage here so other tools understand the layout.

Following that flow keeps the GTK/CLI side thin, ensures every operation funnels through the audited helper, and makes it easier for other Void utilities to integrate.
