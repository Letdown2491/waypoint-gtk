# Waypoint Features

Complete feature list for Waypoint snapshot and rollback tool.

## Snapshot Management

- One-click system rollback with automatic safety backup creation
- Rollback preview showing package changes, kernel version comparison, and affected subvolumes
- Snapshot integrity verification via D-Bus
- Multi-subvolume support (/, /home, /var, etc.) with atomic operations
- Individual file/directory restoration without full system rollback
- Snapshot comparison with file-level diff visualization
- Per-user snapshot notes and pinned favorites
- Search and filter with real-time text search and date ranges
- Browse snapshots in file manager via xdg-open

## Backup & Recovery

- Automatic backup to external drives with real-time progress tracking
- Incremental backups to Btrfs drives using btrfs send/receive
- Full backups to non-Btrfs drives (NTFS, exFAT, network shares) via rsync
- Automatic backup destination discovery and mount monitoring
- Backup verification with file count, size comparison, and SHA256 checksum validation
- Pending backup queue with automatic retry when destinations become available
- Restore snapshots from external backup
- Drive health statistics (space usage, backup count, timestamps)

## Package Management

- Automatic XBPS package state tracking on snapshot creation
- Package diff viewer with side-by-side comparison
- Package change categorization (added, removed, upgraded, downgraded)
- Version change tracking with visual indicators

## Scheduling & Automation

- Multiple concurrent snapshot schedules (hourly, daily, weekly, monthly)
- Per-schedule configuration with custom prefixes, descriptions, and subvolume selection
- Schedule-specific retention policies
- Runit service integration with live status monitoring
- Quick presets and live schedule preview
- Desktop notifications for scheduled snapshot creation

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
- Command-line interface for scripting and automation

## Security & Performance

- Privilege-separated architecture with D-Bus and Polkit integration
- Structured JSON audit logging for all security events
- Rate limiting to prevent DoS attacks
- Input validation preventing command injection and path traversal
- Filesystem query caching with TTL for performance
- Parallel computation for snapshot size calculations
- Background threading for all blocking operations
