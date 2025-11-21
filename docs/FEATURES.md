# Waypoint Features

Complete feature list for Waypoint snapshot and rollback tool.

## Snapshot Management

- One-click system rollback with automatic safety backup creation
- Rollback preview showing package changes, kernel version comparison, and affected subvolumes
- Snapshot integrity verification via D-Bus
- Multi-subvolume support (/, /home, /var, etc.) with atomic operations
- Automatic fstab validation during restore to prevent boot failures
- Safety checks for writable snapshot copies with automatic cleanup
- Clear error messages for missing or invalid restore requirements
- Individual file/directory restoration without full system rollback
- Snapshot comparison with file-level diff visualization organized by directory, showing change counts and grouped file lists
- Per-user snapshot notes and pinned favorites
- Search and filter with real-time text search and date ranges
- Browse snapshots in file manager via xdg-open
- Snapshot exclusion patterns to omit caches, temporary files, and custom paths
- System-default and user-defined exclusion rules with prefix/suffix/glob matching

## Backup & Recovery

- Automatic backup to external drives with real-time progress tracking
- Incremental backups to Btrfs drives using btrfs send/receive with parent tracking
- Full backups to non-Btrfs drives (NTFS, exFAT, network shares) via rsync
- Automatic backup destination discovery and mount monitoring
- Per-destination backup filters (All, Favorites, LastN days, Critical snapshots)
- Flexible backup triggers (on snapshot creation, on drive mount, manual)
- Scheduled snapshot backups triggered automatically when backup destinations are available
- Backup verification with file count and size comparison
- Automatic integrity verification for restored snapshots
- Pending backup queue with automatic retry when destinations become available
- Chronological backup processing (oldest first) to maintain parent relationships
- Restore snapshots from external backup
- Delete individual backups from external drives
- Age-based backup retention policies per destination
- Drive health statistics (space usage, backup count, timestamps)
- Real-time backup status footer showing healthy/pending/failed/disconnected states

## Package Management

- Automatic XBPS package state tracking on snapshot creation
- Package diff viewer with side-by-side comparison
- Package change categorization (added, removed, upgraded, downgraded)
- Version change tracking with visual indicators

## Scheduling & Automation

- Multiple concurrent snapshot schedules (hourly, daily, weekly, monthly)
- Per-schedule configuration with custom prefixes, descriptions, and subvolume selection
- Schedule-specific retention policies
- Root filesystem (/) always included in snapshots for complete system restore capability
- Runit service integration with live status monitoring
- Quick presets and live schedule preview
- Desktop notifications for scheduled snapshot creation
- Automatic backup integration for scheduled snapshots

## Retention & Cleanup

- Timeline-based retention with configurable hourly, daily, weekly, monthly, and yearly buckets
- Global retention policies with max snapshots, max age, and minimum count protection
- Per-schedule retention policies for fine-grained control
- Keep patterns for pinned snapshots
- Real-time preview of snapshots to be deleted
- Dry-run mode for cleanup operations

## Analytics & Insights

- Snapshot analytics dashboard with overview statistics
- Space usage trends and growth analysis
- Actionable insights and recommendations
- Visual size comparison of largest snapshots

## Quota Management

- Enable/disable Btrfs quotas (simple quotas or traditional qgroups)
- Quota limit configuration with human-readable sizes
- Real-time quota usage monitoring
- Automatic quota-based cleanup triggers
- Disk space warnings before snapshot creation

## User Interface

- Modern GTK4 + libadwaita interface following GNOME HIG
- Theme switcher (system/light/dark mode)
- Real-time disk space monitoring with color-coded warnings
- Toast notifications for in-app feedback
- Desktop notifications for all major operations
- Auto-refresh UI every 30 seconds
- Backup status footer with live monitoring and clickable configuration
- Comprehensive command-line interface (waypoint-cli) for scripting and automation
  - All snapshot operations (create, list, delete, restore, verify, compare)
  - Complete backup management (backup, list-backups, verify-backup, restore-backup)
  - File restore, quota control, and drive scanning
  - JSON output mode for machine-readable results
  - Full feature parity with GUI for all core operations

## Security & Performance

- Privilege-separated architecture with D-Bus and Polkit integration
- Desktop-friendly Polkit policy for passwordless snapshots and backups (wheel group users)
- Automated service Polkit policy for passwordless scheduler operations (root)
- Audit logging for security-critical operations
- Rate limiting with mutex poisoning detection to prevent DoS attacks
- TOCTOU mitigation via inode verification
- Input validation preventing command injection and path traversal
- Symlink validation for safe file restoration operations
- Automatic resource cleanup verification in error paths
- Filesystem query caching with TTL for performance
- Parallel computation for snapshot size calculations
- Background threading for all blocking operations

## Integration & External Tools

- Complete D-Bus API for third-party tool integration
- D-Bus signal emissions (SnapshotCreated, BackupProgress) for event-driven workflows
- Polkit-based authorization with fine-grained permission control
- Integration with Nebula package manager for automatic pre-upgrade snapshots
- Runit service integration for scheduled snapshots
- Environment variable configuration for custom paths and testing
- Machine-readable JSON output for scripting and monitoring
