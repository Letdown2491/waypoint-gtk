//! Configuration validation utilities
//!
//! This module provides validation functions for all Waypoint configurations
//! to ensure data integrity and prevent invalid system states.

use std::path::Path;

/// Validation error
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Validation result
pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Validate a time string in HH:MM format
///
/// # Arguments
/// * `time` - Time string to validate
///
/// # Returns
/// `Ok(())` if valid, `Err` with details if invalid
///
/// # Examples
/// ```
/// # use waypoint_common::validation::validate_time_format;
/// assert!(validate_time_format("02:00").is_ok());
/// assert!(validate_time_format("23:59").is_ok());
/// assert!(validate_time_format("24:00").is_err());
/// assert!(validate_time_format("12:60").is_err());
/// assert!(validate_time_format("2:00").is_err()); // Must be zero-padded
/// ```
pub fn validate_time_format(time: &str) -> Result<(), String> {
    // Check format HH:MM
    if time.len() != 5 {
        return Err("Time must be in HH:MM format (e.g., 02:00)".to_string());
    }

    if !time.contains(':') {
        return Err("Time must contain ':'".to_string());
    }

    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 2 {
        return Err("Time must be in HH:MM format".to_string());
    }

    // Validate hours
    let hours: u32 = parts[0]
        .parse()
        .map_err(|_| "Hours must be a number".to_string())?;

    if hours > 23 {
        return Err("Hours must be between 00 and 23".to_string());
    }

    // Validate minutes
    let minutes: u32 = parts[1]
        .parse()
        .map_err(|_| "Minutes must be a number".to_string())?;

    if minutes > 59 {
        return Err("Minutes must be between 00 and 59".to_string());
    }

    // Ensure zero-padding
    if parts[0].len() != 2 || parts[1].len() != 2 {
        return Err("Hours and minutes must be zero-padded (e.g., 02:00, not 2:0)".to_string());
    }

    Ok(())
}

/// Validate scheduler frequency
///
/// # Arguments
/// * `frequency` - Frequency string ("hourly", "daily", "weekly", "monthly", or numeric)
///
/// # Returns
/// `Ok(frequency_code)` if valid (0=hourly, 1=daily, 2=weekly, 3=monthly), `Err` if invalid
pub fn validate_scheduler_frequency(frequency: &str) -> Result<u32, String> {
    match frequency {
        "hourly" | "0" => Ok(0),
        "daily" | "1" => Ok(1),
        "weekly" | "2" => Ok(2),
        "monthly" | "3" => Ok(3),
        _ => Err(format!(
            "Invalid frequency '{frequency}'. Must be one of: hourly, daily, weekly, monthly"
        )),
    }
}

/// Validate day of week for scheduler
///
/// # Arguments
/// * `day` - Day of week (0=Sunday, 1=Monday, ..., 6=Saturday)
///
/// # Returns
/// `Ok(())` if valid, `Err` if invalid
pub fn validate_day_of_week(day: &str) -> Result<(), String> {
    let day_num: u32 = day
        .parse()
        .map_err(|_| "Day of week must be a number between 0 and 6".to_string())?;

    if day_num > 6 {
        return Err("Day of week must be between 0 (Sunday) and 6 (Saturday)".to_string());
    }

    Ok(())
}

/// Validate snapshot name prefix
///
/// Ensures prefix follows naming rules (no special characters)
pub fn validate_snapshot_prefix(prefix: &str) -> Result<(), String> {
    if prefix.is_empty() {
        return Err("Snapshot prefix cannot be empty".to_string());
    }

    if prefix.len() > 50 {
        return Err("Snapshot prefix too long (max 50 characters)".to_string());
    }

    // Only allow alphanumeric, dash, underscore
    if !prefix
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Snapshot prefix can only contain letters, numbers, dashes, and underscores"
                .to_string(),
        );
    }

    // Cannot start with dash or dot
    if prefix.starts_with('-') || prefix.starts_with('.') {
        return Err("Snapshot prefix cannot start with '-' or '.'".to_string());
    }

    Ok(())
}

/// Validate retention policy settings
///
/// # Arguments
/// * `max_snapshots` - Maximum number of snapshots to keep (0 = unlimited)
/// * `max_age_days` - Maximum age in days (0 = unlimited)
/// * `min_snapshots` - Minimum snapshots to always keep
///
/// # Returns
/// `Ok(())` if valid, `Err(errors)` if invalid with all validation errors
pub fn validate_retention_policy(
    max_snapshots: usize,
    max_age_days: u32,
    min_snapshots: usize,
) -> ValidationResult {
    let mut errors = Vec::new();

    // max_snapshots validation
    if max_snapshots > 1000 {
        errors.push(ValidationError::new(
            "max_snapshots",
            "Maximum snapshots cannot exceed 1000 (to prevent filling disk)",
        ));
    }

    // max_age_days validation
    if max_age_days > 3650 {
        // 10 years
        errors.push(ValidationError::new(
            "max_age_days",
            "Maximum age cannot exceed 3650 days (10 years)",
        ));
    }

    // min_snapshots validation
    if min_snapshots > 100 {
        errors.push(ValidationError::new(
            "min_snapshots",
            "Minimum snapshots cannot exceed 100",
        ));
    }

    // Logical consistency check
    if max_snapshots > 0 && min_snapshots > max_snapshots {
        errors.push(ValidationError::new(
            "min_snapshots",
            format!(
                "Minimum snapshots ({min_snapshots}) cannot be greater than maximum snapshots ({max_snapshots})"
            ),
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validate path is safe and writable
///
/// Checks that a configuration path:
/// - Is an absolute path
/// - Doesn't contain path traversal attempts
/// - Parent directory is writable (if path doesn't exist)
pub fn validate_config_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Configuration path must be absolute".to_string());
    }

    // Check for path traversal
    let path_str = path.to_string_lossy();
    if path_str.contains("..") {
        return Err("Configuration path cannot contain '..'".to_string());
    }

    // If path exists, check if it's writable
    if path.exists() {
        if path.is_dir() {
            return Err("Configuration path cannot be a directory".to_string());
        }

        // Try to check permissions (simplified check)
        let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot access path: {e}"))?;

        if metadata.permissions().readonly() {
            return Err("Configuration file is read-only".to_string());
        }
    } else {
        // Check parent directory is writable
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err(format!(
                    "Parent directory does not exist: {}",
                    parent.display()
                ));
            }

            // Note: We can't easily check if directory is writable without trying to write
            // This is a basic check
        } else {
            return Err("Invalid path: no parent directory".to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_time_format_valid() {
        assert!(validate_time_format("00:00").is_ok());
        assert!(validate_time_format("12:30").is_ok());
        assert!(validate_time_format("23:59").is_ok());
        assert!(validate_time_format("02:00").is_ok());
    }

    #[test]
    fn test_validate_time_format_invalid_hours() {
        assert!(validate_time_format("24:00").is_err());
        assert!(validate_time_format("25:30").is_err());
    }

    #[test]
    fn test_validate_time_format_invalid_minutes() {
        assert!(validate_time_format("12:60").is_err());
        assert!(validate_time_format("12:99").is_err());
    }

    #[test]
    fn test_validate_time_format_wrong_format() {
        assert!(validate_time_format("2:00").is_err()); // Not zero-padded
        assert!(validate_time_format("12:5").is_err()); // Not zero-padded
        assert!(validate_time_format("12-30").is_err()); // Wrong separator
        assert!(validate_time_format("1200").is_err()); // No separator
        assert!(validate_time_format("12:30:00").is_err()); // Too many parts
    }

    #[test]
    fn test_validate_scheduler_frequency() {
        assert_eq!(validate_scheduler_frequency("hourly").unwrap(), 0);
        assert_eq!(validate_scheduler_frequency("daily").unwrap(), 1);
        assert_eq!(validate_scheduler_frequency("weekly").unwrap(), 2);
        assert_eq!(validate_scheduler_frequency("monthly").unwrap(), 3);
        assert_eq!(validate_scheduler_frequency("0").unwrap(), 0);
        assert_eq!(validate_scheduler_frequency("1").unwrap(), 1);

        assert!(validate_scheduler_frequency("invalid").is_err());
        assert!(validate_scheduler_frequency("4").is_err());
    }

    #[test]
    fn test_validate_day_of_week() {
        assert!(validate_day_of_week("0").is_ok()); // Sunday
        assert!(validate_day_of_week("6").is_ok()); // Saturday
        assert!(validate_day_of_week("3").is_ok()); // Wednesday

        assert!(validate_day_of_week("7").is_err());
        assert!(validate_day_of_week("10").is_err());
        assert!(validate_day_of_week("monday").is_err());
    }

    #[test]
    fn test_validate_snapshot_prefix() {
        assert!(validate_snapshot_prefix("auto").is_ok());
        assert!(validate_snapshot_prefix("backup").is_ok());
        assert!(validate_snapshot_prefix("pre-upgrade").is_ok());
        assert!(validate_snapshot_prefix("test_2025").is_ok());

        assert!(validate_snapshot_prefix("").is_err()); // Empty
        assert!(validate_snapshot_prefix("-prefix").is_err()); // Starts with dash
        assert!(validate_snapshot_prefix(".hidden").is_err()); // Starts with dot
        assert!(validate_snapshot_prefix("has space").is_err()); // Contains space
        assert!(validate_snapshot_prefix("has/slash").is_err()); // Contains slash
        assert!(validate_snapshot_prefix(&"a".repeat(51)).is_err()); // Too long
    }

    #[test]
    fn test_validate_retention_policy_valid() {
        assert!(validate_retention_policy(10, 30, 3).is_ok());
        assert!(validate_retention_policy(0, 0, 1).is_ok()); // Unlimited
        assert!(validate_retention_policy(100, 365, 5).is_ok());
    }

    #[test]
    fn test_validate_retention_policy_max_snapshots_too_high() {
        let result = validate_retention_policy(1001, 30, 3);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field == "max_snapshots"));
    }

    #[test]
    fn test_validate_retention_policy_max_age_too_high() {
        let result = validate_retention_policy(10, 3651, 3);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field == "max_age_days"));
    }

    #[test]
    fn test_validate_retention_policy_min_exceeds_max() {
        let result = validate_retention_policy(10, 30, 15);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.field == "min_snapshots"));
    }

    #[test]
    fn test_validate_retention_policy_multiple_errors() {
        let result = validate_retention_policy(1500, 5000, 2000);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.len() >= 2); // Multiple validation errors
    }

    #[test]
    fn test_validate_config_path_relative() {
        let path = Path::new("relative/path.conf");
        assert!(validate_config_path(path).is_err());
    }

    #[test]
    fn test_validate_config_path_traversal() {
        let path = Path::new("/etc/../etc/waypoint.conf");
        assert!(validate_config_path(path).is_err());
    }
}
