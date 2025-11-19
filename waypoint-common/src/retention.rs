// Timeline-based retention policy implementation
// Similar to Snapper's timeline cleanup algorithm

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Timeline retention configuration for a schedule
///
/// This implements a Snapper-style retention policy where snapshots are kept
/// based on timeline buckets (hourly, daily, weekly, monthly, yearly).
///
/// For each time period, we keep the most recent snapshot in that bucket.
/// For example, if `hourly_limit` is 24, we keep the most recent snapshot
/// from each of the last 24 hours.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineRetention {
    /// Number of hourly snapshots to keep (0 = disabled)
    pub hourly_limit: u32,

    /// Number of daily snapshots to keep (0 = disabled)
    pub daily_limit: u32,

    /// Number of weekly snapshots to keep (0 = disabled)
    pub weekly_limit: u32,

    /// Number of monthly snapshots to keep (0 = disabled)
    pub monthly_limit: u32,

    /// Number of yearly snapshots to keep (0 = disabled)
    pub yearly_limit: u32,
}

impl Default for TimelineRetention {
    fn default() -> Self {
        Self {
            hourly_limit: 0,
            daily_limit: 7,
            weekly_limit: 4,
            monthly_limit: 3,
            yearly_limit: 0,
        }
    }
}

impl TimelineRetention {
    /// Create retention for hourly schedule
    pub fn for_hourly() -> Self {
        Self {
            hourly_limit: 24,
            daily_limit: 0,
            weekly_limit: 0,
            monthly_limit: 0,
            yearly_limit: 0,
        }
    }

    /// Create retention for daily schedule
    pub fn for_daily() -> Self {
        Self {
            hourly_limit: 0,
            daily_limit: 7,
            weekly_limit: 0,
            monthly_limit: 0,
            yearly_limit: 0,
        }
    }

    /// Create retention for weekly schedule
    pub fn for_weekly() -> Self {
        Self {
            hourly_limit: 0,
            daily_limit: 0,
            weekly_limit: 4,
            monthly_limit: 0,
            yearly_limit: 0,
        }
    }

    /// Create retention for monthly schedule
    pub fn for_monthly() -> Self {
        Self {
            hourly_limit: 0,
            daily_limit: 0,
            weekly_limit: 0,
            monthly_limit: 12,
            yearly_limit: 0,
        }
    }
}

/// Time bucket for grouping snapshots
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum TimeBucket {
    Hourly { year: i32, day_of_year: u32, hour: u32 },
    Daily { year: i32, day_of_year: u32 },
    Weekly { year: i32, week: u32 },
    Monthly { year: i32, month: u32 },
    Yearly { year: i32 },
}

impl TimeBucket {
    fn from_timestamp_hourly(timestamp: DateTime<Utc>) -> Self {
        TimeBucket::Hourly {
            year: timestamp.year(),
            day_of_year: timestamp.ordinal(),
            hour: timestamp.hour(),
        }
    }

    fn from_timestamp_daily(timestamp: DateTime<Utc>) -> Self {
        TimeBucket::Daily {
            year: timestamp.year(),
            day_of_year: timestamp.ordinal(),
        }
    }

    fn from_timestamp_weekly(timestamp: DateTime<Utc>) -> Self {
        // ISO week numbering
        let week = timestamp.iso_week().week();
        TimeBucket::Weekly {
            year: timestamp.year(),
            week,
        }
    }

    fn from_timestamp_monthly(timestamp: DateTime<Utc>) -> Self {
        TimeBucket::Monthly {
            year: timestamp.year(),
            month: timestamp.month(),
        }
    }

    fn from_timestamp_yearly(timestamp: DateTime<Utc>) -> Self {
        TimeBucket::Yearly {
            year: timestamp.year(),
        }
    }
}

/// A snapshot with its timestamp for retention calculation
#[derive(Debug, Clone)]
pub struct SnapshotForRetention {
    pub name: String,
    pub timestamp: DateTime<Utc>,
}

/// Apply timeline-based retention to a list of snapshots
/// Returns the names of snapshots that should be deleted
pub fn apply_timeline_retention(
    snapshots: &[SnapshotForRetention],
    retention: &TimelineRetention,
    now: DateTime<Utc>,
) -> Vec<String> {
    let mut to_keep = HashSet::new();

    // Sort by timestamp (newest first) for easier processing
    let mut sorted = snapshots.to_vec();
    sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Process each time bucket type
    if retention.hourly_limit > 0 {
        keep_timeline_buckets(
            &sorted,
            retention.hourly_limit,
            now,
            TimeBucket::from_timestamp_hourly,
            |ts| now.signed_duration_since(ts) <= Duration::hours(retention.hourly_limit as i64),
            &mut to_keep,
        );
    }

    if retention.daily_limit > 0 {
        keep_timeline_buckets(
            &sorted,
            retention.daily_limit,
            now,
            TimeBucket::from_timestamp_daily,
            |ts| now.signed_duration_since(ts) <= Duration::days(retention.daily_limit as i64),
            &mut to_keep,
        );
    }

    if retention.weekly_limit > 0 {
        keep_timeline_buckets(
            &sorted,
            retention.weekly_limit,
            now,
            TimeBucket::from_timestamp_weekly,
            |ts| now.signed_duration_since(ts) <= Duration::weeks(retention.weekly_limit as i64),
            &mut to_keep,
        );
    }

    if retention.monthly_limit > 0 {
        keep_timeline_buckets(
            &sorted,
            retention.monthly_limit,
            now,
            TimeBucket::from_timestamp_monthly,
            |ts| {
                // Approximate: 30 days per month
                now.signed_duration_since(ts) <= Duration::days(30 * retention.monthly_limit as i64)
            },
            &mut to_keep,
        );
    }

    if retention.yearly_limit > 0 {
        keep_timeline_buckets(
            &sorted,
            retention.yearly_limit,
            now,
            TimeBucket::from_timestamp_yearly,
            |ts| {
                // Approximate: 365 days per year
                now.signed_duration_since(ts) <= Duration::days(365 * retention.yearly_limit as i64)
            },
            &mut to_keep,
        );
    }

    // Return snapshots that are not in the keep set
    snapshots
        .iter()
        .filter(|s| !to_keep.contains(&s.name))
        .map(|s| s.name.clone())
        .collect()
}

/// Keep the most recent snapshot in each time bucket up to the limit
fn keep_timeline_buckets<F, P>(
    snapshots: &[SnapshotForRetention],
    limit: u32,
    _now: DateTime<Utc>,
    bucket_fn: F,
    in_range_fn: P,
    to_keep: &mut HashSet<String>,
) where
    F: Fn(DateTime<Utc>) -> TimeBucket,
    P: Fn(DateTime<Utc>) -> bool,
{
    let mut seen_buckets = HashSet::new();
    let mut bucket_count = 0;

    for snapshot in snapshots {
        // Skip if outside the time range for this bucket type
        if !in_range_fn(snapshot.timestamp) {
            continue;
        }

        let bucket = bucket_fn(snapshot.timestamp);

        // If we haven't seen this bucket yet, keep this snapshot
        if !seen_buckets.contains(&bucket) {
            seen_buckets.insert(bucket);
            to_keep.insert(snapshot.name.clone());
            bucket_count += 1;

            // Stop if we've filled all buckets
            if bucket_count >= limit {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_hourly_retention() {
        let now = Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0).unwrap();

        let snapshots = vec![
            SnapshotForRetention {
                name: "hourly-20250115-1200".to_string(),
                timestamp: now,
            },
            SnapshotForRetention {
                name: "hourly-20250115-1100".to_string(),
                timestamp: now - Duration::hours(1),
            },
            SnapshotForRetention {
                name: "hourly-20250115-1000".to_string(),
                timestamp: now - Duration::hours(2),
            },
            SnapshotForRetention {
                name: "hourly-20250114-1200".to_string(),
                timestamp: now - Duration::hours(24),
            },
            SnapshotForRetention {
                name: "hourly-20250114-1100".to_string(),
                timestamp: now - Duration::hours(25),
            },
        ];

        let retention = TimelineRetention {
            hourly_limit: 24,
            ..Default::default()
        };

        let to_delete = apply_timeline_retention(&snapshots, &retention, now);

        // Should keep all within 24 hours, delete the one from 25 hours ago
        assert_eq!(to_delete.len(), 1);
        assert!(to_delete.contains(&"hourly-20250114-1100".to_string()));
    }

    #[test]
    fn test_daily_retention() {
        let now = Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0).unwrap();

        let snapshots = vec![
            SnapshotForRetention {
                name: "daily-20250115".to_string(),
                timestamp: now,
            },
            SnapshotForRetention {
                name: "daily-20250114".to_string(),
                timestamp: now - Duration::days(1),
            },
            SnapshotForRetention {
                name: "daily-20250113".to_string(),
                timestamp: now - Duration::days(2),
            },
            SnapshotForRetention {
                name: "daily-20250108".to_string(),
                timestamp: now - Duration::days(7),
            },
            SnapshotForRetention {
                name: "daily-20250107".to_string(),
                timestamp: now - Duration::days(8),
            },
        ];

        let retention = TimelineRetention {
            daily_limit: 7,
            ..Default::default()
        };

        let to_delete = apply_timeline_retention(&snapshots, &retention, now);

        // Should keep all within 7 days, delete the one from 8 days ago
        assert_eq!(to_delete.len(), 1);
        assert!(to_delete.contains(&"daily-20250107".to_string()));
    }

    #[test]
    fn test_multiple_snapshots_same_bucket() {
        let now = Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0).unwrap();

        // Multiple snapshots on the same day
        let snapshots = vec![
            SnapshotForRetention {
                name: "daily-20250115-1200".to_string(),
                timestamp: now,
            },
            SnapshotForRetention {
                name: "daily-20250115-1000".to_string(),
                timestamp: now - Duration::hours(2),
            },
            SnapshotForRetention {
                name: "daily-20250114".to_string(),
                timestamp: now - Duration::days(1),
            },
        ];

        let retention = TimelineRetention {
            daily_limit: 7,
            ..Default::default()
        };

        let to_delete = apply_timeline_retention(&snapshots, &retention, now);

        // Should keep the most recent from each day
        // The older snapshot from the same day should be deleted
        assert_eq!(to_delete.len(), 1);
        assert!(to_delete.contains(&"daily-20250115-1000".to_string()));
    }

    #[test]
    fn test_combined_retention() {
        let now = Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0).unwrap();

        let snapshots = vec![
            SnapshotForRetention {
                name: "snapshot-recent".to_string(),
                timestamp: now - Duration::hours(1),
            },
            SnapshotForRetention {
                name: "snapshot-2days".to_string(),
                timestamp: now - Duration::days(2),
            },
            SnapshotForRetention {
                name: "snapshot-10days".to_string(),
                timestamp: now - Duration::days(10),
            },
        ];

        let retention = TimelineRetention {
            hourly_limit: 24,
            daily_limit: 7,
            weekly_limit: 4,
            ..Default::default()
        };

        let to_delete = apply_timeline_retention(&snapshots, &retention, now);

        // Snapshot from 1h ago: kept by hourly
        // Snapshot from 2 days ago: kept by daily
        // Snapshot from 10 days ago: outside daily range but within weekly range
        assert_eq!(to_delete.len(), 0);
    }
}
