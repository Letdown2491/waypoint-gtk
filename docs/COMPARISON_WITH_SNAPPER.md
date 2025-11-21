# Waypoint vs Snapper: Feature Comparison

This document compares Waypoint with Snapper, the most popular Btrfs snapshot management tool used by openSUSE and other distributions.

## Overview

| Aspect | Waypoint | Snapper |
|--------|----------|---------|
| **Primary Distribution** | Void Linux | openSUSE, Arch, Fedora, Ubuntu |
| **Language** | Rust | C++ |
| **GUI Framework** | GTK4 + libadwaita (GNOME) | Qt5 (YaST integration on openSUSE) |
| **Architecture** | D-Bus system service + GUI client | Command-line tool + optional GUI |
| **Configuration** | TOML files | INI-style config files |
| **Package Manager Integration** | XBPS (Void Linux) | Zypper (openSUSE), Pacman (Arch), DNF (Fedora) |

## Core Snapshot Features

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Btrfs snapshot creation** | ✅ Yes | ✅ Yes |
| **Multi-subvolume snapshots** | ✅ Yes (/, /home, /var, etc.) | ✅ Yes (configurable configs) |
| **Atomic multi-subvolume operations** | ✅ Yes | ⚠️ Via separate configs |
| **Automatic pre/post snapshots** | ❌ Manual only | ✅ Yes (timeline + pre/post pairs) |
| **Snapshot descriptions** | ✅ Yes (user-editable) | ✅ Yes |
| **Snapshot metadata** | ✅ JSON (packages, kernel, subvolumes) | ✅ XML (userdata, cleanup algorithm) |
| **Read-only snapshots** | ✅ Always | ✅ Default (configurable) |
| **Writable snapshots** | ⚠️ Temporary during restore | ✅ Supported |

## User Interface

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Native GUI** | ✅ GTK4 + libadwaita (modern GNOME design) | ⚠️ Qt GUI exists but limited adoption |
| **CLI tool** | ✅ waypoint-cli (comprehensive) | ✅ snapper (primary interface) |
| **Desktop notifications** | ✅ Yes (snapshot creation, deletion, restore, backups) | ❌ No |
| **Toast notifications** | ✅ Yes (in-app feedback) | ❌ No |
| **Real-time UI updates** | ✅ Auto-refresh every 30s + D-Bus signals | ❌ Manual refresh |
| **Search and filtering** | ✅ Text search + date range filters | ⚠️ Limited (list filtering via CLI flags) |
| **Pin/favorite snapshots** | ✅ Yes (user preferences) | ❌ No |
| **Per-snapshot notes** | ✅ Yes (multi-line editor) | ⚠️ Via description field only |
| **Dark mode support** | ✅ Yes (system/light/dark) | ⚠️ Depends on Qt theme |

## Rollback & Restoration

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **One-click rollback** | ✅ Yes (with safety backup) | ✅ Yes |
| **Rollback preview** | ✅ Yes (packages, kernel, subvolumes, file count) | ✅ Yes (basic comparison) |
| **Package diff viewer** | ✅ Yes (side-by-side, categorized) | ✅ Via `snapper diff` |
| **File-level comparison** | ✅ Yes (directory-grouped, expandable) | ✅ Via `snapper diff` |
| **Automatic safety backup** | ✅ Yes (before rollback) | ⚠️ Via pre-rollback snapshot |
| **Individual file restore** | ✅ Yes (GUI + CLI, custom target path) | ⚠️ Via manual `cp` from snapshot |
| **Fstab validation** | ✅ Yes (automatic, prevents boot failures) | ❌ No |
| **Automatic cleanup of temp snapshots** | ✅ Yes (writable copies after restore) | ⚠️ Manual |

## Comparison & Diff

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **File comparison method** | `find` + metadata comparison | `btrfs send/receive` or `rsync` |
| **File diff visualization** | Directory-grouped, sorted by change count | File list (CLI), basic GUI diff |
| **Change categorization** | Added/Modified/Deleted with icons | Similar |
| **Directory grouping** | ✅ Smart multi-level (e.g., /usr/lib) | ❌ Flat list |
| **File count in summary** | ✅ Real-time calculation | ⚠️ Via `snapper status` |
| **Package comparison** | ✅ GUI table (added/removed/upgraded/downgraded) | ⚠️ CLI output |
| **Export comparison** | ✅ Text file export | ⚠️ CLI output redirection |
| **Authentication required** | ❌ No (read-only operation) | ⚠️ Depends on configuration |

## Scheduling & Automation

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Scheduled snapshots** | ✅ Runit service (multi-threaded) | ✅ systemd timers |
| **Schedule types** | Hourly, Daily, Weekly, Monthly | Timeline (configurable intervals) |
| **Multiple concurrent schedules** | ✅ Yes (parallel execution) | ✅ Yes (multiple configs) |
| **Per-schedule retention** | ✅ Yes (timeline-based buckets) | ✅ Yes (cleanup algorithms) |
| **GUI schedule editor** | ✅ Yes (live preview, presets) | ⚠️ YaST only (openSUSE) |
| **Service status monitoring** | ✅ Yes (GUI indicator) | ⚠️ systemctl only |
| **Live schedule preview** | ✅ Next snapshot time shown | ❌ No |
| **Root filesystem enforcement** | ✅ Always included in schedules | ⚠️ Per-config setting |

## Retention & Cleanup

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Retention policies** | Timeline-based (hourly/daily/weekly/monthly/yearly) | Number + timeline algorithms |
| **Global retention** | ✅ Yes (max snapshots, max age, min count) | ⚠️ Per-config |
| **Per-schedule retention** | ✅ Yes | ✅ Yes (per-config) |
| **Keep pinned snapshots** | ✅ Yes (excluded from cleanup) | ⚠️ Via important=yes userdata |
| **Dry-run mode** | ✅ Yes (preview before delete) | ✅ Yes (`--dry-run`) |
| **Real-time preview** | ✅ GUI shows what will be deleted | ❌ CLI output only |
| **Automatic cleanup** | ✅ After snapshot creation, scheduled | ✅ systemd timer |

## Backup & Recovery

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **External backup support** | ✅ Yes (Btrfs + non-Btrfs drives) | ⚠️ Manual `btrfs send` |
| **Incremental backups (Btrfs)** | ✅ Yes (automatic parent tracking) | ⚠️ Manual `btrfs send -p` |
| **Full backups (non-Btrfs)** | ✅ Yes (rsync to NTFS/exFAT/network) | ❌ No built-in support |
| **Automatic destination discovery** | ✅ Yes (mount monitoring) | ❌ No |
| **Backup filters** | ✅ All/Favorites/LastN/Critical | ❌ No |
| **Backup triggers** | ✅ On creation/on mount/manual | ❌ No |
| **Backup queue** | ✅ Pending backup management | ❌ No |
| **Real-time progress tracking** | ✅ D-Bus signals (bytes/speed/stage) | ❌ No |
| **Backup verification** | ✅ Automatic (file count, size, checksums) | ⚠️ Manual |
| **Restore from backup** | ✅ GUI + CLI with integrity checks | ⚠️ Manual `btrfs receive` |
| **Per-destination retention** | ✅ Age-based automatic cleanup | ❌ No |
| **Drive statistics** | ✅ Space usage, backup count, timestamps | ❌ No |
| **Failed backup tracking** | ✅ Retry mechanism | ❌ No |

## Quotas & Disk Management

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Btrfs quota management** | ✅ GUI + CLI (simple/traditional qgroups) | ⚠️ Via `btrfs quota` manually |
| **Quota usage monitoring** | ✅ Real-time (GUI preferences) | ❌ No |
| **Quota limit configuration** | ✅ Human-readable sizes | ⚠️ Manual |
| **Disk space warnings** | ✅ Before snapshot creation (1GB minimum) | ⚠️ Snapper monitors but less granular |
| **Space usage analytics** | ✅ Dashboard with trends and insights | ❌ No |
| **Largest snapshot detection** | ✅ Visual comparison | ⚠️ Via `btrfs qgroup show` |

## Snapshot Exclusions

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Exclude patterns** | ✅ Prefix/Suffix/Glob/Exact | ⚠️ Limited (via `ALLOW_USERS`, `ALLOW_GROUPS`) |
| **System defaults** | ✅ Yes (caches, temp files, build artifacts) | ⚠️ No comprehensive defaults |
| **User-customizable** | ✅ GUI preferences tab | ⚠️ Config file editing |
| **Pattern types** | ✅ 4 types (Prefix/Suffix/Glob/Exact) | ❌ Not directly supported |
| **Preview excluded files** | ⚠️ Not yet | ❌ No |

## Analytics & Insights

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Analytics dashboard** | ✅ Overview stats, trends, recommendations | ❌ No |
| **Space usage trends** | ✅ Visual timeline | ❌ No |
| **Growth analysis** | ✅ Actionable insights | ❌ No |
| **Size comparison** | ✅ Largest snapshots highlighted | ⚠️ Via CLI queries |
| **Retention recommendations** | ✅ Based on usage patterns | ❌ No |

## Security & Permissions

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Privilege separation** | ✅ D-Bus + Polkit | ⚠️ Root or configured users |
| **Fine-grained permissions** | ✅ 4 Polkit actions | ⚠️ ALLOW_USERS/ALLOW_GROUPS |
| **Desktop-friendly auth** | ✅ Passwordless for wheel group (optional) | ⚠️ Depends on system config |
| **Read-only operations** | ✅ No auth required | ⚠️ Depends on file permissions |
| **Audit logging** | ✅ Security-critical operations | ⚠️ System logs only |
| **Rate limiting** | ✅ DoS prevention (5s cooldown) | ❌ No |
| **Input validation** | ✅ Command injection prevention | ✅ Yes |
| **Path traversal protection** | ✅ Yes | ✅ Yes |
| **TOCTOU mitigation** | ✅ Inode verification | ⚠️ Standard filesystem semantics |
| **Symlink validation** | ✅ Explicit checks | ⚠️ Standard filesystem semantics |

## Performance & Optimization

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Caching** | ✅ TTL-based (5min snapshots, 30s disk space) | ⚠️ Limited |
| **Background threading** | ✅ All expensive operations | ⚠️ CLI is synchronous |
| **Parallel computation** | ✅ Rayon for bulk operations | ❌ No |
| **Bulk queries** | ✅ GetSnapshotSizes D-Bus method | ❌ No |
| **Memory optimization** | ✅ Rc<T> for large structures | ✅ C++ smart pointers |
| **UI responsiveness** | ✅ Non-blocking (background threads) | ⚠️ CLI is blocking |
| **Performance instrumentation** | ✅ Debug logging with min/max/avg/median | ❌ No built-in profiling |

## Integration & Extensibility

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **D-Bus API** | ✅ Complete system bus service | ❌ No |
| **Event signals** | ✅ SnapshotCreated, BackupProgress | ❌ No |
| **Package manager hooks** | ✅ Nebula integration | ✅ Zypper/Pacman/DNF plugins |
| **JSON output** | ✅ CLI + D-Bus | ✅ CLI --json |
| **Machine-readable output** | ✅ All operations | ✅ Most operations |
| **Third-party integration** | ✅ Via D-Bus API | ⚠️ Via CLI wrapping |
| **Custom scripts** | ✅ Via waypoint-cli | ✅ Via snapper CLI |

## Documentation

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **User guide** | ✅ Comprehensive | ⚠️ Man pages + wiki |
| **API documentation** | ✅ Full D-Bus API spec | ⚠️ CLI reference |
| **Architecture docs** | ✅ Detailed | ⚠️ Code comments |
| **Security documentation** | ✅ Comprehensive | ⚠️ Basic |
| **CLI reference** | ✅ Dedicated CLI.md | ✅ Man pages |
| **Troubleshooting guide** | ✅ Yes | ⚠️ Wiki/forums |

## Distribution & Installation

| Feature | Waypoint | Snapper |
|---------|----------|---------|
| **Package availability** | Void Linux (pending official packaging) | openSUSE, Arch, Fedora, Ubuntu, Debian |
| **Installation method** | setup.sh script + manual build | Package manager |
| **Official support** | Void Linux | openSUSE (primary), others (community) |
| **Init system** | Runit | systemd (primary), others possible |
| **Bootloader integration** | ⚠️ Manual fstab (planned GRUB integration) | ✅ GRUB integration (openSUSE) |

## Unique Waypoint Features

Features that Waypoint has that Snapper lacks:

1. **Modern GTK4 GUI** - Native GNOME desktop integration with libadwaita
2. **Automatic backup system** - Built-in backup to external drives with queue management
3. **Non-Btrfs backup support** - rsync to NTFS/exFAT/network shares
4. **Real-time backup progress** - Live transfer speed and progress tracking
5. **Analytics dashboard** - Space usage trends and actionable recommendations
6. **Directory-grouped file comparison** - Smart organization of file changes
7. **Desktop notifications** - System notifications for all major operations
8. **Pin/favorite snapshots** - User preferences with notes
9. **Live UI updates** - D-Bus signals for real-time state changes
10. **Backup verification** - Automatic integrity checks
11. **Failed backup tracking** - Retry mechanism for transient failures
12. **Drive statistics** - Health monitoring for backup destinations
13. **Fstab validation** - Prevents boot failures from malformed configuration
14. **Comprehensive D-Bus API** - Third-party tool integration
15. **Multi-threaded scheduler** - Parallel schedule execution

## Unique Snapper Features

Features that Snapper has that Waypoint lacks:

1. **Pre/post snapshot pairs** - Automatic before/after snapshots for package operations
2. **Package manager plugins** - Deep integration with Zypper, Pacman, DNF
3. **GRUB integration** - Boot into snapshots from bootloader menu (openSUSE)
4. **Mature ecosystem** - 10+ years of development and testing
5. **Wide distribution support** - Available on most major distributions
6. **YaST integration** - openSUSE's system configuration tool
7. **Writable snapshots** - First-class support for read-write snapshots
8. **Rollback from rescue** - Boot into snapshot without running system

## Use Case Recommendations

### Choose Waypoint if you:
- Run Void Linux (primary target)
- Prefer modern GNOME-style GUI applications
- Want built-in backup management with external drives
- Need automatic backup to non-Btrfs destinations (NTFS, network shares)
- Value real-time progress tracking and notifications
- Want analytics and insights about snapshot usage
- Prefer D-Bus integration for custom tools
- Like Rust and want to contribute to a modern codebase

### Choose Snapper if you:
- Run openSUSE, Arch, Fedora, or Ubuntu
- Need deep package manager integration (pre/post snapshots)
- Want GRUB bootloader snapshot selection
- Prefer mature, battle-tested software
- Need pre/post snapshot pairs for system updates
- Have existing Snapper workflows and scripts
- Want official distribution support

## Migration Considerations

### Snapper → Waypoint
- Snapshots are compatible (both use Btrfs snapshots)
- Metadata needs conversion (XML → JSON)
- Configs need manual recreation (INI → TOML)
- Schedule timings may differ
- Pre/post pairs not supported (manual workflow needed)

### Waypoint → Snapper
- Snapshots are compatible
- Metadata will be lost (Snapper ignores Waypoint JSON)
- Backup configuration lost (manual recreation needed)
- User preferences (notes, favorites) lost

## Conclusion

**Waypoint** is a modern, user-friendly snapshot manager designed for Void Linux with emphasis on GUI usability, built-in backups, and comprehensive automation. It excels at providing a polished desktop experience with real-time feedback, analytics, and external backup management.

**Snapper** is a mature, widely-adopted snapshot manager with deep package manager integration and broad distribution support. It excels at automated pre/post snapshot pairs for system updates and has proven reliability across multiple distributions.

Both are excellent tools for Btrfs snapshot management - the choice depends on your distribution, workflow preferences, and whether you value modern GUI features (Waypoint) or package manager integration and maturity (Snapper).
