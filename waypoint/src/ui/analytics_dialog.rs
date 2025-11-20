//! Analytics dialog showing snapshot statistics and insights

use adw::prelude::*;
use chrono::Utc;
use gtk::prelude::*;
use gtk::{Label, Orientation};
use libadwaita as adw;

use crate::btrfs;
use crate::snapshot::{Snapshot, format_bytes};

/// Create empty state when no snapshots exist
fn create_empty_state() -> adw::StatusPage {
    let status_page = adw::StatusPage::new();
    status_page.set_title("No Snapshots Yet");
    status_page.set_description(Some(
        "Create your first snapshot to see analytics and insights about your system backups.",
    ));
    status_page.set_icon_name(Some("folder-symbolic"));
    status_page.set_vexpand(true);
    status_page
}

/// Persist calculated snapshot sizes back to metadata for future use
fn persist_snapshot_sizes(
    snapshots: &[Snapshot],
    sizes: &std::collections::HashMap<String, u64>,
    _snapshot_manager: &std::rc::Rc<std::cell::RefCell<crate::snapshot::SnapshotManager>>,
) {
    use crate::dbus_client::WaypointHelperClient;

    // Update any snapshots that didn't have size_bytes cached
    for snapshot in snapshots {
        if snapshot.size_bytes.is_none() {
            if let Some(&size) = sizes.get(&snapshot.name) {
                // Create updated snapshot with size
                let mut updated_snapshot = snapshot.clone();
                updated_snapshot.size_bytes = Some(size);

                // Save back to metadata via D-Bus (requires root permissions)
                match WaypointHelperClient::new() {
                    Ok(client) => {
                        if let Err(e) = client.update_snapshot_metadata(&updated_snapshot) {
                            log::warn!("Failed to persist size for snapshot {}: {}", snapshot.name, e);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to connect to waypoint-helper for snapshot {}: {}", snapshot.name, e);
                    }
                }
            }
        }
    }
}

/// Calculate all snapshot sizes once and store in a map
/// Uses parallel processing for significant speedup
fn calculate_all_sizes(snapshots: &[Snapshot]) -> std::collections::HashMap<String, u64> {
    use std::collections::HashMap;

    // First, check which snapshots already have cached sizes
    let mut sizes = HashMap::new();
    let mut paths_to_calculate = Vec::new();

    for snapshot in snapshots {
        if let Some(cached_size) = snapshot.size_bytes {
            // Use cached size from metadata
            sizes.insert(snapshot.name.clone(), cached_size);
        } else {
            // Need to calculate this one
            paths_to_calculate.push(snapshot.path.clone());
        }
    }

    // Calculate remaining sizes in parallel using the optimized bulk function
    if !paths_to_calculate.is_empty() {
        let calculated_sizes = btrfs::get_all_snapshot_sizes(&paths_to_calculate);

        // Map paths back to snapshot names
        for snapshot in snapshots {
            if !sizes.contains_key(&snapshot.name) {
                if let Some(&size) = calculated_sizes.get(&snapshot.path) {
                    sizes.insert(snapshot.name.clone(), size);
                }
            }
        }
    }

    sizes
}

/// Show analytics dialog with snapshot statistics
pub fn show_analytics_dialog(
    parent: &adw::ApplicationWindow,
    snapshots: &[Snapshot],
    snapshot_manager: &std::rc::Rc<std::cell::RefCell<crate::snapshot::SnapshotManager>>,
) {
    let dialog = adw::Window::new();
    dialog.set_title(Some("Analytics"));
    dialog.set_default_size(700, 650);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));

    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Analytics", "")));
    content.append(&header);

    // Check for empty state
    if snapshots.is_empty() {
        content.append(&create_empty_state());
        dialog.set_content(Some(&content));
        dialog.present();
        return;
    }

    // Scrolled window for content
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(800);
    clamp.set_tightening_threshold(600);

    // Calculate all snapshot sizes once (this is the optimization)
    let snapshot_sizes = calculate_all_sizes(snapshots);

    // Persist newly calculated sizes back to metadata for future use
    persist_snapshot_sizes(snapshots, &snapshot_sizes, snapshot_manager);

    // Calculate statistics using the pre-calculated sizes
    let stats = calculate_statistics_with_sizes(snapshots, &snapshot_sizes);

    // Build UI with all sections
    let main_box = gtk::Box::new(Orientation::Vertical, 0);
    main_box.set_margin_start(12);
    main_box.set_margin_end(12);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);

    // Overview section
    main_box.append(&create_overview_section(&stats));

    // Space usage section
    main_box.append(&create_space_section(&stats));

    // Insights and recommendations
    main_box.append(&create_insights_section(&stats, snapshots, &snapshot_sizes));

    // Largest snapshots section
    main_box.append(&create_largest_snapshots_section(
        snapshots,
        &snapshot_sizes,
        stats.total_size,
    ));

    clamp.set_child(Some(&main_box));
    scrolled.set_child(Some(&clamp));
    content.append(&scrolled);

    dialog.set_content(Some(&content));
    dialog.present();
}

/// Statistics calculated from snapshots
struct SnapshotStats {
    total_count: usize,
    total_size: u64,
    oldest_age_days: Option<i64>,
    newest_age_hours: Option<i64>,
    average_size: u64,
    growth_rate_per_week: Option<f64>,
}

/// Calculate statistics using pre-calculated sizes (optimized - no redundant btrfs calls)
fn calculate_statistics_with_sizes(
    snapshots: &[Snapshot],
    sizes: &std::collections::HashMap<String, u64>,
) -> SnapshotStats {
    let total_count = snapshots.len();

    // Calculate total size from pre-calculated map
    let total_size: u64 = sizes.values().sum();
    let counted = sizes.len();

    let average_size = if counted > 0 {
        total_size / counted as u64
    } else {
        0
    };

    // Find oldest and newest snapshots
    let now = Utc::now();
    let oldest_age_days = snapshots
        .iter()
        .map(|s| (now - s.timestamp).num_days())
        .max();

    let newest_age_hours = snapshots
        .iter()
        .map(|s| (now - s.timestamp).num_hours())
        .min();

    // Calculate growth rate (GB per week) using pre-calculated sizes
    let growth_rate_per_week = if snapshots.len() >= 2 {
        let mut sorted = snapshots.to_vec();
        sorted.sort_by_key(|s| s.timestamp);

        if let (Some(oldest), Some(newest)) = (sorted.first(), sorted.last()) {
            let oldest_size = sizes.get(&oldest.name).copied().unwrap_or(0);
            let newest_size = sizes.get(&newest.name).copied().unwrap_or(0);
            let time_diff_days = (newest.timestamp - oldest.timestamp).num_days();

            if time_diff_days > 0 && newest_size > oldest_size {
                let size_diff = (newest_size - oldest_size) as f64;
                let days = time_diff_days as f64;
                Some((size_diff / days) * 7.0) // Convert to per-week
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    SnapshotStats {
        total_count,
        total_size,
        oldest_age_days,
        newest_age_hours,
        average_size,
        growth_rate_per_week,
    }
}

/// Create overview section with basic stats
fn create_overview_section(stats: &SnapshotStats) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Overview");
    group.set_margin_bottom(18);

    // Total snapshots
    let total_row = adw::ActionRow::new();
    total_row.set_title("Total Snapshots");
    total_row.add_suffix(&create_stat_label(&stats.total_count.to_string()));
    group.add(&total_row);

    // Oldest snapshot
    if let Some(days) = stats.oldest_age_days {
        let oldest_row = adw::ActionRow::new();
        oldest_row.set_title("Oldest Snapshot");
        let age_text = if days == 0 {
            "Today".to_string()
        } else if days == 1 {
            "1 day ago".to_string()
        } else if days < 30 {
            format!("{days} days ago")
        } else if days < 365 {
            format!("{} months ago", days / 30)
        } else {
            format!("{} years ago", days / 365)
        };
        oldest_row.add_suffix(&create_stat_label(&age_text));
        group.add(&oldest_row);
    }

    // Newest snapshot
    if let Some(hours) = stats.newest_age_hours {
        let newest_row = adw::ActionRow::new();
        newest_row.set_title("Newest Snapshot");
        let age_text = if hours == 0 {
            "Just now".to_string()
        } else if hours < 24 {
            format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
        } else {
            format!(
                "{} day{} ago",
                hours / 24,
                if hours / 24 == 1 { "" } else { "s" }
            )
        };
        newest_row.add_suffix(&create_stat_label(&age_text));
        group.add(&newest_row);
    }

    // Average frequency
    if let Some(oldest_days) = stats.oldest_age_days {
        if oldest_days > 0 && stats.total_count > 1 {
            let freq_row = adw::ActionRow::new();
            freq_row.set_title("Snapshot Frequency");
            let per_day = stats.total_count as f64 / oldest_days as f64;
            let freq_text = if per_day >= 1.0 {
                format!("{per_day:.1} per day")
            } else {
                format!("1 per {:.0} days", 1.0 / per_day)
            };
            freq_row.add_suffix(&create_stat_label(&freq_text));
            group.add(&freq_row);
        }
    }

    group
}

/// Create space usage section
fn create_space_section(stats: &SnapshotStats) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Space Usage");
    group.set_margin_bottom(18);

    // Total space used
    let total_row = adw::ActionRow::new();
    total_row.set_title("Total Space Used");
    total_row.add_suffix(&create_stat_label(&format_bytes(stats.total_size)));
    group.add(&total_row);

    // Average snapshot size
    let avg_row = adw::ActionRow::new();
    avg_row.set_title("Average Snapshot Size");
    avg_row.add_suffix(&create_stat_label(&format_bytes(stats.average_size)));
    group.add(&avg_row);

    group
}

/// Create largest snapshots section with visual size indicators (optimized)
fn create_largest_snapshots_section(
    snapshots: &[Snapshot],
    sizes: &std::collections::HashMap<String, u64>,
    total_size: u64,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Largest Snapshots");
    group.set_description(Some("Top 5 snapshots consuming the most disk space"));

    // Build list with sizes from pre-calculated map
    let mut snapshots_with_sizes: Vec<(&Snapshot, u64)> = snapshots
        .iter()
        .filter_map(|s| {
            let size = sizes.get(&s.name).copied()?;
            Some((s, size))
        })
        .collect();

    // Sort by size and take top 5
    snapshots_with_sizes.sort_by(|a, b| b.1.cmp(&a.1));
    let top_5: Vec<_> = snapshots_with_sizes.iter().take(5).collect();

    if top_5.is_empty() {
        return group;
    }

    for (idx, (snapshot, size)) in top_5.iter().enumerate() {
        // Create ActionRow with custom content
        let row = adw::ActionRow::new();

        // Build title with rank
        let title_text = format!("#{} {}", idx + 1, snapshot.name);
        row.set_title(&title_text);

        // Build subtitle
        let subtitle = format!(
            "{} • {} packages",
            snapshot.format_timestamp(),
            snapshot.package_count.unwrap_or(0)
        );
        row.set_subtitle(&subtitle);

        // Size and percentage in a box
        let size_box = gtk::Box::new(Orientation::Vertical, 2);

        let size_label = Label::new(Some(&format_bytes(*size)));
        size_label.set_halign(gtk::Align::End);
        size_box.append(&size_label);

        // Add percentage of total
        let percentage = if total_size > 0 {
            (*size as f64 / total_size as f64 * 100.0) as u32
        } else {
            0
        };
        let pct_label = Label::new(Some(&format!("{percentage}%")));
        pct_label.add_css_class("caption");
        pct_label.add_css_class("dim-label");
        pct_label.set_halign(gtk::Align::End);
        size_box.append(&pct_label);

        row.add_suffix(&size_box);

        // Add progress bar as a separate widget below the row
        let container = gtk::Box::new(Orientation::Vertical, 0);

        // The row itself
        let row_container = gtk::Box::new(Orientation::Vertical, 6);
        row_container.append(&row);

        // Progress bar - shows size relative to total storage (matches percentage label)
        let progress_bar = gtk::ProgressBar::new();
        let fraction = if total_size > 0 {
            (*size as f64) / (total_size as f64)
        } else {
            0.0
        };
        progress_bar.set_fraction(fraction);
        progress_bar.set_show_text(false);
        progress_bar.set_margin_start(12);
        progress_bar.set_margin_end(12);
        progress_bar.set_margin_bottom(6);
        row_container.append(&progress_bar);

        container.append(&row_container);

        group.add(&container);
    }

    group
}

/// Create insights and recommendations section
fn create_insights_section(
    stats: &SnapshotStats,
    _snapshots: &[Snapshot],
    sizes: &std::collections::HashMap<String, u64>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Insights and Recommendations");
    group.set_margin_bottom(18);

    let mut insights = Vec::new();

    // Insight 1: Growth analysis (always show if we have data)
    if let Some(growth) = stats.growth_rate_per_week {
        let monthly_growth = growth * 4.3;
        let monthly_gb = monthly_growth / (1024.0 * 1024.0 * 1024.0);

        if monthly_gb > 10.0 {
            // High growth - warning
            insights.push((
                "High storage growth",
                format!("Snapshots growing at {}/week (≈{}/month). Monitor disk space and consider adjusting retention policy.",
                    format_bytes(growth as u64),
                    format_bytes(monthly_growth as u64)),
                "warning"
            ));
        } else if monthly_gb > 1.0 {
            // Moderate growth - informational
            insights.push((
                "Steady growth",
                format!(
                    "Snapshots growing at {}/week (≈{}/month). Current growth rate is sustainable.",
                    format_bytes(growth as u64),
                    format_bytes(monthly_growth as u64)
                ),
                "info",
            ));
        }
    } else if stats.total_count > 1 {
        // No growth or negative growth
        insights.push((
            "Stable storage usage",
            "Snapshot sizes are consistent or decreasing. Your system footprint is well-managed."
                .to_string(),
            "success",
        ));
    }

    // Insight 2: Snapshot count management
    if stats.total_count > 50 {
        insights.push((
            "Large snapshot count",
            format!("You have {} snapshots. Consider adjusting retention policy to automatically clean up old snapshots.", stats.total_count),
            "warning"
        ));
    } else if stats.total_count > 20 && stats.total_count <= 50 {
        insights.push((
            "Moderate snapshot count",
            format!(
                "{} snapshots stored. Your retention policy appears to be working well.",
                stats.total_count
            ),
            "info",
        ));
    } else if stats.total_count <= 5 {
        insights.push((
            "Few snapshots",
            format!(
                "Only {} snapshot{}. Consider enabling automated scheduling for regular backups.",
                stats.total_count,
                if stats.total_count == 1 { "" } else { "s" }
            ),
            "info",
        ));
    }

    // Insight 3: Size distribution
    let largest_size = sizes.values().copied().max().unwrap_or(0);

    if largest_size > 0 && stats.average_size > 0 {
        let ratio = largest_size as f64 / stats.average_size as f64;
        if ratio > 3.0 {
            insights.push((
                "Uneven snapshot sizes",
                format!("Some snapshots are {}x larger than average. Check largest snapshots below to identify candidates for deletion.", ratio as u32),
                "info"
            ));
        }
    }

    // Insight 4: Snapshot frequency
    if let Some(oldest_days) = stats.oldest_age_days {
        if oldest_days > 7 && stats.total_count > 1 {
            let per_day = stats.total_count as f64 / oldest_days as f64;
            if per_day < 0.2 {
                insights.push((
                    "Infrequent snapshots",
                    "Creating snapshots less than once per week. Enable automated scheduling for better system protection.".to_string(),
                    "info"
                ));
            } else if per_day > 3.0 {
                insights.push((
                    "Frequent snapshots",
                    format!("Creating snapshots {per_day:.1}x per day. Ensure this frequency aligns with your backup strategy."),
                    "info"
                ));
            }
        }
    }

    // Insight 5: Overall health status (only if no other insights)
    if insights.is_empty() {
        insights.push((
            "Everything looks good",
            "Your snapshot management is healthy. No issues detected.".to_string(),
            "success",
        ));
    }

    // Add all insights to the group
    for (title, description, _level) in insights {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(&description);
        row.set_title_lines(2);
        row.set_subtitle_lines(3);
        group.add(&row);
    }

    group
}

/// Create a styled stat label
fn create_stat_label(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_selectable(true);
    label
}
