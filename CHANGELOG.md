# Changelog

All notable changes to Waypoint will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
