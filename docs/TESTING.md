# Testing Guide

This document explains how to run and write tests for Waypoint.

## Table of Contents

- [Running Tests](#running-tests)
- [Test Organization](#test-organization)
- [Test Coverage](#test-coverage)
- [Writing New Tests](#writing-new-tests)
- [Testing Conventions](#testing-conventions)
- [Continuous Integration](#continuous-integration)

## Running Tests

### Run All Tests

```sh
cargo test
```

This runs all unit tests and integration tests across all crates (waypoint, waypoint-helper, waypoint-common).

### Run Tests for Specific Package

```sh
# Test only waypoint-common
cargo test -p waypoint-common

# Test only waypoint-helper
cargo test -p waypoint-helper

# Test only waypoint (main GUI)
cargo test -p waypoint
```

### Run Library Tests Only

```sh
# Exclude binary tests, only run library tests
cargo test --lib
```

### Run Specific Test Module

```sh
# Run only validation tests
cargo test validation

# Run only retention policy tests
cargo test retention

# Run only backup config tests
cargo test backup_config
```

### Run Specific Test Function

```sh
cargo test test_validate_snapshot_name
cargo test test_timeline_retention_basic
```

### Run Tests with Output

```sh
# Show println! output even for passing tests
cargo test -- --nocapture

# Show test output and run tests sequentially
cargo test -- --nocapture --test-threads=1
```

### Run Tests in Release Mode

```sh
# Faster execution for performance-sensitive tests
cargo test --release
```

## Test Organization

Waypoint has comprehensive test coverage with 90+ test functions across multiple test modules:

### waypoint-common (39 tests)

The common library has the most comprehensive test coverage:

**waypoint-common/src/validation.rs**
- `test_validate_snapshot_name` - Valid snapshot names
- `test_validate_snapshot_name_invalid` - Invalid characters, empty names
- `test_validate_path` - Path traversal prevention
- `test_validate_subvolume` - Subvolume path validation
- `test_validate_description` - Description length and content
- `test_validate_prefix` - Prefix format validation
- `test_validate_cron_expression` - Cron syntax validation
- Additional validation tests for edge cases

**waypoint-common/src/exclude.rs**
- `test_is_excluded` - Pattern matching for excluded paths
- `test_is_excluded_with_wildcards` - Glob pattern support
- `test_is_excluded_directories` - Directory exclusion rules

**waypoint-common/src/retention.rs**
- `test_timeline_retention_basic` - Basic timeline bucket logic
- `test_timeline_retention_empty` - Empty snapshot list handling
- `test_timeline_retention_keep_all` - Zero bucket configuration
- `test_timeline_retention_complex` - Multi-bucket scenarios
- `test_get_snapshots_to_delete_respects_pinned` - Pinned snapshot protection
- `test_get_snapshots_to_delete_respects_minimum` - Minimum count enforcement
- Additional retention policy tests

**waypoint-common/src/schedules.rs**
- `test_parse_cron_expression` - Cron parsing
- `test_calculate_next_run` - Next run time calculation
- `test_is_schedule_due` - Due time detection
- Schedule validation tests

**waypoint-common/src/backup_config.rs**
- `test_backup_config_serialization` - JSON serialization
- `test_backup_destination_validation` - Destination path checks
- `test_backup_config_defaults` - Default value behavior

**waypoint-common/src/config.rs**
- `test_config_load` - Configuration file loading
- `test_config_save` - Configuration persistence
- `test_config_defaults` - Default configuration values
- `test_config_validation` - Invalid configuration rejection

**waypoint-common/src/quota.rs**
- `test_parse_quota_size` - Human-readable size parsing (50G, 1T)
- `test_quota_calculations` - Usage percentage calculations
- `test_quota_exceeded` - Limit detection

### waypoint-helper (minimal tests)

**waypoint-helper/src/btrfs.rs**
- Basic btrfs command construction tests
- Error handling tests

### waypoint (UI and core logic tests)

**waypoint/src/packages.rs**
- `test_parse_xbps_output` - XBPS package list parsing
- `test_package_diff_calculation` - Diff between package states
- `test_version_comparison` - Version string parsing

**waypoint/src/cache.rs**
- `test_cache_expiration` - TTL-based cache eviction
- `test_cache_hit_miss` - Cache hit/miss behavior

**waypoint/src/performance.rs**
- `test_bulk_query_optimization` - Batch size calculation
- `test_parallel_processing` - Rayon parallelism

**waypoint/src/snapshot.rs**
- `test_snapshot_metadata_serialization` - JSON persistence
- `test_snapshot_comparison` - Snapshot diff logic

**waypoint/src/subvolume.rs**
- `test_detect_subvolumes` - Automatic subvolume detection
- `test_subvolume_filtering` - Filtering logic

**waypoint/src/ui/create_snapshot_dialog.rs**
- `test_subvolume_selection_validation` - UI validation logic

**waypoint/src/ui/error_helpers.rs**
- `test_error_message_sanitization` - Path sanitization in errors
- `test_user_friendly_error_messages` - Error message formatting

**waypoint/src/ui/validation.rs**
- `test_ui_input_validation` - Form input validation
- `test_path_input_sanitization` - Path input cleaning

## Test Coverage

### Current Coverage Summary

- ✅ **Validation** - Comprehensive coverage of all input validation
- ✅ **Retention Policies** - All retention logic tested (timeline, min counts, pinned)
- ✅ **Schedules** - Cron parsing and next-run calculation
- ✅ **Backup Configuration** - Serialization, validation, defaults
- ✅ **Configuration** - Loading, saving, validation
- ✅ **Quotas** - Size parsing, calculations, limit detection
- ✅ **Package Tracking** - XBPS parsing, diffs, version comparison
- ✅ **Caching** - TTL expiration, hit/miss behavior
- ⚠️ **UI Components** - Minimal coverage (only validation helpers)
- ⚠️ **Btrfs Operations** - Minimal coverage (mostly in waypoint-helper)
- ⚠️ **D-Bus Integration** - No unit tests (requires integration tests)

### Areas for Improvement

1. **Integration Tests** - No integration tests for D-Bus service
2. **UI Tests** - Limited GTK widget testing
3. **Btrfs Operations** - More tests for waypoint-helper commands
4. **End-to-End Tests** - No full workflow tests (create → backup → restore)

## Writing New Tests

### Basic Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        // Arrange - Set up test data
        let input = "test-snapshot";

        // Act - Execute the function
        let result = validate_snapshot_name(input);

        // Assert - Verify the result
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), input);
    }
}
```

### Testing Error Cases

```rust
#[test]
fn test_invalid_input() {
    let result = validate_snapshot_name("invalid name with spaces");
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Snapshot names cannot contain spaces"
    );
}
```

### Testing with Mock Data

```rust
#[test]
fn test_retention_policy() {
    use chrono::Utc;

    let now = Utc::now();
    let snapshots = vec![
        SnapshotMetadata {
            name: "hourly-1".to_string(),
            timestamp: now - chrono::Duration::hours(1),
            pinned: false,
            ..Default::default()
        },
        SnapshotMetadata {
            name: "hourly-2".to_string(),
            timestamp: now - chrono::Duration::hours(2),
            pinned: true, // This should be kept
            ..Default::default()
        },
    ];

    let to_delete = get_snapshots_to_delete(&snapshots, &retention_policy);

    // Should not delete pinned snapshots
    assert!(!to_delete.contains(&"hourly-2".to_string()));
}
```

### Testing Async Functions

```rust
#[tokio::test]
async fn test_async_operation() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

### Testing with Temporary Files

```rust
#[test]
fn test_config_file_operations() {
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");

    let config = Config::default();
    config.save(&config_path).unwrap();

    let loaded = Config::load(&config_path).unwrap();
    assert_eq!(config, loaded);
}
```

## Testing Conventions

### Naming Conventions

- Test modules: `mod tests { ... }`
- Test functions: `test_<feature>_<scenario>`
- Examples:
  - `test_validate_snapshot_name_valid`
  - `test_validate_snapshot_name_invalid`
  - `test_retention_policy_respects_pinned`

### Assertions

Use descriptive assertion messages:

```rust
assert!(
    result.is_ok(),
    "Expected valid snapshot name to pass validation, got: {:?}",
    result
);
```

### Test Organization

- Keep tests in the same file as the code they test (inline `#[cfg(test)]` modules)
- Group related tests in the same `mod tests` block
- Use descriptive test function names that explain what's being tested

### Test Data

- Use realistic test data (e.g., actual snapshot names like "hourly-20251118-1400")
- Test edge cases (empty strings, maximum lengths, special characters)
- Test boundary conditions (minimum/maximum values)

### Cleanup

- Use `tempfile::tempdir()` for temporary directories (auto-cleanup)
- Use `#[test]` instead of manual test runners
- Avoid side effects that persist after tests

## Continuous Integration

### Pre-commit Checks

Before committing, ensure all tests pass:

```sh
# Run all tests
cargo test

# Check for compilation warnings
cargo clippy

# Format code
cargo fmt
```

### Build Verification

```sh
# Clean build from scratch
cargo clean
cargo build --release

# Verify no warnings
cargo build 2>&1 | grep warning
```

### Test Before Release

```sh
# Full test suite in release mode
cargo test --release --all

# Verify all binaries work
./target/release/waypoint-cli --version
./target/release/waypoint-scheduler --version
```

## Measuring Code Coverage

To measure test coverage, use `cargo-tarpaulin`:

```sh
# Install tarpaulin
cargo install cargo-tarpaulin

# Run coverage analysis
cargo tarpaulin --out Html --output-dir coverage

# Open coverage report
xdg-open coverage/index.html
```

## Common Test Patterns

### Testing Validation Functions

```rust
#[test]
fn test_validation() {
    // Valid inputs
    assert!(validate("valid-input").is_ok());

    // Invalid inputs
    assert!(validate("").is_err());
    assert!(validate("invalid input").is_err());
}
```

### Testing Serialization

```rust
#[test]
fn test_serialization() {
    let original = MyStruct { field: "value".to_string() };
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: MyStruct = serde_json::from_str(&json).unwrap();
    assert_eq!(original, deserialized);
}
```

### Testing Time-Based Logic

```rust
#[test]
fn test_time_based_retention() {
    use chrono::{Utc, Duration};

    let now = Utc::now();
    let old_snapshot = now - Duration::days(30);

    assert!(should_delete(old_snapshot, now));
}
```

## Running Tests in CI/CD

Example GitHub Actions workflow:

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - run: cargo test --all
      - run: cargo clippy -- -D warnings
```

## Debugging Failed Tests

### Run Single Test with Output

```sh
cargo test test_name -- --nocapture --test-threads=1
```

### Use `dbg!()` Macro

```rust
#[test]
fn test_debug() {
    let value = compute_value();
    dbg!(&value); // Prints debug info
    assert_eq!(value, expected);
}
```

### Enable Debug Logging

```sh
RUST_LOG=debug cargo test test_name -- --nocapture
```

## Best Practices

1. **Test public APIs** - Focus on testing public functions that other code depends on
2. **Test error paths** - Don't just test the happy path, test failures too
3. **Keep tests fast** - Avoid slow I/O, use mocks where possible
4. **Make tests deterministic** - No random data, no dependency on external state
5. **One assertion per test** - Makes failures easier to diagnose
6. **Use descriptive names** - Test name should describe what's being tested
7. **Test edge cases** - Empty inputs, maximum values, special characters
8. **Don't test implementation details** - Test behavior, not internal structure

## Additional Resources

- [Rust Testing Documentation](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Rust By Example - Testing](https://doc.rust-lang.org/rust-by-example/testing.html)
- [cargo test Documentation](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
