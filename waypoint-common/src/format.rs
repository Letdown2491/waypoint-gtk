//! Formatting utilities for displaying sizes and times

/// Format bytes as human-readable size using binary units (KiB, MiB, GiB)
///
/// # Examples
/// ```
/// use waypoint_common::format_bytes;
/// assert_eq!(format_bytes(1024), "1.00 KiB");
/// assert_eq!(format_bytes(1536), "1.50 KiB");
/// assert_eq!(format_bytes(1048576), "1.00 MiB");
/// ```
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{size:.2} {}", UNITS[unit_idx])
}

/// Format elapsed time into human-readable string
///
/// # Examples
/// ```
/// use waypoint_common::format_elapsed_time;
/// assert_eq!(format_elapsed_time(30), "30s");
/// assert_eq!(format_elapsed_time(90), "1m 30s");
/// assert_eq!(format_elapsed_time(3665), "1h 1m");
/// assert_eq!(format_elapsed_time(90000), "1d 1h");
/// ```
pub fn format_elapsed_time(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        if secs == 0 {
            format!("{mins}m")
        } else {
            format!("{mins}m {secs}s")
        }
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        if mins == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {mins}m")
        }
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        if hours == 0 {
            format!("{days}d")
        } else {
            format!("{days}d {hours}h")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0.00 B");
        assert_eq!(format_bytes(512), "512.00 B");
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1536), "1.50 KiB");
        assert_eq!(format_bytes(1048576), "1.00 MiB");
        assert_eq!(format_bytes(1073741824), "1.00 GiB");
    }

    #[test]
    fn test_format_elapsed_time() {
        assert_eq!(format_elapsed_time(0), "0s");
        assert_eq!(format_elapsed_time(30), "30s");
        assert_eq!(format_elapsed_time(60), "1m");
        assert_eq!(format_elapsed_time(90), "1m 30s");
        assert_eq!(format_elapsed_time(3600), "1h");
        assert_eq!(format_elapsed_time(3665), "1h 1m");
        assert_eq!(format_elapsed_time(86400), "1d");
        assert_eq!(format_elapsed_time(90000), "1d 1h");
    }
}
