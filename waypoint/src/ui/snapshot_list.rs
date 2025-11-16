//! Snapshot list display and filtering
//!
//! This module handles the display and filtering of snapshots in the main list view.

use gtk::prelude::*;
use gtk::{Button, Label, ListBox};
use libadwaita as adw;
use libadwaita::prelude::PreferencesRowExt;
use std::cell::RefCell;
use std::rc::Rc;

use crate::snapshot::SnapshotManager;
use crate::user_preferences::UserPreferencesManager;
use crate::backup_manager::BackupManager;
use crate::performance;
use super::snapshot_row::{SnapshotRow, SnapshotAction, BackupStatus};

/// Date filter options for snapshot list
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DateFilter {
    /// Show all snapshots regardless of age
    All,
    /// Show only snapshots from the last 7 days
    Last7Days,
    /// Show only snapshots from the last 30 days
    Last30Days,
    /// Show only snapshots from the last 90 days
    Last90Days,
}

/// Compute the backup status for a snapshot
fn compute_backup_status(snapshot_id: &str, backup_manager: &Rc<RefCell<BackupManager>>) -> BackupStatus {
    use waypoint_common::BackupStatus as ConfigStatus;

    let bm = backup_manager.borrow();
    let config = match bm.get_config() {
        Ok(c) => c,
        Err(_) => return BackupStatus::NotBackedUp,
    };

    // Count enabled destinations
    let enabled_count = config.enabled_destinations().count();
    if enabled_count == 0 {
        return BackupStatus::NotBackedUp;
    }

    // Use helper method to get backup destinations
    let backup_destinations = bm.get_snapshot_backup_destinations(snapshot_id);
    let backed_up_count = backup_destinations.len();

    // Check for pending and failed backups
    let mut pending_count = 0;
    let mut failed_count = 0;
    for pb in &config.pending_backups {
        if pb.snapshot_id == snapshot_id {
            match pb.status {
                ConfigStatus::Pending => pending_count += 1,
                ConfigStatus::Failed => failed_count += 1,
                _ => {}
            }
        }
    }

    // Determine status based on counts
    if failed_count > 0 {
        BackupStatus::Failed
    } else if pending_count > 0 {
        BackupStatus::Pending
    } else if backed_up_count == 0 {
        BackupStatus::NotBackedUp
    } else if backed_up_count >= enabled_count {
        BackupStatus::FullyBackedUp
    } else {
        BackupStatus::PartiallyBackedUp(backed_up_count, enabled_count)
    }
}

/// Refresh the snapshot list with optional filtering
///
/// This function loads all snapshots, applies optional text and date filters,
/// updates the UI to display matching snapshots, and configures the compare button state.
///
/// # Arguments
/// * `_window` - Parent application window (unused, kept for API compatibility)
/// * `manager` - Snapshot manager containing all snapshot data
/// * `list` - ListBox widget to populate with snapshot rows
/// * `compare_btn` - Compare button to enable/disable based on snapshot count
/// * `search_text` - Optional text filter to search snapshot names and descriptions
/// * `date_filter` - Optional date range filter
/// * `match_label` - Optional label to show "X of Y snapshots" count
/// * `action_handler` - Callback to handle snapshot actions (delete, restore, browse, etc.)
///
/// # Behavior
/// - Clears the existing list
/// - Loads snapshots from the manager
/// - Applies text filter (case-insensitive search in name/description)
/// - Applies date filter (age-based filtering)
/// - Updates match count label if provided
/// - Enables/disables compare button (requires ≥2 snapshots)
/// - Shows placeholder if no snapshots match
/// - Creates `SnapshotRow` widgets for each matching snapshot
pub fn refresh_snapshot_list_internal(
    _window: &adw::ApplicationWindow,
    manager: &Rc<RefCell<SnapshotManager>>,
    user_prefs_manager: &Rc<RefCell<UserPreferencesManager>>,
    backup_manager: &Rc<RefCell<BackupManager>>,
    list: &ListBox,
    compare_btn: &Button,
    search_text: Option<&str>,
    date_filter: Option<DateFilter>,
    match_label: Option<&Label>,
    action_handler: impl Fn(&str, SnapshotAction) + 'static + Clone,
    create_btn: Option<&Button>,
) {
    let _timer = performance::tracker().start("refresh_snapshot_list");

    // Clear existing items
    let _clear_timer = performance::tracker().start("clear_list_items");
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    drop(_clear_timer);

    // Load all snapshots
    let _load_timer = performance::tracker().start("load_snapshots");
    let all_snapshots = match manager.borrow().load_snapshots() {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to load snapshots: {}", e);
            return;
        }
    };
    drop(_load_timer);

    // Apply filters if provided
    let _filter_timer = performance::tracker().start("filter_snapshots");
    let filtered_snapshots: Vec<_> = if let (Some(search), Some(filter)) = (search_text, date_filter) {
        let search_lower = search.to_lowercase();
        let now = chrono::Utc::now();

        all_snapshots.iter().filter(|snapshot| {
            // Text filter
            let text_match = search.is_empty() ||
                snapshot.name.to_lowercase().contains(&search_lower) ||
                snapshot.description.as_ref()
                    .map(|d| d.to_lowercase().contains(&search_lower))
                    .unwrap_or(false);

            // Date filter
            let age_days = now.signed_duration_since(snapshot.timestamp).num_days();
            let date_match = match filter {
                DateFilter::All => true,
                DateFilter::Last7Days => age_days <= 7,
                DateFilter::Last30Days => age_days <= 30,
                DateFilter::Last90Days => age_days <= 90,
            };

            text_match && date_match
        }).collect()
    } else {
        // No filtering, use all snapshots
        all_snapshots.iter().collect()
    };
    drop(_filter_timer);

    // Update match count label if provided
    if let Some(label) = match_label {
        let is_filtered = search_text.map(|s| !s.is_empty()).unwrap_or(false) ||
            date_filter.map(|f| f != DateFilter::All).unwrap_or(false);

        if is_filtered {
            label.set_text(&format!("Showing {} of {} snapshots",
                filtered_snapshots.len(), all_snapshots.len()));
        } else {
            label.set_text(&format!("{} snapshots", all_snapshots.len()));
        }
    }

    // Update compare button state
    if filtered_snapshots.len() < 2 {
        compare_btn.set_sensitive(false);
        compare_btn.set_tooltip_text(Some("At least 2 snapshots needed to compare"));
    } else {
        compare_btn.set_sensitive(true);
        compare_btn.set_tooltip_text(Some("Compare packages between snapshots"));
    }

    // Display snapshots or placeholder
    let _ui_timer = performance::tracker().start("populate_ui");
    if filtered_snapshots.is_empty() {
        let placeholder = adw::StatusPage::new();

        if all_snapshots.is_empty() {
            placeholder.set_title("No Restore Points Yet");
            placeholder.set_description(Some("Restore points let you roll back your system to a previous state"));
            placeholder.set_icon_name(Some("waypoint"));

            // Add prominent "Create Restore Point" button if create_btn is provided
            if let Some(main_create_btn) = create_btn {
                let create_button = gtk::Button::with_label("Create Your First Restore Point");
                create_button.add_css_class("pill");
                create_button.add_css_class("suggested-action");

                // Wire up to activate the main create button
                let main_btn_clone = main_create_btn.clone();
                create_button.connect_clicked(move |_| {
                    main_btn_clone.emit_clicked();
                });

                placeholder.set_child(Some(&create_button));
            }
        } else {
            placeholder.set_title("No Matching Snapshots");
            placeholder.set_description(Some("No snapshots match your search criteria.\n\nTry adjusting your search or filter settings."));

            // Create custom icon with specific size
            let icon = gtk::Image::from_icon_name("edit-find-symbolic");
            icon.set_pixel_size(64);
            placeholder.set_child(Some(&icon));
        }
        placeholder.set_vexpand(true);

        list.append(&placeholder);
    } else {
        // Calculate max size for relative sizing of level bars
        let max_size = filtered_snapshots
            .iter()
            .filter_map(|s| s.size_bytes)
            .max();

        // Load user preferences
        let user_prefs = user_prefs_manager.borrow().load().unwrap_or_default();

        // Separate pinned and non-pinned snapshots based on user preferences
        let (pinned, regular): (Vec<_>, Vec<_>) = filtered_snapshots
            .into_iter()
            .partition(|s| {
                user_prefs.get(&s.id)
                    .map(|p| p.is_favorite)
                    .unwrap_or(false)
            });

        // Add pinned snapshots section if any exist
        if !pinned.is_empty() {
            // Add section header for pinned snapshots
            let pinned_header = adw::ActionRow::new();
            pinned_header.set_title("Pinned Restore Points");
            pinned_header.add_css_class("header-row");
            pinned_header.set_activatable(false);
            list.append(&pinned_header);

            // Add pinned snapshots (most recent first)
            for snapshot in pinned.iter().rev() {
                let prefs = user_prefs.get(&snapshot.id).cloned().unwrap_or_default();
                let backup_status = compute_backup_status(&snapshot.id, backup_manager);
                let handler_clone = action_handler.clone();
                let row = SnapshotRow::new_with_context(snapshot, &prefs, move |id, action| {
                    handler_clone(&id, action);
                }, max_size, &backup_status);
                list.append(&row);
            }

            // Add section header for regular snapshots if any exist
            if !regular.is_empty() {
                let regular_header = adw::ActionRow::new();
                regular_header.set_title("All Restore Points");
                regular_header.add_css_class("header-row");
                regular_header.set_activatable(false);
                regular_header.set_margin_top(12);
                list.append(&regular_header);
            }
        }

        // Add regular snapshots (most recent first)
        // Note: action_handler is cloned for each row, but it's a closure which is relatively
        // lightweight. The Snapshot references passed to SnapshotRow::new use Rc<T> internally
        // for expensive fields (packages, subvolumes), so cloning snapshots is cheap.
        for snapshot in regular.iter().rev() {
            let prefs = user_prefs.get(&snapshot.id).cloned().unwrap_or_default();
            let backup_status = compute_backup_status(&snapshot.id, backup_manager);
            let handler_clone = action_handler.clone();
            let row = SnapshotRow::new_with_context(snapshot, &prefs, move |id, action| {
                handler_clone(&id, action);
            }, max_size, &backup_status);
            list.append(&row);
        }
    }
    drop(_ui_timer);

    // Log performance statistics at debug level
    performance::log_stats();
}
