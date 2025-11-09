# Changelog

All notable changes to Waypoint will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-11-09

### Core Snapshot Management

- **One-Click System Rollback**: Full system restore with automatic backup creation
- **Rollback Preview**: Preview changes before restoring a snapshot
  - Shows package changes: added, removed, upgraded, downgraded
  - Displays kernel version comparison
  - Lists affected subvolumes
  - Prevents blind rollbacks with detailed diff preview
- **Snapshot Integrity Verification**: Verify snapshot health with btrfs subvolume checks
  - New "Verify" button on snapshot cards
  - Checks each subvolume exists and is valid
  - Reports errors and warnings in detail dialog
  - Integrated D-Bus method for privileged verification
- **Multi-Subvolume Support**: Snapshot multiple Btrfs subvolumes atomically
  - Support for root (/), /home, /var, and other subvolumes
  - Preferences dialog to select which subvolumes to snapshot
  - Atomic operations across all selected subvolumes
  - Automatic subvolume detection
- **Package Tracking**: Full package state management
  - Automatically captures installed package list with each snapshot
  - Package count display on snapshot cards
  - Integration with XBPS package manager
- **Package Diff Viewer**: Visual comparison between snapshots
  - Side-by-side snapshot selection
  - Added, removed, and updated package lists
  - Version change tracking for updated packages
  - Integrated into main toolbar
- **Browse Snapshots**: Open snapshot directories in file manager
  - Uses xdg-open to launch default file manager
  - Works with Nautilus, Thunar, Dolphin, etc.
- **Snapshot Search & Filter**: Enhanced snapshot discovery
  - Real-time text search across names and descriptions
  - Date range filters (7/30/90 days, or all)
  - Match count display showing filtered vs total snapshots
  - Instant UI updates as you type

### Automation & Integration

- **Command-Line Interface**: `waypoint-cli` for scriptable snapshot management
  - List, create, delete, and restore snapshots from the command line
  - JSON output for integration with other tools
  - Full D-Bus integration with Polkit authentication
- **Scheduled Snapshots**: Automatic periodic snapshot creation
  - Runit service (`waypoint-scheduler`) for automated snapshots
  - Configurable schedules: hourly, daily, weekly, or custom
  - GUI configuration dialog with service status monitoring
  - Live service restart after configuration changes
  - Comprehensive logging to `/var/log/waypoint-scheduler/`
- **Scheduler Quick Presets**: One-click schedule configuration
  - "Daily at 2 AM" preset for convenient overnight snapshots
  - "Daily at Midnight" preset
  - "Weekly on Sunday" preset
  - Presets auto-populate frequency, hour, minute, and day fields
- **Live Schedule Preview**: See next snapshot time as you configure
  - Real-time calculation of next snapshot timestamp
  - Human-friendly format: "Next snapshot: Tomorrow at 02:00 (in 5 hours)"
  - Shows minutes when next snapshot is less than 1 hour away
  - Updates instantly as schedule parameters change
- **Retention Policies**: Automatic snapshot cleanup
  - Visual GUI for configuring retention policies
  - Edit max snapshots, max age, and minimum count
  - Configure keep patterns for pinned snapshots
  - Real-time preview of what would be cleaned up
  - Configurable via JSON file

### User Interface

- **GTK4 + libadwaita**: Modern, clean interface following GNOME HIG
  - Main window with header bar
  - Snapshot list with boxed-list styling
  - Empty state placeholder
  - Status banner for Btrfs detection
- **Hamburger Menu**: Clean menu interface in header bar
  - Theme switcher with system/light/dark mode options
  - Retention Policy editor for automatic cleanup rules
  - Snapshot Schedule for automated snapshot configuration
  - Snapshot Preferences for subvolume configuration
  - About Waypoint dialog with project links and version info
- **Application Icons**: Custom Waypoint icons in multiple sizes
  - PNG icons (128x128, 256x256, 512x512)
  - SVG icon for scalable display
  - Icon displayed in header bar and application launcher
- **Real-Time Disk Space Monitoring**: Footer shows available disk space
  - Color-coded warnings: green (>20% free), yellow (10-20%), red (<10%)
  - Automatic updates every 30 seconds
  - Updates immediately after snapshot creation/deletion
  - Cached queries for performance (30-second TTL)
- **Auto-Refresh**: UI automatically refreshes every 30 seconds
  - External snapshots (from scheduler/CLI) appear automatically
  - No manual tab switching needed to see new snapshots
  - Keeps UI synchronized with filesystem state

### Configuration & Flexibility

- **Centralized Configuration**: Unified config system with environment variable support
  - `WAYPOINT_SNAPSHOT_DIR`: Override default snapshot directory
  - `WAYPOINT_METADATA_FILE`: Custom metadata file location
  - `WAYPOINT_SCHEDULER_CONFIG`: Custom scheduler config path
  - `WAYPOINT_SERVICE_DIR`: Custom service directory
  - `WAYPOINT_MIN_FREE_SPACE`: Configurable minimum free space threshold
  - Eliminates hardcoded paths for better flexibility
- **Setup Script**: Preserves configuration on upgrades
  - No longer overwrites /etc/waypoint/scheduler.conf on reinstall
  - Installs scheduler.conf.example for reference when config exists
  - Follows standard package manager behavior for config files
  - Automatic dependency checking and installation
  - Single-command installation and uninstallation
  - Better error handling and user feedback

### Security

- **Privilege-Separated Architecture**: D-Bus system service for security
  - `waypoint-helper`: Privileged D-Bus service running as root
  - `waypoint`: User-space GUI communicating via D-Bus
  - Polkit integration for secure authentication
  - Automatic D-Bus activation on demand
  - All privileged operations require explicit user authorization
  - GUI runs as user, operations run as privileged helper
  - Strict D-Bus access control
- **Input Validation**: Comprehensive snapshot name validation
  - Prevents empty or too-long names (>255 chars)
  - Blocks invalid characters (/, .., etc.)
  - Rejects names starting with '-' or '.' to avoid command injection
  - Centralized validation in waypoint-common library
  - Applied consistently across CLI, GUI, and D-Bus interfaces
- **Path Validation**: Prevents directory traversal in file browser
  - Validates paths before passing to xdg-open
  - Restricts browsing to allowed snapshot directories
  - Blocks symlink attacks and path traversal attempts
- **Automatic fstab Backup**: Safety net before system modifications
  - Creates `/etc/fstab.bak` before first modification
  - Subsequent backups timestamped: `/etc/fstab.bak.YYYYMMDD-HHMMSS`
  - Prevents data loss from rollback operations
  - Reduces risk of unbootable system
- **Polkit Automation**: Secure privilege elevation for scheduler
  - Root can bypass authentication for automated operations
  - Regular users still require password for GUI operations
  - Enables background snapshot creation without compromising security
  - Rule installed via setup.sh to /etc/polkit-1/rules.d/50-waypoint-automated.rules
- **Disk Space Warnings**: Check available space before snapshot creation
  - Requires minimum 1GB free space
  - Shows clear error dialog if insufficient space

### Performance

- **Filesystem Query Caching**: TTL-based cache for expensive operations
  - 5-minute cache for snapshot size calculations
  - 30-second cache for available disk space
  - Thread-safe with automatic expiration cleanup
  - Eliminates redundant `du` and `df` calls
- **Memory Optimization**: Rc<T> for large data structures
  - Uses reference counting for packages and subvolumes lists
  - Reduces per-clone overhead from ~500KB to ~40 bytes (12,500x improvement)
  - Maintains full serde compatibility with custom serialization
  - Significantly improves performance with large package lists
- **UI Threading**: All blocking operations moved to background threads
  - Snapshot size calculations no longer freeze UI
  - Disk space queries run asynchronously
  - GTK idle polling for result handling
  - Maintains responsive interface during expensive operations

### Error Handling

- **Informative Error Dialogs**: Replaced ungraceful process::exit
  - Shows helpful troubleshooting steps
  - Guides user to check Btrfs availability and /.snapshots directory
  - Prevents unexpected application termination
  - libadwaita::MessageDialog for all user interactions
  - Confirmation dialogs for destructive actions
  - Error dialogs with detailed messages

### Architecture

- **Modular Rust Codebase**:
  - `btrfs.rs`: Low-level Btrfs operations
  - `snapshot.rs`: Metadata management with automatic phantom cleanup
  - `ui/`: GTK interface components
  - Extracted toolbar creation to separate module
  - Unified snapshot list refresh logic
  - Reduced code duplication across refresh functions
- **Namespace**: Professional namespace for wider distribution
  - Uses `tech.geektoshi.waypoint` namespace
  - All D-Bus service files, Polkit policies, and desktop entries updated
- **Metadata Persistence**: JSON-based storage of snapshot information
  - Centralized at `/var/lib/waypoint/snapshots.json`
  - Automatic cleanup of phantom snapshots (entries without filesystem presence)
  - Automatic deduplication
  - Multiple subvolumes per snapshot
  - Package lists stored with metadata
  - Disk size calculation and caching
  - Kernel version tracking

### Project Infrastructure

- Comprehensive `setup.sh` installation script
- Desktop entry file
- Polkit policy files
- D-Bus service configuration
- MIT license
- Comprehensive README and documentation
