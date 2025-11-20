# Contributing to Waypoint

Thank you for your interest in contributing to Waypoint! This document provides guidelines for contributing to the project.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Code Style](#code-style)
- [Testing](#testing)
- [Submitting Changes](#submitting-changes)
- [Reporting Bugs](#reporting-bugs)
- [Feature Requests](#feature-requests)

---

## Getting Started

Waypoint is a GTK4/libadwaita snapshot and rollback tool for Void Linux with Btrfs filesystems. Before contributing:

1. Read the [User Guide](docs/USER_GUIDE.md) to understand how Waypoint works
2. Review [ARCHITECTURE.md](docs/ARCHITECTURE.md) to understand the codebase structure
3. Check [existing issues](https://github.com/Letdown2491/waypoint-gtk/issues) to see what's being worked on

---

## Development Setup

### Prerequisites

- Void Linux with Btrfs filesystem
- Rust 1.70 or newer
- GTK 4.10+ and libadwaita 1.4+ development libraries
- D-Bus and Polkit

### Installing Dependencies

```bash
# Install build dependencies
sudo xbps-install -S base-devel rust cargo gtk4-devel libadwaita-devel \
    dbus-devel polkit-devel btrfs-progs
```

### Building from Source

```bash
# Clone the repository
git clone https://github.com/Letdown2491/waypoint-gtk.git
cd waypoint-gtk

# Build debug version
cargo build

# Run the GUI
cargo run --bin waypoint

# Run the CLI
cargo run --bin waypoint-cli -- --help

# Build release version
cargo build --release
```

### Project Structure

```
waypoint-gtk/
├── waypoint/              # GTK GUI application
├── waypoint-helper/       # Privileged D-Bus service
├── waypoint-cli           # Bash-based CLI wrapper
├── waypoint-scheduler/    # Runit scheduler service
├── waypoint-common/       # Shared library (types, config)
├── data/                  # D-Bus, Polkit, desktop files
├── services/              # Runit service definitions
└── docs/                  # Documentation
```

---

## Code Style

### Rust

- Follow standard Rust formatting: `cargo fmt`
- Run clippy and fix warnings: `cargo clippy`
- No compiler warnings allowed - code must compile cleanly
- Use meaningful variable names
- Add comments for complex logic
- Keep functions focused and reasonably sized

### Bash (waypoint-cli)

- Use `set -euo pipefail` at the top of scripts
- Quote variables: `"$variable"` not `$variable`
- Use functions for repeated logic
- Add comments for non-obvious behavior

### Documentation

- Keep documentation up to date with code changes
- Use clear, concise language
- Include code examples where helpful
- Follow existing documentation structure

---

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p waypoint-common

# Run tests with output
cargo test -- --nocapture
```

### Test Coverage

We aim for:
- Unit tests for business logic
- Integration tests for D-Bus methods
- Manual testing for GUI interactions

See [TESTING.md](docs/TESTING.md) for detailed testing guidelines.

### Before Submitting

Ensure your changes pass:

```bash
# Format check
cargo fmt --check

# Linting
cargo clippy -- -D warnings

# Build check
cargo build --release

# Test suite
cargo test
```

---

## Submitting Changes

### Creating Pull Requests

1. **Fork the repository** on GitHub
2. **Create a feature branch**: `git checkout -b feature/your-feature-name`
3. **Make your changes** with clear, focused commits
4. **Write tests** for new functionality
5. **Update documentation** if needed
6. **Run tests and linting**: `cargo test && cargo clippy`
7. **Push to your fork**: `git push origin feature/your-feature-name`
8. **Open a Pull Request** with a clear description

### Commit Messages

Write clear commit messages:

```
Add fstab validation for multi-subvolume restores

- Validates fstab syntax (4-6 fields per entry)
- Checks mount options (rw for root, subvol for btrfs)
- Verifies snapshot subvolumes exist
- Prevents boot failures from malformed config

Fixes #123
```

Format:
- First line: Brief summary (50 chars or less)
- Blank line
- Detailed explanation (wrap at 72 chars)
- Reference issues if applicable

### Pull Request Guidelines

- Keep PRs focused on a single feature or fix
- Include tests for new functionality
- Update relevant documentation
- Ensure CI passes (if configured)
- Respond to review feedback promptly
- Be patient - reviews take time

---

## Reporting Bugs

### Before Reporting

1. Check [existing issues](https://github.com/Letdown2491/waypoint-gtk/issues)
2. Verify the bug on latest version
3. Check [TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)

### Creating Bug Reports

Include:

1. **Waypoint version**: `waypoint --version`
2. **Void Linux version**: `uname -a`
3. **Btrfs filesystem layout**: `btrfs subvolume list /`
4. **Steps to reproduce** (numbered list)
5. **Expected behavior**
6. **Actual behavior**
7. **Error messages** (full output)
8. **Relevant logs**: Check `/var/log/waypoint-scheduler/current`

Example:

```markdown
**Version**: Waypoint 1.0.0
**System**: Void Linux 6.12.58_1 x86_64

**Steps to reproduce:**
1. Create snapshot with multiple subvolumes
2. Try to restore snapshot
3. Observe error

**Expected:** Snapshot restores successfully
**Actual:** Error: "fstab validation failed"

**Error message:**
```
Line 5: Mount point must be absolute path: home
```

**Logs:**
```
[relevant log output]
```
```

---

## Feature Requests

We welcome feature ideas! When suggesting features:

1. **Check existing issues** - it may already be planned
2. **Describe the use case** - what problem does it solve?
3. **Propose a solution** - how should it work?
4. **Consider alternatives** - what other approaches exist?

Be aware:
- Waypoint is designed specifically for Void Linux + Btrfs
- Features must align with security and simplicity goals
- Not all feature requests can be implemented

---

## Code of Conduct

Be respectful and constructive:

- Be welcoming to newcomers
- Provide helpful, constructive feedback
- Focus on what's best for the project
- Accept criticism gracefully
- Show empathy towards others

---

## Questions?

- Check the [User Guide](docs/USER_GUIDE.md)
- Review [ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Ask in [GitHub Discussions](https://github.com/Letdown2491/waypoint-gtk/discussions) (if enabled)
- File an issue with the `question` label

---

## License

By contributing to Waypoint, you agree that your contributions will be licensed under the MIT License.

Thank you for contributing to Waypoint! 🚀
