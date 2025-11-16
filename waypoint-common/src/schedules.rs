// Snapshot schedule configuration with TOML support

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of snapshot schedule
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleType {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

impl ScheduleType {
    pub fn as_str(&self) -> &str {
        match self {
            ScheduleType::Hourly => "hourly",
            ScheduleType::Daily => "daily",
            ScheduleType::Weekly => "weekly",
            ScheduleType::Monthly => "monthly",
        }
    }
}

/// A single snapshot schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    /// Whether this schedule is enabled
    pub enabled: bool,

    /// Type of schedule (hourly, daily, weekly, monthly)
    #[serde(rename = "type")]
    pub schedule_type: ScheduleType,

    /// Time of day for daily/weekly/monthly schedules (HH:MM format)
    /// Only used for daily, weekly, and monthly schedules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,

    /// Day of week for weekly schedules (0-6, where 0=Sunday)
    /// Only used for weekly schedules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_of_week: Option<u8>,

    /// Day of month for monthly schedules (1-31)
    /// Only used for monthly schedules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_of_month: Option<u8>,

    /// Snapshot name prefix (e.g., "hourly", "daily")
    pub prefix: String,

    /// Description for snapshots created by this schedule
    pub description: String,

    /// Maximum number of snapshots to keep for this schedule
    pub keep_count: u32,

    /// Maximum age in days for snapshots from this schedule
    pub keep_days: u32,
}

impl Schedule {
    /// Create a default hourly schedule (disabled)
    pub fn default_hourly() -> Self {
        Self {
            enabled: false,
            schedule_type: ScheduleType::Hourly,
            time: None,
            day_of_week: None,
            day_of_month: None,
            prefix: "hourly".to_string(),
            description: "Hourly snapshot".to_string(),
            keep_count: 24,
            keep_days: 1,
        }
    }

    /// Create a default daily schedule (enabled)
    pub fn default_daily() -> Self {
        Self {
            enabled: true,
            schedule_type: ScheduleType::Daily,
            time: Some("02:00".to_string()),
            day_of_week: None,
            day_of_month: None,
            prefix: "daily".to_string(),
            description: "Daily snapshot".to_string(),
            keep_count: 7,
            keep_days: 7,
        }
    }

    /// Create a default weekly schedule (disabled)
    pub fn default_weekly() -> Self {
        Self {
            enabled: false,
            schedule_type: ScheduleType::Weekly,
            time: Some("03:00".to_string()),
            day_of_week: Some(0), // Sunday
            day_of_month: None,
            prefix: "weekly".to_string(),
            description: "Weekly snapshot".to_string(),
            keep_count: 4,
            keep_days: 28,
        }
    }

    /// Create a default monthly schedule (disabled)
    pub fn default_monthly() -> Self {
        Self {
            enabled: false,
            schedule_type: ScheduleType::Monthly,
            time: Some("04:00".to_string()),
            day_of_week: None,
            day_of_month: Some(1), // First of month
            prefix: "monthly".to_string(),
            description: "Monthly snapshot".to_string(),
            keep_count: 3,
            keep_days: 90,
        }
    }

    /// Validate this schedule configuration
    pub fn validate(&self) -> Result<(), String> {
        // Validate time format if present
        if let Some(ref time) = self.time {
            if !is_valid_time_format(time) {
                return Err(format!(
                    "Invalid time format '{}'. Expected HH:MM (24-hour)",
                    time
                ));
            }
        }

        // Validate day_of_week if present
        if let Some(day) = self.day_of_week {
            if day > 6 {
                return Err(format!(
                    "Invalid day_of_week {}. Must be 0-6 (0=Sunday)",
                    day
                ));
            }
        }

        // Validate day_of_month if present
        if let Some(day) = self.day_of_month {
            if day < 1 || day > 31 {
                return Err(format!("Invalid day_of_month {}. Must be 1-31", day));
            }
        }

        // Type-specific validations
        match self.schedule_type {
            ScheduleType::Hourly => {
                // Hourly doesn't need time/day
            }
            ScheduleType::Daily => {
                if self.time.is_none() {
                    return Err("Daily schedule requires 'time' field".to_string());
                }
            }
            ScheduleType::Weekly => {
                if self.time.is_none() {
                    return Err("Weekly schedule requires 'time' field".to_string());
                }
                if self.day_of_week.is_none() {
                    return Err("Weekly schedule requires 'day_of_week' field".to_string());
                }
            }
            ScheduleType::Monthly => {
                if self.time.is_none() {
                    return Err("Monthly schedule requires 'time' field".to_string());
                }
                if self.day_of_month.is_none() {
                    return Err("Monthly schedule requires 'day_of_month' field".to_string());
                }
            }
        }

        Ok(())
    }
}

/// Container for all snapshot schedules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulesConfig {
    #[serde(rename = "schedule")]
    pub schedules: Vec<Schedule>,
}

impl Default for SchedulesConfig {
    fn default() -> Self {
        Self {
            schedules: vec![
                Schedule::default_hourly(),
                Schedule::default_daily(),
                Schedule::default_weekly(),
                Schedule::default_monthly(),
            ],
        }
    }
}

impl SchedulesConfig {
    /// Load schedules from a TOML file
    pub fn load_from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: SchedulesConfig = toml::from_str(&content)?;

        // Validate all schedules
        for schedule in &config.schedules {
            schedule.validate().map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(config)
    }

    /// Save schedules to a TOML file
    pub fn save_to_file(&self, path: &PathBuf) -> anyhow::Result<()> {
        // Validate all schedules before saving
        for schedule in &self.schedules {
            schedule.validate().map_err(|e| anyhow::anyhow!(e))?;
        }

        let content = toml::to_string_pretty(self)?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get all enabled schedules
    pub fn enabled_schedules(&self) -> Vec<&Schedule> {
        self.schedules.iter().filter(|s| s.enabled).collect()
    }

    /// Get schedule by type
    pub fn get_schedule(&self, schedule_type: ScheduleType) -> Option<&Schedule> {
        self.schedules
            .iter()
            .find(|s| s.schedule_type == schedule_type)
    }

    /// Get mutable schedule by type
    pub fn get_schedule_mut(&mut self, schedule_type: ScheduleType) -> Option<&mut Schedule> {
        self.schedules
            .iter_mut()
            .find(|s| s.schedule_type == schedule_type)
    }
}

/// Validate time format (HH:MM in 24-hour format)
fn is_valid_time_format(time: &str) -> bool {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 2 {
        return false;
    }

    let hour: Result<u8, _> = parts[0].parse();
    let minute: Result<u8, _> = parts[1].parse();

    match (hour, minute) {
        (Ok(h), Ok(m)) => h < 24 && m < 60,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_schedules() {
        let config = SchedulesConfig::default();
        assert_eq!(config.schedules.len(), 4);

        // Daily should be enabled by default
        let daily = config.get_schedule(ScheduleType::Daily).unwrap();
        assert!(daily.enabled);
        assert_eq!(daily.prefix, "daily");

        // Others should be disabled
        let hourly = config.get_schedule(ScheduleType::Hourly).unwrap();
        assert!(!hourly.enabled);
    }

    #[test]
    fn test_time_validation() {
        assert!(is_valid_time_format("00:00"));
        assert!(is_valid_time_format("12:30"));
        assert!(is_valid_time_format("23:59"));
        assert!(!is_valid_time_format("24:00"));
        assert!(!is_valid_time_format("12:60"));
        assert!(!is_valid_time_format("12"));
        assert!(!is_valid_time_format("12:30:00"));
    }

    #[test]
    fn test_schedule_validation() {
        let mut schedule = Schedule::default_daily();
        assert!(schedule.validate().is_ok());

        // Invalid time
        schedule.time = Some("25:00".to_string());
        assert!(schedule.validate().is_err());

        // Missing required time for daily
        schedule.time = None;
        assert!(schedule.validate().is_err());
    }

    #[test]
    fn test_toml_serialization() {
        let config = SchedulesConfig::default();
        let toml = toml::to_string(&config).unwrap();

        assert!(toml.contains("[[schedule]]"));
        assert!(toml.contains("type = \"daily\""));
        assert!(toml.contains("enabled = true"));
    }

    #[test]
    fn test_enabled_schedules() {
        let config = SchedulesConfig::default();
        let enabled = config.enabled_schedules();

        // Only daily is enabled by default
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].schedule_type, ScheduleType::Daily);
    }
}
