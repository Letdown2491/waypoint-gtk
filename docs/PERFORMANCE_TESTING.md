# Performance Testing Guide

This guide explains how to test and profile Waypoint's performance, particularly with large numbers of snapshots.

## Performance Monitoring

Waypoint includes built-in performance tracking that measures timing for key operations:

- **refresh_snapshot_list** - Total time to refresh the snapshot list UI
- **load_snapshots** - Time to load snapshot metadata from disk
- **filter_snapshots** - Time to filter snapshots based on search criteria
- **populate_ui** - Time to create and populate UI widgets
- **get_snapshot_size** - Time to calculate snapshot size
- **get_snapshot_size_cache_hit** - Time for cached snapshot size lookups
- **du_command** - Time for the actual `du` command execution
- **get_available_space** - Time to check available disk space
- **df_command** - Time for the actual `df` command execution
- **bulk_snapshot_sizes** - Time to query multiple snapshot sizes in parallel (analytics)
- **backup_progress_update** - Time to emit backup progress signals

### Enabling Performance Logging

To view performance statistics, run Waypoint with debug logging enabled:

```bash
RUST_LOG=debug cargo run
```

Or for the installed binary:

```bash
RUST_LOG=debug waypoint
```

Performance statistics will be logged after each snapshot list refresh.

### Understanding the Output

Example output:

```text
[DEBUG] === Performance Statistics ===
[DEBUG] refresh_snapshot_list: 5 calls, total 226.15ms, avg 45.23ms, median 43.10ms (min 38.50ms, max 56.20ms)
[DEBUG] load_snapshots: 5 calls, total 61.70ms, avg 12.34ms, median 11.20ms (min 10.50ms, max 15.80ms)
[DEBUG] filter_snapshots: 5 calls, total 10.75ms, avg 2.15ms, median 2.10ms (min 1.90ms, max 2.50ms)
[DEBUG] populate_ui: 5 calls, total 142.25ms, avg 28.45ms, median 27.30ms (min 24.10ms, max 35.20ms)
[DEBUG] get_snapshot_size: 8 calls, total 1005.36ms, avg 125.67ms, median 98.20ms (min 85.30ms, max 245.10ms)
[DEBUG] get_snapshot_size_cache_hit: 3 calls, total 0.15ms, avg 0.05ms, median 0.04ms (min 0.03ms, max 0.08ms)
[DEBUG] du_command: 5 calls, total 621.15ms, avg 124.23ms, median 97.50ms (min 84.80ms, max 244.20ms)
[DEBUG] ==============================
```

**Key metrics:**
- **calls** - Number of times the operation was executed
- **total** - Total cumulative time spent in this operation
- **avg** - Average execution time per call
- **median** - Median execution time (less affected by outliers)
- **min/max** - Fastest and slowest execution times

## Testing with Large Snapshot Counts

### Creating Test Snapshots

To test performance with many snapshots, create multiple snapshots using the application:

1. Enable debug logging: `RUST_LOG=debug cargo run`
2. Create snapshots manually using the "Create Restore Point" button
3. Use different descriptions to test filtering performance
4. Monitor the debug output after each UI refresh

### Recommended Test Scenarios

#### Baseline (10-20 snapshots)
- Create 10-20 snapshots
- Note refresh times
- Test search and filtering

#### Medium Load (50-100 snapshots)
- Create 50-100 snapshots over time
- Monitor UI responsiveness
- Check if cache is effective (compare cache hit vs. cache miss times)

#### Heavy Load (200+ snapshots)
- Create 200+ snapshots (if disk space allows)
- Measure list refresh performance
- Test search with various queries
- Check memory usage with `htop` or `ps`

### Performance Expectations

**Well-Optimized Operations:**
- Snapshot list refresh: < 100ms for 100 snapshots
- Search/filtering: < 10ms for 100 snapshots
- Cache hits: < 1ms
- UI population: < 50ms for 100 rows
- Bulk snapshot size query: < 2s for 50 snapshots (parallel processing)
- Analytics dashboard load: < 3s for 100 snapshots

**Expensive Operations (cached):**
- `du` command: 100ms - 5s depending on snapshot size
- Cache reduces this to < 1ms for repeated queries
- Cache TTL: 5 minutes for snapshot sizes, 30 seconds for disk space
- Backup operations: Progress updates every 100ms for responsive UI

### Performance Bottlenecks

Based on the code audit, potential bottlenecks include:

1. **Snapshot size calculation** (`du` command)
   - Mitigated by 5-minute cache
   - Runs in background thread
   - Bulk queries use parallel processing with rayon
   - Only calculated when viewing details or analytics

2. **UI widget creation** (creating 100+ GTK widgets)
   - Each snapshot creates a complex `SnapshotRow` widget
   - Uses `Rc<T>` for cheap cloning of expensive data
   - Could be optimized with virtual scrolling for 500+ snapshots

3. **Disk I/O** (loading snapshot metadata)
   - Already cached by filesystem
   - Typically < 20ms even with 100 snapshots

4. **Analytics calculations** (storage trends, retention analysis)
   - Uses bulk D-Bus queries to minimize round-trips
   - Parallel size calculation for multiple snapshots
   - Results cached for dashboard performance

## Optimization Status

The following optimizations are already implemented:

✅ **TTL Caching**
- 5-minute cache for snapshot sizes (`du` command)
- 30-second cache for available disk space (`df` command)

✅ **Background Threading**
- All expensive operations run in background threads
- UI remains responsive during operations

✅ **Parallel Computation**
- Snapshot size calculations use rayon for parallel processing
- Multiple snapshots sized concurrently (analytics dashboard)
- Optimized bulk snapshot size queries via D-Bus

✅ **Cheap Cloning**
- Snapshot data uses `Rc<Vec<T>>` for packages and subvolumes
- Filtering and sorting don't duplicate large data structures

✅ **Performance Instrumentation**
- Comprehensive timing for all operations
- Debug logging for analysis
- Detailed performance profiling with operation breakdowns
- Performance statistics include min/max/avg/median for all measured operations

✅ **Bulk Query Optimization**
- `GetSnapshotSizes()` D-Bus method for batch size queries
- Reduces round-trip overhead when querying multiple snapshots
- Analytics dashboard uses bulk queries for efficiency
- Parallel processing of bulk requests with rayon

✅ **Backup Progress Tracking**
- Real-time progress updates via D-Bus `BackupProgress` signal
- Tracks bytes transferred, total bytes, and transfer speed
- Progress updates sent every 100ms during transfers
- Non-blocking UI with responsive progress indicators
- Stage tracking: preparing, transferring, verifying, complete

✅ **Rate Limiting (DoS Prevention)**
- Per-user, per-operation rate limiting in waypoint-helper
- 1 operation per 5 seconds per user for expensive operations
- Prevents system overload from malicious or buggy clients
- Mutex poisoning detection and recovery for robustness

## Memory Usage

Monitor memory usage during testing:

```bash
# Terminal 1: Run Waypoint with profiling
RUST_LOG=debug cargo run 2>&1 | grep Performance

# Terminal 2: Monitor memory
watch -n 1 'ps aux | grep waypoint | grep -v grep'
```

Expected memory usage:
- < 50 MB with 10 snapshots
- < 100 MB with 100 snapshots
- < 200 MB with 500 snapshots

## Reporting Performance Issues

If you encounter performance problems:

1. Run with `RUST_LOG=debug`
2. Note the number of snapshots
3. Copy the performance statistics output
4. Note which operations are slow
5. Report on GitHub with:
   - Performance statistics
   - Number of snapshots
   - System specs (CPU, RAM, disk type)
   - Btrfs filesystem configuration

## Future Optimization Ideas

If performance becomes an issue with very large snapshot counts (500+):

1. **Virtual scrolling** - Only create widgets for visible snapshots
2. **Lazy loading** - Load snapshot details on demand
3. **Incremental updates** - Reuse existing widgets when data hasn't changed
4. **Parallel loading** - Load multiple snapshot metadata files in parallel
5. **Database backend** - Use SQLite for faster queries on large datasets
