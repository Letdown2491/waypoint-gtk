//! Snapshot list display and filtering
//!
//! This module handles the display and filtering of snapshots in the main list view.

use gtk::prelude::*;
use gtk::{Button, Label, ListBox};
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

use crate::snapshot::SnapshotManager;
use super::snapshot_row::{SnapshotRow, SnapshotAction};

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
    list: &ListBox,
    compare_btn: &Button,
    search_text: Option<&str>,
    date_filter: Option<DateFilter>,
    match_label: Option<&Label>,
    action_handler: impl Fn(&str, SnapshotAction) + 'static + Clone,
) {
    // Clear existing items
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    // Load all snapshots
    let all_snapshots = match manager.borrow().load_snapshots() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load snapshots: {}", e);
            return;
        }
    };

    // Apply filters if provided
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
    if filtered_snapshots.is_empty() {
        let placeholder = adw::StatusPage::new();

        if all_snapshots.is_empty() {
            placeholder.set_title("No Restore Points");
            placeholder.set_description(Some("Restore points let you roll back your system to a previous state.\n\nClick \"Create Restore Point\" to save your current system state."));

            // Create custom icon with specific size
            let icon = gtk::Image::from_icon_name("document-save-symbolic");
            icon.set_pixel_size(64);
            placeholder.set_child(Some(&icon));
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
        for snapshot in filtered_snapshots.iter().rev() {
            let handler_clone = action_handler.clone();
            let row = SnapshotRow::new(snapshot, move |id, action| {
                handler_clone(&id, action);
            });
            list.append(&row);
        }
    }
}
