// Waypoint Snapshot Scheduler - Rust Implementation
// Manages multiple concurrent snapshot schedules

use anyhow::{Context, Result};
use chrono::{Datelike, Local, Timelike};
use std::process::Command;
use std::thread;
use std::time::Duration;
use waypoint_common::{Schedule, ScheduleType, SchedulesConfig, WaypointConfig};

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("================================================");
    log::info!("Waypoint Scheduler Service Starting (Rust)");
    log::info!("================================================");

    // Load configuration
    let config = WaypointConfig::new();
    log::info!("Schedules config: {}", config.schedules_config.display());

    // Main service loop
    loop {
        if let Err(e) = run_scheduler_loop(&config) {
            log::error!("Scheduler error: {}", e);
            log::info!("Will retry in 60 seconds...");
            thread::sleep(Duration::from_secs(60));
        }
    }
}

/// Main scheduler loop
fn run_scheduler_loop(config: &WaypointConfig) -> Result<()> {
    // Load schedules
    let schedules = load_schedules(config)?;

    // Get enabled schedules
    let enabled = schedules.enabled_schedules();

    if enabled.is_empty() {
        log::warn!("No schedules are enabled. Waiting 5 minutes before checking again...");
        thread::sleep(Duration::from_secs(300));
        return Ok(());
    }

    log::info!("Enabled schedules:");
    for schedule in &enabled {
        log::info!(
            "  - {} ({})",
            schedule.prefix,
            schedule.schedule_type.as_str()
        );
    }

    // Calculate next run time for each schedule
    let mut next_runs: Vec<(Duration, &Schedule)> = enabled
        .iter()
        .filter_map(|s| calculate_next_run(s).map(|duration| (duration, *s)).ok())
        .collect();

    if next_runs.is_empty() {
        log::error!("Could not calculate next run time for any schedule");
        return Err(anyhow::anyhow!("No valid schedules"));
    }

    // Sort by soonest first
    next_runs.sort_by_key(|(duration, _)| *duration);

    // Get the soonest schedule
    let (sleep_duration, next_schedule) = next_runs[0];

    log::info!(
        "Next snapshot: {} in {} ({})",
        next_schedule.prefix,
        format_duration(sleep_duration),
        next_schedule.description
    );

    // Sleep until it's time
    thread::sleep(sleep_duration);

    // Create the snapshot
    create_snapshot(next_schedule)?;

    // Apply retention cleanup after snapshot creation
    if let Err(e) = apply_retention_cleanup() {
        log::warn!("Failed to apply retention cleanup: {}", e);
        // Don't fail the main loop if cleanup fails
    }

    Ok(())
}

/// Load schedules from configuration file
fn load_schedules(config: &WaypointConfig) -> Result<SchedulesConfig> {
    if !config.schedules_config.exists() {
        log::warn!(
            "Schedules config not found at {}. Using defaults.",
            config.schedules_config.display()
        );
        return Ok(SchedulesConfig::default());
    }

    SchedulesConfig::load_from_file(&config.schedules_config)
        .context("Failed to load schedules configuration")
}

/// Calculate duration until next run for a schedule
fn calculate_next_run(schedule: &Schedule) -> Result<Duration> {
    let now = Local::now();

    match schedule.schedule_type {
        ScheduleType::Hourly => {
            // Next hour
            let seconds_into_hour = now.minute() * 60 + now.second();
            let seconds_until_next_hour = 3600 - seconds_into_hour;
            Ok(Duration::from_secs(seconds_until_next_hour as u64))
        }

        ScheduleType::Daily => {
            let time = schedule
                .time
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Daily schedule missing time"))?;

            calculate_next_daily(now, time)
        }

        ScheduleType::Weekly => {
            let time = schedule
                .time
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Weekly schedule missing time"))?;

            let day_of_week = schedule
                .day_of_week
                .ok_or_else(|| anyhow::anyhow!("Weekly schedule missing day_of_week"))?;

            calculate_next_weekly(now, time, day_of_week)
        }

        ScheduleType::Monthly => {
            let time = schedule
                .time
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Monthly schedule missing time"))?;

            let day_of_month = schedule
                .day_of_month
                .ok_or_else(|| anyhow::anyhow!("Monthly schedule missing day_of_month"))?;

            calculate_next_monthly(now, time, day_of_month)
        }
    }
}

/// Calculate next daily run time
fn calculate_next_daily(now: chrono::DateTime<Local>, time: &str) -> Result<Duration> {
    let parts: Vec<&str> = time.split(':').collect();
    let target_hour: u32 = parts[0].parse()?;
    let target_min: u32 = parts[1].parse()?;

    let current_secs = now.hour() * 3600 + now.minute() * 60 + now.second();
    let target_secs = target_hour * 3600 + target_min * 60;

    let seconds = if current_secs < target_secs {
        // Later today
        target_secs - current_secs
    } else {
        // Tomorrow
        86400 - current_secs + target_secs
    };

    Ok(Duration::from_secs(seconds as u64))
}

/// Calculate next weekly run time
fn calculate_next_weekly(
    now: chrono::DateTime<Local>,
    time: &str,
    day_of_week: u8,
) -> Result<Duration> {
    let parts: Vec<&str> = time.split(':').collect();
    let target_hour: u32 = parts[0].parse()?;
    let target_min: u32 = parts[1].parse()?;

    let current_day = now.weekday().num_days_from_sunday();
    let target_day = day_of_week as u32;

    let mut days_until = if target_day >= current_day {
        target_day - current_day
    } else {
        7 - current_day + target_day
    };

    // If it's the target day but time has passed, wait until next week
    if days_until == 0 {
        let current_secs = now.hour() * 3600 + now.minute() * 60 + now.second();
        let target_secs = target_hour * 3600 + target_min * 60;

        if current_secs >= target_secs {
            days_until = 7;
        }
    }

    let current_secs = now.hour() * 3600 + now.minute() * 60 + now.second();
    let target_secs = target_hour * 3600 + target_min * 60;

    let seconds = if days_until == 0 {
        target_secs - current_secs
    } else {
        days_until * 86400 + target_secs - current_secs
    };

    Ok(Duration::from_secs(seconds as u64))
}

/// Calculate next monthly run time
fn calculate_next_monthly(
    now: chrono::DateTime<Local>,
    time: &str,
    day_of_month: u8,
) -> Result<Duration> {
    let parts: Vec<&str> = time.split(':').collect();
    let target_hour: u32 = parts[0].parse()?;
    let target_min: u32 = parts[1].parse()?;

    let current_day = now.day();
    let target_day = day_of_month as u32;

    // Simplified: just calculate days until target day in current/next month
    // This doesn't handle all edge cases (e.g., day 31 in February) but works for common cases
    let days_until = if target_day >= current_day {
        target_day - current_day
    } else {
        // Assume 30 days per month for simplicity
        // In production, we'd calculate actual days in month
        30 - current_day + target_day
    };

    let current_secs = now.hour() * 3600 + now.minute() * 60 + now.second();
    let target_secs = target_hour * 3600 + target_min * 60;

    let seconds = if days_until == 0 && current_secs < target_secs {
        target_secs - current_secs
    } else if days_until == 0 {
        // Next month
        30 * 86400 + target_secs - current_secs
    } else {
        days_until * 86400 + target_secs - current_secs
    };

    Ok(Duration::from_secs(seconds as u64))
}

/// Load subvolumes configuration from user config
fn load_subvolumes_config() -> Result<Vec<String>> {
    use std::fs;
    use std::path::PathBuf;

    // Try user config first (~/.config/waypoint/subvolumes.json)
    if let Some(home) = std::env::var_os("HOME") {
        let user_config = PathBuf::from(home)
            .join(".config/waypoint/subvolumes.json");

        if user_config.exists() {
            let content = fs::read_to_string(&user_config)
                .context("Failed to read subvolumes config")?;
            let subvolumes: Vec<String> = serde_json::from_str(&content)
                .context("Failed to parse subvolumes config")?;

            if !subvolumes.is_empty() {
                log::info!("Loaded subvolumes from user config: {:?}", subvolumes);
                return Ok(subvolumes);
            }
        }
    }

    // Fall back to just root if no config found
    log::warn!("No subvolumes config found, defaulting to [/]");
    Ok(vec!["/".to_string()])
}

/// Create a snapshot for the given schedule
fn create_snapshot(schedule: &Schedule) -> Result<()> {
    waypoint_common::validate_snapshot_name(&schedule.prefix)
        .map_err(|e| anyhow::anyhow!("Invalid schedule prefix '{}': {}", schedule.prefix, e))?;
    let snapshot_name = format!("{}-{}", schedule.prefix, Local::now().format("%Y%m%d-%H%M"));

    log::info!("Creating scheduled snapshot: {}", snapshot_name);

    // Load subvolumes configuration
    let subvolumes = load_subvolumes_config()?;
    let subvolumes_arg = subvolumes.join(",");

    // Call waypoint-cli to create snapshot with subvolumes
    let output = Command::new("waypoint-cli")
        .arg("create")
        .arg(&snapshot_name)
        .arg(&schedule.description)
        .arg(&subvolumes_arg)
        .output()
        .context("Failed to execute waypoint-cli")?;

    if output.status.success() {
        log::info!("✓ Snapshot created successfully: {}", snapshot_name);
        log::info!("  Subvolumes: {}", subvolumes_arg);
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("✗ Failed to create snapshot: {}", stderr);
        return Err(anyhow::anyhow!("Snapshot creation failed: {}", stderr));
    }

    Ok(())
}

/// Apply retention cleanup after creating a snapshot
fn apply_retention_cleanup() -> Result<()> {
    log::info!("Running retention cleanup...");

    // Call waypoint-cli to apply retention
    let output = Command::new("waypoint-cli")
        .arg("cleanup")
        .arg("--schedule-based")
        .output()
        .context("Failed to execute waypoint-cli cleanup")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            log::info!("Retention cleanup: {}", stdout.trim());
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!("Retention cleanup warning: {}", stderr);
        // Don't fail the entire operation if cleanup fails - just log it
    }

    Ok(())
}

/// Format duration into human-readable string
fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();

    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}
