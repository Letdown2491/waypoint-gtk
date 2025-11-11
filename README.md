# Waypoint

NOTE: APPLICATION STILL IN ALPHA TESTING. DO NOT USE IN PRODUCTION YET.

Waypoint is a GTK/libadwaita snapshot and rollback tool with a built in scheduling service for Btrfs filesystems on Void Linux.

For Void Linux users, Waypoint integration is available on [Nebula](https://github.com/Letdown2491/nebula-gtk) >= 1.3.0 to automatically create system snapshots when performing system upgrades.

## Screenshots

<p align="center">
  <img src="assets/screenshots/1-waypoint.png" alt="Waypoint UI" width="49%">
  <img src="assets/screenshots/2-waypointservice.png" alt="Waypoint Snapshot Scheduling" width="49%">
</p>

## Features

- One-click system rollback with automatic backup creation
- Rollback preview showing package changes before restoring
- Multi-subvolume support (/, /home, /var, etc.)
- Package state tracking with XBPS integration
- Scheduled snapshots via runit service
- Retention policies for automatic cleanup
- Desktop notifications for snapshot events
- Command-line interface for those that prefer it.

## Requirements

- Void Linux with Btrfs filesystem
- GTK 4.10+ and libadwaita 1.4+ runtimes
- Rust 1.70 or newer
- DBus and Polkit
- @snapshots subvolume mounted at `/.snapshots`

## Quick Start

```sh
cargo run --bin waypoint
```

## Production Build

```sh
cargo build --release
```

The optimized binaries are written to `target/release/`. Use `cargo run --release` to execute the release build directly after compiling.

## System Install

```sh
sudo ./setup.sh install
```

The helper script builds the release binaries, installs them into `/usr/bin`, registers the desktop entry, D-Bus service, and Polkit policies. Use `sudo ./setup.sh uninstall` to remove those assets.

## Command Line

The CLI tool provides scriptable snapshot management:

```sh
# List all snapshots
waypoint-cli list

# Create a snapshot
waypoint-cli create "snapshot-name" "Optional description"

# Restore a snapshot
waypoint-cli restore "snapshot-name"

# Delete a snapshot
waypoint-cli delete "snapshot-name"
```

## Scheduler Service

Enable automatic periodic snapshots:

```sh
# Enable the scheduler service
sudo ln -s /etc/sv/waypoint-scheduler /var/service/

# Configure via GUI or edit /etc/waypoint/scheduler.conf
```
