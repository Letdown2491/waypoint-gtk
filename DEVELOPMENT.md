# Development Guide

## Quick Start

### Prerequisites

Install dependencies on Void Linux:

```bash
sudo xbps-install -S base-devel rust cargo pkg-config gtk4-devel libadwaita-devel
```

### Building

```bash
# Debug build (faster compilation)
cargo build

# Release build (optimized)
cargo build --release
```

### Running

```bash
# Run debug version
cargo run

# Run release version
./target/release/waypoint
```

### Testing Snapshots

**⚠️ WARNING**: Snapshot creation requires:
1. Root filesystem on Btrfs
2. Root privileges (via sudo/pkexec)
3. Sufficient disk space

To test on a Btrfs system:

```bash
# Check if your root is on Btrfs
findmnt -n -o FSTYPE /

# Run with appropriate privileges
sudo cargo run
```

### Project Structure

```
src/
├── main.rs           # Application entry point, GTK setup
├── btrfs.rs          # Btrfs operations (snapshot, delete, list)
├── snapshot.rs       # Snapshot metadata and persistence
└── ui/
    ├── mod.rs        # Main window and application logic
    └── snapshot_row.rs # Custom widget for snapshot list items
```

## Current Features (Phase 1 & 2)

- [x] Btrfs filesystem detection
- [x] Read-only snapshot creation
- [x] Snapshot deletion with confirmation
- [x] Browse snapshots in file manager
- [x] Snapshot metadata tracking (timestamp, kernel, size)
- [x] JSON-based persistence of snapshot metadata
- [x] GTK4 + libadwaita UI with modern dialogs
- [x] Snapshot list view with placeholder state
- [x] Disk space warnings (1GB minimum)
- [x] Error handling with native dialogs
- [ ] Polkit integration (planned for Phase 3)
- [ ] Snapshot rollback (planned for Phase 3)

## Development Notes

### Implementation Notes

1. **Dialogs**: Using libadwaita::MessageDialog for all user interactions
   - Confirmation dialogs for destructive actions (delete)
   - Error dialogs for failures
   - Info dialogs for "coming soon" features

2. **Privilege Escalation**: Currently requires running with sudo
   - Polkit integration is stubbed but not fully implemented
   - Will be completed in Phase 3

3. **Rollback**: Shows manual instructions, automatic rollback coming in Phase 3
   - Needs careful testing to avoid breaking systems
   - Will implement btrfs subvolume set-default approach

### Code Quality

```bash
# Check code
cargo check

# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy
```

### Adding New Features

1. **New Btrfs Operations**: Add to `src/btrfs.rs`
2. **UI Components**: Add to `src/ui/` directory
3. **Snapshot Metadata**: Extend `Snapshot` struct in `src/snapshot.rs`

### Debugging

Enable GTK debug output:

```bash
GTK_DEBUG=interactive cargo run
```

Enable Rust backtrace:

```bash
RUST_BACKTRACE=1 cargo run
```

## Testing on Non-Btrfs Systems

If your system doesn't use Btrfs, the app will:
- Show a warning banner
- Disable snapshot creation
- Allow you to explore the UI

You can test the UI without Btrfs by:
1. Running the app normally (it won't crash)
2. The "Create Restore Point" button will show an error when clicked

## Common Issues

### "Permission Denied" when creating snapshot

**Solution**: Run with elevated privileges:
```bash
sudo ./target/release/waypoint
```

### GTK/libadwaita not found

**Solution**: Install development packages:
```bash
sudo xbps-install -S gtk4-devel libadwaita-devel
```

### Compilation errors about GTK versions

**Solution**: Update your GTK packages:
```bash
sudo xbps-install -Su
```

## Roadmap

See [README.md](README.md) for the full roadmap.

**Phase 2 Completed! ✅**
- ✅ Snapshot deletion with confirmation
- ✅ Browse snapshots
- ✅ Disk space warnings
- ✅ Proper error/confirmation dialogs

**Next priorities (Phase 3):**
1. Automatic snapshot rollback functionality
2. Complete polkit integration (seamless privilege escalation)
3. Package state tracking with xbps
4. Diff views (what changed between snapshots)
