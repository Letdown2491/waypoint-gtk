//! Performance monitoring and optimization utilities
//!
//! This module provides utilities for measuring and improving application performance,
//! particularly for expensive filesystem operations.
//!
//! # Usage
//!
//! Performance tracking is automatically enabled throughout the application for key operations:
//! - `refresh_snapshot_list` - Measures total time to refresh the snapshot list UI
//! - `load_snapshots` - Measures time to load snapshot metadata from disk
//! - `filter_snapshots` - Measures time to filter snapshots based on search criteria
//! - `populate_ui` - Measures time to create and populate UI widgets
//! - `get_snapshot_size` - Measures time to calculate snapshot size (including cache hits)
//! - `du_command` - Measures time for the actual `du` command execution
//! - `get_available_space` - Measures time to check available disk space
//! - `df_command` - Measures time for the actual `df` command execution
//!
//! # Viewing Statistics
//!
//! To view performance statistics, run the application with debug logging enabled:
//!
//! ```bash
//! RUST_LOG=debug cargo run
//! ```
//!
//! Statistics will be logged after each snapshot list refresh, showing:
//! - Number of times each operation was called
//! - Average, median, min, and max execution times
//!
//! # Example Output
//!
//! ```text
//! [DEBUG] === Performance Statistics ===
//! [DEBUG] refresh_snapshot_list: 5 calls, total 226.15ms, avg 45.23ms, median 43.10ms (min 38.50ms, max 56.20ms)
//! [DEBUG] load_snapshots: 5 calls, total 61.70ms, avg 12.34ms, median 11.20ms (min 10.50ms, max 15.80ms)
//! [DEBUG] filter_snapshots: 5 calls, total 10.75ms, avg 2.15ms, median 2.10ms (min 1.90ms, max 2.50ms)
//! [DEBUG] populate_ui: 5 calls, total 142.25ms, avg 28.45ms, median 27.30ms (min 24.10ms, max 35.20ms)
//! [DEBUG] get_snapshot_size: 8 calls, total 1005.36ms, avg 125.67ms, median 98.20ms (min 85.30ms, max 245.10ms)
//! [DEBUG] ==============================
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Performance metrics tracker
///
/// Tracks timing data for operations to identify bottlenecks
#[derive(Debug, Clone)]
pub struct PerformanceTracker {
    measurements: Arc<Mutex<VecDeque<Measurement>>>,
    max_measurements: usize,
}

#[derive(Debug, Clone)]
struct Measurement {
    operation: String,
    duration: Duration,
}

impl PerformanceTracker {
    /// Create a new performance tracker
    ///
    /// # Arguments
    /// * `max_measurements` - Maximum number of measurements to keep in memory
    pub fn new(max_measurements: usize) -> Self {
        Self {
            measurements: Arc::new(Mutex::new(VecDeque::with_capacity(max_measurements))),
            max_measurements,
        }
    }

    /// Record the start of an operation
    ///
    /// Returns a `PerformanceTimer` that will automatically record the duration when dropped
    pub fn start(&self, operation: impl Into<String>) -> PerformanceTimer {
        PerformanceTimer {
            operation: operation.into(),
            start: Instant::now(),
            tracker: self.clone(),
        }
    }

    /// Manually record a measurement
    pub fn record(&self, operation: impl Into<String>, duration: Duration) {
        let measurement = Measurement {
            operation: operation.into(),
            duration,
        };

        if let Ok(mut measurements) = self.measurements.lock() {
            if measurements.len() >= self.max_measurements {
                measurements.pop_front();
            }
            measurements.push_back(measurement);
        }
    }

    /// Get statistics for a specific operation
    pub fn get_stats(&self, operation: &str) -> Option<OperationStats> {
        if let Ok(measurements) = self.measurements.lock() {
            let matching: Vec<&Measurement> = measurements
                .iter()
                .filter(|m| m.operation == operation)
                .collect();

            if matching.is_empty() {
                return None;
            }

            let durations: Vec<Duration> = matching.iter().map(|m| m.duration).collect();
            let total: Duration = durations.iter().sum();
            let count = durations.len();
            let avg = total / count as u32;

            let max = durations.iter().max().copied().unwrap_or(Duration::ZERO);
            let min = durations.iter().min().copied().unwrap_or(Duration::ZERO);

            // Calculate median (requires sorted data)
            let mut sorted = durations.clone();
            sorted.sort();
            let median = if count % 2 == 0 {
                let mid = count / 2;
                (sorted[mid - 1] + sorted[mid]) / 2
            } else {
                sorted[count / 2]
            };

            Some(OperationStats {
                operation: operation.to_string(),
                count,
                total,
                average: avg,
                median,
                min,
                max,
            })
        } else {
            None
        }
    }

    /// Get all unique operation names
    pub fn get_operations(&self) -> Vec<String> {
        if let Ok(measurements) = self.measurements.lock() {
            let mut ops: Vec<String> = measurements.iter().map(|m| m.operation.clone()).collect();
            ops.sort();
            ops.dedup();
            ops
        } else {
            Vec::new()
        }
    }

    /// Get all statistics for all tracked operations
    pub fn get_all_stats(&self) -> Vec<OperationStats> {
        self.get_operations()
            .iter()
            .filter_map(|op| self.get_stats(op))
            .collect()
    }

    /// Clear all measurements
    #[allow(dead_code)]
    pub fn clear(&self) {
        if let Ok(mut measurements) = self.measurements.lock() {
            measurements.clear();
        }
    }
}

/// RAII timer that records duration when dropped
pub struct PerformanceTimer {
    operation: String,
    start: Instant,
    tracker: PerformanceTracker,
}

impl Drop for PerformanceTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.tracker.record(&self.operation, duration);
    }
}

/// Statistics for a specific operation
#[derive(Debug, Clone)]
pub struct OperationStats {
    pub operation: String,
    pub count: usize,
    pub total: Duration,
    pub average: Duration,
    pub median: Duration,
    pub min: Duration,
    pub max: Duration,
}

impl OperationStats {
    /// Format stats as a human-readable string
    pub fn format(&self) -> String {
        format!(
            "{}: {} calls, total {:.2}ms, avg {:.2}ms, median {:.2}ms (min {:.2}ms, max {:.2}ms)",
            self.operation,
            self.count,
            self.total.as_secs_f64() * 1000.0,
            self.average.as_secs_f64() * 1000.0,
            self.median.as_secs_f64() * 1000.0,
            self.min.as_secs_f64() * 1000.0,
            self.max.as_secs_f64() * 1000.0
        )
    }
}

/// Global performance tracker instance
static GLOBAL_TRACKER: once_cell::sync::Lazy<PerformanceTracker> =
    once_cell::sync::Lazy::new(|| PerformanceTracker::new(1000));

/// Get the global performance tracker
pub fn tracker() -> &'static PerformanceTracker {
    &GLOBAL_TRACKER
}

/// Log all performance statistics at debug level
///
/// This will output timing statistics for all tracked operations.
/// Only logs if the log level is set to debug or lower.
pub fn log_stats() {
    if !log::log_enabled!(log::Level::Debug) {
        return;
    }

    let stats = tracker().get_all_stats();
    if stats.is_empty() {
        log::debug!("No performance statistics collected");
        return;
    }

    log::debug!("=== Performance Statistics ===");
    for stat in stats {
        log::debug!("{}", stat.format());
    }
    log::debug!("==============================");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_performance_tracker_basic() {
        let tracker = PerformanceTracker::new(100);

        tracker.record("test_op", Duration::from_millis(10));
        tracker.record("test_op", Duration::from_millis(20));
        tracker.record("test_op", Duration::from_millis(15));

        let stats = tracker.get_stats("test_op").unwrap();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min, Duration::from_millis(10));
        assert_eq!(stats.max, Duration::from_millis(20));
        assert_eq!(stats.median, Duration::from_millis(15));
    }

    #[test]
    fn test_performance_timer_raii() {
        let tracker = PerformanceTracker::new(100);

        {
            let _timer = tracker.start("sleep_test");
            thread::sleep(Duration::from_millis(10));
        } // Timer drops here and records measurement

        let stats = tracker.get_stats("sleep_test").unwrap();
        assert_eq!(stats.count, 1);
        assert!(stats.average >= Duration::from_millis(10));
    }

    #[test]
    fn test_tracker_max_measurements() {
        let tracker = PerformanceTracker::new(3);

        for i in 0..5 {
            tracker.record("test", Duration::from_millis(i * 10));
        }

        let stats = tracker.get_stats("test").unwrap();
        assert_eq!(stats.count, 3); // Should only keep last 3
    }

    #[test]
    fn test_get_operations() {
        let tracker = PerformanceTracker::new(100);

        tracker.record("op1", Duration::from_millis(10));
        tracker.record("op2", Duration::from_millis(20));
        tracker.record("op1", Duration::from_millis(15));

        let ops = tracker.get_operations();
        assert_eq!(ops.len(), 2);
        assert!(ops.contains(&"op1".to_string()));
        assert!(ops.contains(&"op2".to_string()));
    }

    #[test]
    fn test_format_stats() {
        let stats = OperationStats {
            operation: "test_op".to_string(),
            count: 5,
            total: Duration::from_millis(100),
            average: Duration::from_millis(20),
            median: Duration::from_millis(18),
            min: Duration::from_millis(15),
            max: Duration::from_millis(30),
        };

        let formatted = stats.format();
        assert!(formatted.contains("test_op"));
        assert!(formatted.contains("5 calls"));
        assert!(formatted.contains("total 100.00ms"));
        assert!(formatted.contains("avg 20.00ms"));
    }

    #[test]
    fn test_clear_measurements() {
        let tracker = PerformanceTracker::new(100);

        tracker.record("test", Duration::from_millis(10));
        assert!(tracker.get_stats("test").is_some());

        tracker.clear();
        assert!(tracker.get_stats("test").is_none());
    }
}
