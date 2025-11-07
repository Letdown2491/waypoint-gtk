# Waypoint

A GTK-based snapshot and rollback tool for Void Linux.

## Overview

Waypoint provides a simple, user-friendly interface for creating filesystem snapshots (restore points) and managing them. It's designed specifically for Void Linux with Btrfs filesystems, making it easy to:

- Create restore points before system upgrades
- Browse and manage existing snapshots
- Roll back to previous system states if something breaks

## Features

### ✅ Implemented (Phase 1 & 2)

- **Btrfs Snapshot Creation**: Create read-only snapshots of your root filesystem
- **Snapshot Deletion**: Remove unwanted snapshots with confirmation dialogs
- **Browse Snapshots**: Open snapshot directories in your file manager
- **Clean GTK4/libadwaita UI**: Modern interface that fits seamlessly with GNOME and other GTK-based desktops
- **Metadata Tracking**: Automatically records kernel version, timestamp, and snapshot size
- **Safety Checks**:
  - Verifies Btrfs support before operations
  - Checks available disk space (requires 1GB minimum)
  - Requires root privileges for destructive operations
- **Confirmation Dialogs**: Native libadwaita dialogs for all destructive actions
- **Error Handling**: Clear error messages for all failure scenarios

## Requirements

### System Requirements

- Void Linux with a Btrfs root filesystem
- GTK4 (>= 4.10)
- libadwaita (>= 1.4)
- polkit (for privilege escalation)

### Build Requirements

- Rust (>= 1.70)
- cargo
- Development packages:
  ```bash
  sudo xbps-install -S base-devel pkg-config gtk4-devel libadwaita-devel
  ```

## Building

```bash
# Clone the repository
git clone <repository-url>
cd waypoint-gtk

# Build the project
cargo build --release

# The binary will be at: target/release/waypoint
```

## Installation

```bash
# Build the project
cargo build --release

# Install binary
sudo install -Dm755 target/release/waypoint /usr/bin/waypoint

# Install desktop entry
sudo install -Dm644 data/com.voidlinux.waypoint.desktop /usr/share/applications/com.voidlinux.waypoint.desktop

# Install polkit policy
sudo install -Dm644 data/com.voidlinux.waypoint.policy /usr/share/polkit-1/actions/com.voidlinux.waypoint.policy
```

## Usage

### Running the Application

Launch Waypoint from your application menu or run:

```bash
waypoint
```

### Creating a Restore Point

1. Click the **"Create Restore Point"** button
2. You'll be prompted for your password (via polkit)
3. Wait for the snapshot to complete
4. The new restore point will appear in the list

**Note**: Creating snapshots requires root privileges and a Btrfs filesystem.

### Managing Snapshots

Each snapshot in the list shows:
- Snapshot name and creation time
- Kernel version at time of snapshot
- Storage size used

Available actions:
- **Browse** 📁: Opens the snapshot directory in your file manager (e.g., Thunar, Nautilus, Dolphin)
- **Restore** 🔄: Roll back to this snapshot (coming in Phase 3 - shows instructions for manual restore)
- **Delete** 🗑️: Remove the snapshot to free up space
  - Requires root privileges
  - Shows confirmation dialog before deletion
  - Permanently removes the Btrfs subvolume

## Architecture

Waypoint is built with:

- **Rust**: Core logic and safety
- **GTK4**: Modern UI framework
- **libadwaita**: GNOME-style widgets and design patterns
- **Btrfs**: Efficient copy-on-write snapshots

### Project Structure

```
waypoint-gtk/
├── src/
│   ├── main.rs           # Application entry point
│   ├── btrfs.rs          # Btrfs operations (snapshot, list, delete)
│   ├── snapshot.rs       # Snapshot metadata management
│   └── ui/
│       ├── mod.rs        # Main window and UI logic
│       └── snapshot_row.rs # Custom snapshot list widget
├── data/
│   ├── com.voidlinux.waypoint.desktop  # Desktop entry
│   └── com.voidlinux.waypoint.policy   # Polkit policy
└── Cargo.toml            # Project dependencies
```

## Roadmap

### Phase 1: MVP ✅ Completed
- [x] Basic Btrfs snapshot creation
- [x] Snapshot listing with metadata
- [x] GTK4 + libadwaita UI
- [x] Basic safety checks

### Phase 2: Core Features ✅ Completed
- [x] Delete snapshot support with confirmation dialogs
- [x] Browse snapshot contents in file manager
- [x] Disk space warnings (1GB minimum)
- [x] Proper error/confirmation dialogs with libadwaita::MessageDialog
- [x] Action callbacks for snapshot operations

### Phase 3: Enhanced Functionality
- [ ] Package state tracking (xbps integration)
- [ ] Diff views (packages and files)
- [ ] Pre-upgrade hook integration
- [ ] Scheduled automatic snapshots

### Phase 4: Advanced Features
- [ ] Non-Btrfs fallback (rsync-based)
- [ ] GRUB integration for boot-time recovery
- [ ] Snapshot export/import
- [ ] Multi-subvolume support (/home snapshots)

## Known Limitations

- **Btrfs Only**: Currently only supports Btrfs filesystems. Non-Btrfs fallback planned for Phase 4.
- **Root Required**: Snapshot creation and deletion require root privileges (run with sudo).
- **Read-Only Snapshots**: Snapshots are created as read-only for safety.
- **No Automatic Rollback**: Restore functionality shows manual instructions. Automated rollback coming in Phase 3.
- **Basic Polkit**: Polkit policy file exists but full integration (seamless privilege escalation) is not yet implemented.

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

GPL-3.0-or-later

## Credits

Developed for Void Linux users who want a simple, reliable way to manage system snapshots.
