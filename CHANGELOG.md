# Changelog

All notable changes to Waypoint will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Snapshot integrity verification**: Verify snapshot health with btrfs subvolume checks
  - New "Verify" button on snapshot cards
  - Checks each subvolume exists and is valid
  - Reports errors and warnings in detail dialog
  - Integrated D-Bus method for privileged verification
- **Rollback preview**: Preview changes before restoring a snapshot
  - Shows package changes: added, removed, upgraded, downgraded
  - Displays kernel version comparison
  - Lists affected subvolumes
  - Prevents blind rollbacks with detailed diff preview
- **Real-time disk space monitoring**: Header bar shows available disk space
  - Color-coded warnings: green (>20% free), yellow (10-20%), red (<10%)
  - Automatic updates every 30 seconds
  - Cached queries for performance (30-second TTL)
- **Scheduler quick presets**: One-click schedule configuration
  - "Daily at 2 AM" preset for convenient overnight snapshots
  - "Daily at Midnight" preset
  - "Weekly on Sunday" preset
  - Presets auto-populate frequency, hour, minute, and day fields
- **Live schedule preview**: See next snapshot time as you configure
  - Real-time calculation of next snapshot timestamp
  - Human-friendly format: "Next snapshot: Tomorrow at 02:00 (in 5 hours)"
  - Updates instantly as schedule parameters change
- **Centralized configuration**: Unified config system with environment variable support
  - `WAYPOINT_SNAPSHOT_DIR`: Override default snapshot directory
  - `WAYPOINT_METADATA_FILE`: Custom metadata file location
  - `WAYPOINT_SCHEDULER_CONFIG`: Custom scheduler config path
  - `WAYPOINT_SERVICE_DIR`: Custom service directory
  - `WAYPOINT_MIN_FREE_SPACE`: Configurable minimum free space threshold
  - Eliminates hardcoded paths for better flexibility
- **Input validation**: Comprehensive snapshot name validation
  - Prevents empty or too-long names (>255 chars)
  - Blocks invalid characters (/, .., etc.)
  - Rejects names starting with '-' or '.' to avoid command injection
  - Centralized validation in waypoint-common library
- **Automatic fstab backup**: Safety net before system modifications
  - Creates `/etc/fstab.bak` before first modification
  - Subsequent backups timestamped: `/etc/fstab.bak.YYYYMMDD-HHMMSS`
  - Prevents data loss from rollback operations
- **Filesystem query caching**: TTL-based cache for expensive operations
  - 5-minute cache for snapshot size calculations
  - 30-second cache for available disk space
  - Thread-safe with automatic expiration cleanup
  - Eliminates redundant `du` and `df` calls

### Changed
- **Error handling**: Replaced ungraceful process::exit with informative error dialogs
  - Shows helpful troubleshooting steps
  - Guides user to check Btrfs availability and /.snapshots directory
  - Prevents unexpected application termination
- **UI threading**: All blocking operations moved to background threads
  - Snapshot size calculations no longer freeze UI
  - Disk space queries run asynchronously
  - GTK idle polling for result handling
  - Maintains responsive interface during expensive operations
- **Snapshot structure optimization**: Memory-efficient cloning with Rc<T>
  - Uses reference counting for packages and subvolumes lists
  - Reduces per-clone overhead from ~500KB to ~40 bytes (12,500x improvement)
  - Maintains full serde compatibility with custom serialization
  - Significantly improves performance with large package lists
- **Scheduler UI**: Enhanced with presets and live preview
  - Quick preset buttons for common schedules
  - Real-time next snapshot calculation
  - Improved user experience with immediate feedback

### Fixed
- **Disk space indicator**: Now updates immediately after snapshot creation/deletion
  - Previously only updated on 30-second timer
  - Footer now reflects actual disk usage without delay
- **Package comparison**: Fixed comparison showing no differences
  - GUI and helper now use unified metadata file location
  - Changed from user-local (~/.local/share) to centralized (/var/lib/waypoint)
  - Both components now read from same metadata source
- **Scheduler service**: Fixed automated snapshots not being created
  - Added Polkit rule allowing root to bypass authentication
  - Scheduler can now create snapshots without interactive prompts
  - Rule installed via setup.sh to /etc/polkit-1/rules.d/50-waypoint-automated.rules
- **Scheduler preview**: Improved time display for imminent snapshots
  - Shows minutes when next snapshot is less than 1 hour away
  - Previously showed "in 0 hours" which was confusing
  - Better user experience with more precise timing information
- **Snapshot list refresh**: UI now auto-refreshes every 30 seconds
  - External snapshots (from scheduler/CLI) now appear automatically
  - No longer requires manual tab switching to see new snapshots
  - Keeps UI synchronized with filesystem state

### Security
- **Input validation**: Prevents snapshot name injection attacks
  - Validates all user-provided snapshot names
  - Blocks potentially dangerous characters and patterns
  - Applied consistently across CLI, GUI, and D-Bus interfaces
- **Path validation**: Prevents directory traversal in file browser
  - Validates paths before passing to xdg-open
  - Restricts browsing to allowed snapshot directories (/.snapshots, /mnt/btrfs-root/@snapshots)
  - Blocks symlink attacks and path traversal attempts
- **fstab safety**: Automatic backup before modifications
  - Reduces risk of unbootable system from rollback
  - Timestamped backups preserve history
- **Polkit automation**: Secure privilege elevation for scheduler
  - Root can bypass authentication for automated operations
  - Regular users still require password for GUI operations
  - Enables background snapshot creation without compromising security

### Performance
- **Caching layer**: Reduces filesystem query overhead
  - TTL-based cache eliminates redundant du/df commands
  - Configurable expiration times per query type
  - Thread-safe implementation for concurrent access
- **Memory optimization**: Rc<T> for large data structures
  - Dramatically reduces cloning overhead for snapshot metadata
  - Especially beneficial with 500+ package snapshots
  - Zero-cost abstraction with transparent serde support

## [1.0.0] - 2025-01-08 (Stable Release)

### Added
- **Hamburger menu**: Clean menu interface in header bar with GNOME-style design
  - Theme switcher with system/light/dark mode options
  - Retention Policy editor for automatic cleanup rules
  - Snapshot Schedule for automated snapshot configuration
  - Snapshot Preferences for subvolume configuration
  - Snapshot Statistics for disk usage and metrics
  - About Waypoint dialog with project links and version info
- **Application icons**: Custom Waypoint icons in multiple sizes
  - PNG icons (128x128, 256x256, 512x512)
  - SVG icon for scalable display
  - Icon displayed in header bar and application launcher
  - Integrated installation via setup.sh

### Changed
- Updated to stable 1.0.0 version number
- Streamlined toolbar by moving settings to hamburger menu
- Improved UI organization with menu-based access to all features
- Enhanced Statistics dialog with focused disk usage information
- Retention Policy dialog now shows cleanup preview

### Fixed
- Application icon now displays correctly in header bar
- Desktop entry uses proper Waypoint icon
- Statistics dialog cleaned up (removed duplicate retention policy section)

## [0.4.0] - 2025-01-08 (Feature-Complete Release)

### Added
- **Scheduled snapshots**: Automatic periodic snapshot creation
  - Runit service (`waypoint-scheduler`) for automated snapshots
  - Configurable schedules: hourly, daily, weekly, or custom
  - GUI configuration dialog with service status monitoring
  - Live service restart after configuration changes
  - Comprehensive logging to `/var/log/waypoint-scheduler/`
- **Command-line interface**: `waypoint-cli` for scriptable snapshot management
  - List, create, delete, and restore snapshots from the command line
  - JSON output for integration with other tools
  - Full D-Bus integration with Polkit authentication
- **Retention policy editor**: Visual GUI for configuring retention policies
  - Edit max snapshots, max age, and minimum count
  - Configure keep patterns for pinned snapshots
  - Real-time preview of what would be cleaned up
  - Integrated into statistics dialog
- **Search and filter**: Enhanced snapshot discovery
  - Real-time text search across names and descriptions
  - Date range filters (7/30/90 days, or all)
  - Match count display showing filtered vs total snapshots
  - Instant UI updates as you type
- **Namespace migration**: Professional namespace for wider distribution
  - Changed from `com.voidlinux.waypoint` to `tech.geektoshi.waypoint`
  - Updated all D-Bus service files, Polkit policies, and desktop entries
  - Migration guide in README for existing installations

### Changed
- Improved statistics dialog with retention policy editor integration
- Enhanced async size calculation with better progress indicators
- Refactored UI modules for better maintainability
  - Extracted toolbar creation to separate module
  - Unified snapshot list refresh logic
  - Reduced code duplication across refresh functions
- Updated README with scheduler documentation and CLI examples
- Replaced Makefile with comprehensive `setup.sh` installation script
  - Automatic dependency checking and installation
  - Single-command installation and uninstallation
  - Better error handling and user feedback

### Fixed
- **Security**: Polkit policy now uses correct `auth_admin` action (was `auth_admin_keep`)
- **Retention policy**: Fixed deletion logic to respect `min_snapshots` correctly
- **UI responsiveness**: Size calculation no longer blocks the main thread
- **Progress indicators**: Spinner now shows during size calculation operations
- **Build warnings**: Cleaned up all unused imports and dead code
- **Test coverage**: Added comprehensive unit tests for validation functions

### Removed
- Deprecated synchronous size calculation function

## [0.3.0] - 2025-01-05 (Production-Ready Release)

### Added
- **Privilege-separated architecture**: D-Bus system service for security
  - `waypoint-helper`: Privileged D-Bus service running as root
  - `waypoint`: User-space GUI communicating via D-Bus
  - Polkit integration for secure authentication
  - Automatic D-Bus activation on demand
- **Multi-subvolume support**: Snapshot multiple Btrfs subvolumes atomically
  - Support for root (/), /home, /var, and other subvolumes
  - Preferences dialog to select which subvolumes to snapshot
  - Atomic operations across all selected subvolumes
  - Automatic subvolume detection
- **Package tracking**: Full package state management
  - Automatically captures installed package list with each snapshot
  - Package count display on snapshot cards
  - Integration with XBPS package manager
- **Package diff viewer**: Visual comparison between snapshots
  - Side-by-side snapshot selection
  - Added, removed, and updated package lists
  - Version change tracking for updated packages
  - Integrated into main toolbar
- **Retention policies**: Automatic snapshot cleanup
  - Configurable via JSON file (`~/.config/waypoint/retention.json`)
  - Max snapshots count limit
  - Max age in days
  - Minimum snapshots protection
  - Keep patterns for pinned snapshots
- **Statistics dashboard**: Comprehensive storage and usage metrics
  - Total snapshot count and disk usage
  - Oldest snapshot age tracking
  - Available disk space monitoring
  - Top 3 largest snapshots
  - Retention policy status
  - Calculate missing sizes tool with async progress
- **XBPS hooks**: Automatic snapshots before system upgrades
  - Pre-upgrade hook (`waypoint-pre-upgrade.sh`)
  - Configurable behavior via `/etc/waypoint/waypoint.conf`
  - Automatic snapshot naming with timestamps
  - Integration with `xbps-install`
- **Enhanced metadata**: Richer snapshot information
  - Multiple subvolumes per snapshot
  - Package lists stored with metadata
  - Disk size calculation and caching
  - Kernel version tracking

### Changed
- Complete architectural overhaul with D-Bus privilege separation
- Improved UI with libadwaita native widgets throughout
- Better error handling and user feedback
- Enhanced snapshot cards showing subvolume information
- Project structure reorganized for clarity

### Security
- **Polkit authentication**: All privileged operations require explicit user authorization
- **Privilege separation**: GUI runs as user, operations run as privileged helper
- **D-Bus policy**: Strict access control for system service
- **Input validation**: Sanitization of user inputs to prevent injection attacks

## [0.2.0] - 2025-01-02 (Phase 2 Complete)

### Added
- **Snapshot deletion**: Delete unwanted snapshots with confirmation dialog
  - Native libadwaita confirmation dialog before deletion
  - Removes both the Btrfs subvolume and metadata
  - Requires root privileges
- **Browse snapshots**: Open snapshot directories in file manager
  - Uses xdg-open to launch default file manager
  - Works with Nautilus, Thunar, Dolphin, etc.
- **Disk space warnings**: Check available space before snapshot creation
  - Requires minimum 1GB free space
  - Shows clear error dialog if insufficient space
- **Modern dialogs system**: Complete rewrite of error handling
  - libadwaita::MessageDialog for all user interactions
  - Confirmation dialogs for destructive actions
  - Error dialogs with detailed messages
  - Info dialogs for coming-soon features
- **Action callbacks**: Proper event handling for snapshot operations
  - Browse, Restore, and Delete buttons now functional
  - Clean callback architecture for UI updates

### Changed
- Improved UI responsiveness with proper GTK callbacks
- Better error messages throughout the application
- Refactored dialog system for consistency

### Fixed
- Unused warnings cleaned up
- Proper trait imports for libadwaita components
- Toast notification system (simplified for now)

## [0.1.0] - 2025-01-XX (MVP/Phase 1)

### Added
- **Btrfs snapshot creation**: Create read-only snapshots of root filesystem
- **Snapshot listing**: View all created snapshots with metadata
  - Timestamp
  - Kernel version at time of creation
  - Storage size
- **Metadata persistence**: JSON-based storage of snapshot information
- **GTK4 + libadwaita UI**: Modern, clean interface
  - Main window with header bar
  - Snapshot list with boxed-list styling
  - Empty state placeholder
  - Status banner for Btrfs detection
- **Safety checks**:
  - Btrfs filesystem detection
  - Root privilege verification
  - Basic error handling
- **Project infrastructure**:
  - Rust project with proper dependencies
  - Makefile for building and installation
  - Desktop entry file
  - Polkit policy file (infrastructure)
  - MIT license
  - Comprehensive README and DEVELOPMENT guides

### Architecture
- Modular Rust codebase
  - `btrfs.rs`: Low-level Btrfs operations
  - `snapshot.rs`: Metadata management
  - `ui/`: GTK interface components
- Follows Rust best practices and safety guidelines
