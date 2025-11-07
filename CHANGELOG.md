# Changelog

All notable changes to Waypoint will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2025-01-XX (Phase 2 Complete)

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
