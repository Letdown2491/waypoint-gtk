//! Backup management UI
//!
//! Provides interface for backing up snapshots to external drives

use adw::prelude::*;
use gtk::prelude::*;
use gtk::{Button, Label, Orientation, Widget};
use libadwaita as adw;
use serde::{Deserialize, Serialize};

use super::dialogs;
use crate::dbus_client::WaypointHelperClient;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum DriveType {
    Removable,
    Network,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupDestination {
    mount_point: String,
    label: String,
    drive_type: DriveType,
    uuid: Option<String>,
    fstype: String, // Filesystem type (btrfs, ntfs, exfat, etc.)
}

use crate::backup_manager::BackupManager;
use std::cell::RefCell;
use std::rc::Rc;

/// Create the backups content page
pub fn create_backups_content(
    parent: &adw::ApplicationWindow,
    backup_manager: Rc<RefCell<BackupManager>>,
) -> gtk::Box {
    let container = gtk::Box::new(Orientation::Vertical, 0);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(800);
    clamp.set_tightening_threshold(600);

    let content_box = gtk::Box::new(Orientation::Vertical, 0);
    content_box.set_margin_top(24);
    content_box.set_margin_bottom(24);
    content_box.set_margin_start(12);
    content_box.set_margin_end(12);

    // Header section
    let header_group = adw::PreferencesGroup::new();
    header_group.set_title("Snapshot Backups");
    header_group.set_description(Some(
        "Create backups of your snapshots to external drives. Btrfs drives support efficient incremental backups. \
         Other filesystems (NTFS, exFAT, etc.) are also supported using full copy backups.",
    ));

    content_box.append(&header_group);

    // Destinations section
    let dest_group = adw::PreferencesGroup::new();
    dest_group.set_title("Backup Destinations");
    dest_group.set_description(Some("Available external drives for backups"));
    dest_group.set_margin_top(18);

    // Bulk actions row
    let bulk_actions_row = adw::ActionRow::new();
    bulk_actions_row.set_title("Bulk Actions");
    bulk_actions_row.set_subtitle("Quickly enable or disable all destinations");

    let enable_all_btn = Button::with_label("Enable All");
    enable_all_btn.set_valign(gtk::Align::Center);
    enable_all_btn.add_css_class("flat");

    let disable_all_btn = Button::with_label("Disable All");
    disable_all_btn.set_valign(gtk::Align::Center);
    disable_all_btn.add_css_class("flat");

    bulk_actions_row.add_suffix(&enable_all_btn);
    bulk_actions_row.add_suffix(&disable_all_btn);
    dest_group.add(&bulk_actions_row);

    // Destinations list container
    let destinations_list = adw::PreferencesGroup::new();
    destinations_list.set_visible(true); // Show by default
    let destination_rows: Rc<RefCell<Vec<Widget>>> = Rc::new(RefCell::new(Vec::new()));

    // Scan button section (separate group below destinations)
    let scan_group = adw::PreferencesGroup::new();
    scan_group.set_margin_top(12);

    let scan_row = adw::ActionRow::new();
    scan_row.set_title("Scan for Destinations");
    scan_row.set_subtitle("Detect available external drives");

    let scan_button = Button::with_label("Scan");
    scan_button.set_valign(gtk::Align::Center);
    scan_button.add_css_class("suggested-action");
    scan_row.add_suffix(&scan_button);

    scan_group.add(&scan_row);

    // Helper function to perform scan
    let perform_scan = |btn: Option<&Button>,
                        dest_list: adw::PreferencesGroup,
                        parent_ref: adw::ApplicationWindow,
                        bm_clone: Rc<RefCell<BackupManager>>,
                        rows_store: Rc<RefCell<Vec<Widget>>>| {

        if let Some(button) = btn {
            button.set_sensitive(false);
            button.set_label("Scanning...");
        }

        let btn_opt = btn.map(|b| b.clone());

        // Use thread + channel pattern instead of tokio
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = scan_destinations();
            let _ = tx.send(result);
        });

        // Poll for result
        let rows_store_clone = rows_store.clone();
        gtk::glib::spawn_future_local(async move {
            let result = loop {
                match rx.try_recv() {
                    Ok(result) => break result,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        gtk::glib::timeout_future(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        dialogs::show_error(
                            &parent_ref,
                            "Scan Failed",
                            "Scan thread disconnected unexpectedly",
                        );
                        if let Some(btn) = btn_opt {
                            btn.set_sensitive(true);
                            btn.set_label("Scan");
                        }
                        return;
                    }
                }
            };

            if let Some(btn) = btn_opt.as_ref() {
                btn.set_sensitive(true);
                btn.set_label("Scan");
            }

            // Clear existing destinations
            {
                let mut existing_rows = rows_store_clone.borrow_mut();
                for row in existing_rows.drain(..) {
                    dest_list.remove(&row);
                }
            }

            match result {
                Ok(scanned_destinations) => {
                    // Load saved destinations from config
                    let saved_config = bm_clone.borrow().get_config().unwrap_or_default();

                    // Build merged list: prioritize scanned (currently mounted) destinations,
                    // then add saved destinations that aren't currently mounted
                    let mut merged_destinations: Vec<BackupDestination> = Vec::new();
                    let mut seen_uuids = std::collections::HashSet::new();

                    // Add scanned destinations
                    for dest in scanned_destinations {
                        if let Some(ref uuid) = dest.uuid {
                            seen_uuids.insert(uuid.clone());
                        }
                        merged_destinations.push(dest);
                    }

                    // Add saved destinations that aren't currently mounted
                    for (uuid, config) in saved_config.destinations.iter() {
                        if !seen_uuids.contains(uuid) {
                            // This is a saved destination that's not currently mounted
                            // Create a "disconnected" destination entry
                            merged_destinations.push(BackupDestination {
                                mount_point: format!("{} (not connected)", config.last_mount_point),
                                label: config.label.clone(),
                                drive_type: DriveType::Removable, // Default to removable
                                uuid: Some(uuid.clone()),
                                fstype: config.fstype.clone(),
                            });
                        }
                    }

                    if merged_destinations.is_empty() {
                        // Enhanced empty state with actionable guidance
                        let empty_status = adw::StatusPage::new();
                        empty_status.set_title("No Backup Drives Found");
                        empty_status.set_description(Some(
                            "To get started with backups:\n\n\
                             1. Connect an external drive (USB, network, or internal)\n\
                             2. Format it with btrfs for incremental backups (recommended)\n\
                             3. Or use NTFS/exFAT for compatibility with other systems\n\
                             4. Click 'Scan' below to detect the drive"
                        ));
                        empty_status.set_icon_name(Some("drive-harddisk-symbolic"));

                        dest_list.add(&empty_status);
                        rows_store_clone
                            .borrow_mut()
                            .push(empty_status.clone().upcast::<Widget>());
                    } else {
                        for dest in &merged_destinations {
                            // Update last_mount_point if drive is connected at a new location
                            if let Some(ref uuid) = dest.uuid {
                                let is_connected = !dest.mount_point.contains("(not connected)");
                                if is_connected {
                                    if let Some(saved_dest) = saved_config.get_destination(uuid) {
                                        // If mount point changed, update the saved config
                                        if saved_dest.last_mount_point != dest.mount_point {
                                            log::info!(
                                                "Drive {} moved from {} to {}",
                                                dest.label,
                                                saved_dest.last_mount_point,
                                                dest.mount_point
                                            );

                                            // Update the mount point in config
                                            let mut updated_config = saved_dest.clone();
                                            updated_config.last_mount_point = dest.mount_point.clone();
                                            let _ = bm_clone.borrow().add_destination(uuid.clone(), updated_config);
                                        }
                                    }
                                }
                            }

                            let row = create_destination_row(dest, &parent_ref, bm_clone.clone());
                            dest_list.add(&row);
                            rows_store_clone
                                .borrow_mut()
                                .push(row.clone().upcast::<Widget>());
                        }
                    }

                    dest_list.set_visible(true);
                }
                Err(e) => {
                    dialogs::show_error(
                        &parent_ref,
                        "Scan Failed",
                        &format!("Failed to scan for destinations: {}", e),
                    );
                }
            }
        });
    };

    // Auto-scan on page load
    perform_scan(
        None,
        destinations_list.clone(),
        parent.clone(),
        backup_manager.clone(),
        destination_rows.clone(),
    );

    // Smart refresh: Auto-refresh when drives are mounted/unmounted
    // Check every 5 seconds for mount changes
    let dest_list_refresh = destinations_list.clone();
    let parent_refresh = parent.clone();
    let backup_manager_refresh = backup_manager.clone();
    let rows_refresh = destination_rows.clone();

    // Store last known mount state to detect changes
    let last_mount_state: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let last_mount_state_clone = last_mount_state.clone();

    gtk::glib::timeout_add_seconds_local(5, move || {
        // Get current mount state
        let current_mounts = match get_current_mounts() {
            Ok(mounts) => mounts,
            Err(_) => return gtk::glib::ControlFlow::Continue,
        };

        // Check if mounts changed
        let mounts_changed = {
            let last = last_mount_state_clone.borrow();
            *last != current_mounts
        };

        if mounts_changed {
            log::info!("Mount state changed, refreshing backup destinations");
            *last_mount_state_clone.borrow_mut() = current_mounts;

            // Trigger refresh
            perform_scan(
                None,
                dest_list_refresh.clone(),
                parent_refresh.clone(),
                backup_manager_refresh.clone(),
                rows_refresh.clone(),
            );
        }

        gtk::glib::ControlFlow::Continue
    });

    // Wire up manual scan button
    let dest_list_clone = destinations_list.clone();
    let parent_clone = parent.clone();
    let backup_manager_clone = backup_manager.clone();
    let rows_clone = destination_rows.clone();

    scan_button.connect_clicked(move |btn| {
        perform_scan(
            Some(btn),
            dest_list_clone.clone(),
            parent_clone.clone(),
            backup_manager_clone.clone(),
            rows_clone.clone(),
        );
    });

    // Wire up bulk action buttons
    let bm_enable_all = backup_manager.clone();
    let parent_enable = parent.clone();
    let dest_list_enable = destinations_list.clone();
    let rows_enable = destination_rows.clone();
    enable_all_btn.connect_clicked(move |_| {
        let config = match bm_enable_all.borrow().get_config() {
            Ok(c) => c,
            Err(_) => return,
        };

        for (uuid, mut dest_config) in config.destinations {
            dest_config.enabled = true;
            let _ = bm_enable_all.borrow().add_destination(uuid, dest_config);
        }

        // Refresh the list
        perform_scan(
            None,
            dest_list_enable.clone(),
            parent_enable.clone(),
            bm_enable_all.clone(),
            rows_enable.clone(),
        );

        dialogs::show_toast(&parent_enable, "All destinations enabled");
    });

    let bm_disable_all = backup_manager.clone();
    let parent_disable = parent.clone();
    let dest_list_disable = destinations_list.clone();
    let rows_disable = destination_rows.clone();
    disable_all_btn.connect_clicked(move |_| {
        let config = match bm_disable_all.borrow().get_config() {
            Ok(c) => c,
            Err(_) => return,
        };

        for (uuid, mut dest_config) in config.destinations {
            dest_config.enabled = false;
            let _ = bm_disable_all.borrow().add_destination(uuid, dest_config);
        }

        // Refresh the list
        perform_scan(
            None,
            dest_list_disable.clone(),
            parent_disable.clone(),
            bm_disable_all.clone(),
            rows_disable.clone(),
        );

        dialogs::show_toast(&parent_disable, "All destinations disabled");
    });

    content_box.append(&dest_group);
    content_box.append(&destinations_list);
    content_box.append(&scan_group);

    // Pending backups section
    let pending_group = adw::PreferencesGroup::new();
    pending_group.set_title("Pending Backups");
    pending_group.set_margin_top(18);

    // Status summary row
    let status_row = adw::ActionRow::new();
    status_row.set_title("Queue Status");
    update_backup_status_summary(&status_row, backup_manager.clone());
    pending_group.add(&status_row);

    // Create pending backups list container
    let pending_container = gtk::Box::new(Orientation::Vertical, 6);
    let pending_list = create_pending_backups_list(backup_manager.clone());
    pending_container.append(&pending_list);

    pending_group.add(&pending_container);

    // Set up auto-refresh timer (every 5 seconds)
    let bm_timer = backup_manager.clone();
    let container_timer = pending_container.clone();
    let status_timer = status_row.clone();
    gtk::glib::timeout_add_seconds_local(5, move || {
        // Update status summary
        update_backup_status_summary(&status_timer, bm_timer.clone());

        // Update pending list
        while let Some(child) = container_timer.first_child() {
            container_timer.remove(&child);
        }
        let new_list = create_pending_backups_list(bm_timer.clone());
        container_timer.append(&new_list);

        gtk::glib::ControlFlow::Continue
    });

    content_box.append(&pending_group);

    // Note: Removed global "Backup Statistics" and "Recent Backups" sections
    // These only showed backups tracked in config history, not actual backups on drives
    // Backup counts are now shown per-drive in the Drive Health section

    // Settings section
    let settings_group = adw::PreferencesGroup::new();
    settings_group.set_title("Backup Settings");
    settings_group.set_margin_top(18);

    // Mount check interval setting
    let interval_row = adw::ActionRow::new();
    interval_row.set_title("Mount Check Interval");
    interval_row.set_subtitle("How often to check for newly mounted backup drives (in seconds)");

    let current_interval = backup_manager
        .borrow()
        .get_config()
        .map(|c| c.mount_check_interval_seconds)
        .unwrap_or(60);

    let interval_spin = gtk::SpinButton::with_range(15.0, 300.0, 15.0);
    interval_spin.set_value(current_interval as f64);
    interval_spin.set_valign(gtk::Align::Center);

    let bm_interval = backup_manager.clone();
    interval_spin.connect_value_changed(move |spin| {
        let new_value = spin.value() as u64;
        if let Err(e) = bm_interval.borrow().set_mount_check_interval(new_value) {
            log::error!("Failed to save mount check interval: {}", e);
        } else {
            log::info!(
                "Updated mount check interval to {} seconds (requires restart to take effect)",
                new_value
            );
        }
    });

    interval_row.add_suffix(&interval_spin);
    settings_group.add(&interval_row);

    content_box.append(&settings_group);

    // Removed "Backup Statistics" and "Recent Backups" sections
    // These only tracked backups created through Waypoint (stored in config.backup_history)
    // Drive-specific backup counts are shown in the Drive Health section of each destination

    clamp.set_child(Some(&content_box));
    scrolled.set_child(Some(&clamp));
    container.append(&scrolled);

    container
}

/// Scan for available backup destinations
fn scan_destinations() -> anyhow::Result<Vec<BackupDestination>> {
    let client = WaypointHelperClient::new()?;
    let (success, result) = client.scan_backup_destinations()?;

    if !success {
        return Err(anyhow::anyhow!(result));
    }

    // Parse JSON response
    let destinations: Vec<BackupDestination> = serde_json::from_str(&result)?;
    Ok(destinations)
}

/// Get current mount points (for detecting mount/unmount events)
fn get_current_mounts() -> anyhow::Result<Vec<String>> {
    use std::fs;

    let mounts_content = fs::read_to_string("/proc/mounts")?;
    let mount_points: Vec<String> = mounts_content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let mount_point = parts[1];
                // Filter relevant mounts (skip system mounts)
                if mount_point.starts_with("/run/media/")
                    || mount_point.starts_with("/media/")
                    || mount_point.starts_with("/mnt/") {
                    return Some(mount_point.to_string());
                }
            }
            None
        })
        .collect();

    Ok(mount_points)
}

/// Create a row for a backup destination
fn create_destination_row(
    dest: &BackupDestination,
    parent: &adw::ApplicationWindow,
    backup_manager: Rc<RefCell<BackupManager>>,
) -> adw::ExpanderRow {
    use waypoint_common::{BackupDestinationConfig, BackupFilter};

    let row = adw::ExpanderRow::new();

    // Check if drive is currently connected
    let is_connected = !dest.mount_point.contains("(not connected)");

    // Get display name (with nickname if available)
    let display_name = if let Some(ref uuid_val) = dest.uuid {
        let config = backup_manager.borrow().get_config().unwrap_or_default();
        config.destinations
            .get(uuid_val)
            .map(|d| d.display_name().to_string())
            .unwrap_or_else(|| dest.label.clone())
    } else {
        dest.label.clone()
    };

    // Add drive type badge to title
    let type_badge = match dest.drive_type {
        DriveType::Removable => "USB",
        DriveType::Network => "Network",
        DriveType::Internal => "Internal",
    };

    row.set_title(&display_name);

    // Build comprehensive subtitle with status, type, backup count, and pending
    // Get backup count from drive stats if connected
    let backup_count = if is_connected && dest.uuid.is_some() {
        // Try to get backup count from drive stats
        if let Ok(client) = WaypointHelperClient::new() {
            match client.get_drive_stats(dest.mount_point.clone()) {
                Ok(stats) => Some(stats.backup_count),
                Err(_) => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    let fs_badge = if dest.fstype == "btrfs" {
        "btrfs"
    } else {
        &dest.fstype
    };

    let mut subtitle_parts = Vec::new();

    // Status indicator (emoji for visual clarity)
    if is_connected {
        subtitle_parts.push("🟢 Connected".to_string());
    } else {
        subtitle_parts.push("⚪ Disconnected".to_string());
    }

    // Drive type and filesystem
    subtitle_parts.push(format!("{} • {}", type_badge, fs_badge));

    // Backup count (if available)
    if let Some(count) = backup_count {
        subtitle_parts.push(format!("{} backup{}", count, if count == 1 { "" } else { "s" }));
    }

    // Pending count
    if let Some(ref uuid) = dest.uuid {
        let pending_count = backup_manager.borrow().get_pending_count(uuid);
        if pending_count > 0 {
            subtitle_parts.push(format!("{} pending", pending_count));
        }
    }

    let subtitle = subtitle_parts.join(" • ");
    row.set_subtitle(&subtitle);

    // Add icon based on drive type and connection status
    let icon_name = if !is_connected {
        "network-offline-symbolic" // Disconnected icon
    } else {
        match dest.drive_type {
            DriveType::Removable => "media-removable-symbolic",
            DriveType::Network => "network-server-symbolic",
            DriveType::Internal => "drive-harddisk-symbolic",
        }
    };
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_margin_start(6);
    icon.set_margin_end(6);
    row.add_prefix(&icon);

    // Get current configuration if UUID exists
    let uuid = dest.uuid.clone();
    let (is_enabled, current_filter, on_snapshot_creation, on_drive_mount) =
        if let Some(ref uuid) = uuid {
            let config = backup_manager.borrow().get_config().unwrap_or_default();
            if let Some(dest_config) = config.get_destination(uuid) {
                (
                    dest_config.enabled,
                    dest_config.filter.clone(),
                    dest_config.on_snapshot_creation,
                    dest_config.on_drive_mount,
                )
            } else {
                (false, BackupFilter::All, true, true)
            }
        } else {
            (false, BackupFilter::All, true, true)
        };

    // Add enable switch
    let enable_switch = gtk::Switch::new();
    enable_switch.set_active(is_enabled);
    enable_switch.set_valign(gtk::Align::Center);
    row.add_suffix(&enable_switch);

    // Expanded content - configuration options
    if uuid.is_some() {
        // Drive health status section
        if is_connected {
            let health_group = create_drive_health_section(&dest.mount_point);
            row.add_row(&health_group);
        }

        // Backup filter selector
        let filter_row = adw::ActionRow::new();
        filter_row.set_title("Backup Filter");

        let filter_combo = gtk::DropDown::from_strings(&["All Snapshots", "Favorites Only"]);
        filter_combo.set_selected(match current_filter {
            BackupFilter::All => 0,
            BackupFilter::Favorites => 1,
        });
        filter_combo.set_valign(gtk::Align::Center);
        filter_row.add_suffix(&filter_combo);

        row.add_row(&filter_row);

        // Auto-backup on snapshot creation toggle
        let on_creation_row = adw::ActionRow::new();
        on_creation_row.set_title("Backup on Snapshot Creation");
        on_creation_row.set_subtitle("Automatically queue backups when new snapshots are created");

        let on_creation_switch = gtk::Switch::new();
        on_creation_switch.set_active(on_snapshot_creation);
        on_creation_switch.set_valign(gtk::Align::Center);
        on_creation_row.add_suffix(&on_creation_switch);

        row.add_row(&on_creation_row);

        // Auto-backup on drive mount toggle
        let on_mount_row = adw::ActionRow::new();
        on_mount_row.set_title("Backup on Drive Mount");
        on_mount_row.set_subtitle("Automatically process backups when this drive is connected");

        let on_mount_switch = gtk::Switch::new();
        on_mount_switch.set_active(on_drive_mount);
        on_mount_switch.set_valign(gtk::Align::Center);
        on_mount_row.add_suffix(&on_mount_switch);

        row.add_row(&on_mount_row);

        // Rename row
        let rename_row = adw::ActionRow::new();
        rename_row.set_title("Drive Nickname");
        rename_row.set_subtitle("Optional custom name for this drive");

        let nickname_entry = gtk::Entry::new();
        nickname_entry.set_placeholder_text(Some("e.g., My Work Backup"));
        nickname_entry.set_valign(gtk::Align::Center);
        nickname_entry.set_width_chars(20);

        // Load current nickname if exists
        if let Some(ref uuid_val) = uuid {
            let config = backup_manager.borrow().get_config().unwrap_or_default();
            if let Some(dest_config) = config.get_destination(uuid_val) {
                if let Some(ref nickname) = dest_config.nickname {
                    nickname_entry.set_text(nickname);
                }
            }
        }

        let nickname_icon = gtk::Image::from_icon_name("document-edit-symbolic");
        nickname_icon.set_margin_end(6);
        rename_row.add_suffix(&nickname_icon);
        rename_row.add_suffix(&nickname_entry);

        row.add_row(&rename_row);

        // Retention policy row
        let retention_row = adw::ActionRow::new();
        retention_row.set_title("Backup Retention");
        retention_row.set_subtitle("How long to keep backups before automatic cleanup");

        // Load current retention setting
        let current_retention_days = if let Some(ref uuid_val) = uuid {
            let config = backup_manager.borrow().get_config().unwrap_or_default();
            config.destinations
                .get(uuid_val)
                .and_then(|d| d.retention_days)
        } else {
            None
        };

        // Create retention dropdown with presets
        let retention_options = [
            "Keep Forever",
            "1 Week (7 days)",
            "2 Weeks (14 days)",
            "1 Month (30 days)",
            "2 Months (60 days)",
            "3 Months (90 days)",
            "6 Months (180 days)",
            "1 Year (365 days)",
        ];
        let retention_dropdown = gtk::DropDown::from_strings(&retention_options);

        // Set current selection based on retention_days
        let selected_index = match current_retention_days {
            None => 0, // Keep Forever
            Some(7) => 1,
            Some(14) => 2,
            Some(30) => 3,
            Some(60) => 4,
            Some(90) => 5,
            Some(180) => 6,
            Some(365) => 7,
            Some(_) => 0, // Custom value, default to Keep Forever for now
        };
        retention_dropdown.set_selected(selected_index);
        retention_dropdown.set_valign(gtk::Align::Center);

        retention_row.add_suffix(&retention_dropdown);
        row.add_row(&retention_row);

        // View backups button row
        let view_row = adw::ActionRow::new();
        view_row.set_title("View Existing Backups");

        if !is_connected {
            view_row.set_subtitle("Drive must be connected to view backups");
        }

        let view_button = Button::with_label("View");
        view_button.set_valign(gtk::Align::Center);
        view_button.set_sensitive(is_connected); // Disable if not connected
        let dest_mount = dest.mount_point.clone();
        let parent_clone = parent.clone();
        view_button.connect_clicked(move |_| {
            show_backups_list_dialog(&parent_clone, &dest_mount);
        });
        view_row.add_suffix(&view_button);

        row.add_row(&view_row);

        // Verify backups button row
        let verify_row = adw::ActionRow::new();
        verify_row.set_title("Verify Backup Integrity");

        if !is_connected {
            verify_row.set_subtitle("Drive must be connected to verify backups");
        } else {
            verify_row.set_subtitle("Check if backups are intact and readable");
        }

        let verify_button = Button::with_label("Verify All");
        verify_button.set_valign(gtk::Align::Center);
        verify_button.set_sensitive(is_connected); // Disable if not connected

        let dest_mount_verify = dest.mount_point.clone();
        let parent_verify = parent.clone();
        verify_button.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("Verifying...");

            let btn_clone = btn.clone();
            let mount_clone = dest_mount_verify.clone();
            let parent_clone = parent_verify.clone();

            // Spawn the verification work in a background thread
            let (sender, receiver) = async_channel::bounded(1);

            std::thread::spawn(move || {
                let result = verify_all_backups(&mount_clone);
                let _ = sender.send_blocking(result);
            });

            // Handle the result on the main thread
            gtk::glib::spawn_future_local(async move {
                if let Ok(result) = receiver.recv().await {
                    btn_clone.set_sensitive(true);
                    btn_clone.set_label("Verify All");
                    show_verification_results_dialog(&parent_clone, result);
                }
            });
        });

        verify_row.add_suffix(&verify_button);
        row.add_row(&verify_row);

        // Forget destination button row
        let forget_row = adw::ActionRow::new();
        forget_row.set_title("Forget This Destination");
        forget_row.set_subtitle("Remove this drive from backup destinations");

        let forget_button = Button::with_label("Forget");
        forget_button.set_valign(gtk::Align::Center);
        forget_button.add_css_class("destructive-action");

        if let Some(ref uuid_val) = uuid {
            let uuid_forget = uuid_val.clone();
            let bm_forget = backup_manager.clone();
            let parent_forget = parent.clone();
            forget_button.connect_clicked(move |_| {
                let uuid_clone = uuid_forget.clone();
                let bm_clone = bm_forget.clone();

                dialogs::show_confirmation(
                    &parent_forget,
                    "Forget Destination?",
                    "This will remove this backup destination from the configuration. Existing backups on the drive will not be deleted.",
                    "Forget",
                    true,
                    move || {
                        if let Err(e) = bm_clone.borrow().remove_destination(&uuid_clone) {
                            log::error!("Failed to remove destination: {}", e);
                        } else {
                            log::info!("Removed destination {}", uuid_clone);
                        }
                    },
                );
            });
        }

        forget_row.add_suffix(&forget_button);
        row.add_row(&forget_row);

        // Connect settings changes
        if let Some(uuid) = uuid {
            // Helper function to save current configuration
            let save_config = {
                let uuid = uuid.clone();
                let label = dest.label.clone();
                let mount = dest.mount_point.clone();
                let fstype = dest.fstype.clone(); // Clone fstype to avoid lifetime issues
                let bm = backup_manager.clone();
                let enable_sw = enable_switch.clone();
                let filter_dd = filter_combo.clone();
                let on_creation_sw = on_creation_switch.clone();
                let on_mount_sw = on_mount_switch.clone();
                let nickname_ent = nickname_entry.clone();
                let retention_dd = retention_dropdown.clone();
                let parent_window = parent.clone();

                move || {
                    let filter_index = filter_dd.selected();
                    let filter = if filter_index == 0 {
                        BackupFilter::All
                    } else {
                        BackupFilter::Favorites
                    };

                    // Get nickname from entry, convert empty string to None
                    let nickname_text = nickname_ent.text().to_string();
                    let nickname = if nickname_text.trim().is_empty() {
                        None
                    } else {
                        Some(nickname_text)
                    };

                    // Get retention days from dropdown selection
                    let retention_days = match retention_dd.selected() {
                        0 => None,           // Keep Forever
                        1 => Some(7),        // 1 Week
                        2 => Some(14),       // 2 Weeks
                        3 => Some(30),       // 1 Month
                        4 => Some(60),       // 2 Months
                        5 => Some(90),       // 3 Months
                        6 => Some(180),      // 6 Months
                        7 => Some(365),      // 1 Year
                        _ => None,           // Default to Keep Forever
                    };

                    let dest_config = BackupDestinationConfig {
                        uuid: uuid.clone(),
                        label: label.clone(),
                        nickname,
                        last_mount_point: mount.clone(),
                        fstype: fstype.clone(),
                        enabled: enable_sw.is_active(),
                        filter,
                        on_snapshot_creation: on_creation_sw.is_active(),
                        on_drive_mount: on_mount_sw.is_active(),
                        retention_days,
                    };

                    if let Err(e) = bm.borrow().add_destination(uuid.clone(), dest_config) {
                        log::error!("Failed to save backup configuration: {}", e);
                        dialogs::show_toast(&parent_window, "Failed to save backup settings");
                    } else {
                        dialogs::show_toast(&parent_window, "Backup settings saved");
                    }
                }
            };

            // Connect enable switch
            let save_clone = save_config.clone();
            enable_switch.connect_active_notify(move |_| {
                save_clone();
            });

            // Connect filter dropdown
            let save_clone = save_config.clone();
            filter_combo.connect_selected_notify(move |_| {
                save_clone();
            });

            // Connect on_snapshot_creation switch
            let save_clone = save_config.clone();
            on_creation_switch.connect_active_notify(move |_| {
                save_clone();
            });

            // Connect on_drive_mount switch
            let save_clone = save_config.clone();
            on_mount_switch.connect_active_notify(move |_| {
                save_clone();
            });

            // Connect retention dropdown
            let save_clone = save_config.clone();
            retention_dropdown.connect_selected_notify(move |_| {
                save_clone();
            });

            // Connect nickname entry (save on focus out or Enter key)
            nickname_entry.connect_activate(move |_| {
                save_config();
            });
        }
    }

    row
}

/// Show dialog listing backups at a destination
fn show_backups_list_dialog(parent: &adw::ApplicationWindow, destination_mount: &str) {
    let dialog = adw::Window::new();
    dialog.set_title(Some("Backups"));
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));
    dialog.set_default_size(600, 500);

    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Backups", destination_mount)));
    content.append(&header);

    // Loading state
    let loading_label = Label::new(Some("Loading backups..."));
    loading_label.set_vexpand(true);
    loading_label.add_css_class("dim-label");
    content.append(&loading_label);

    dialog.set_content(Some(&content));
    dialog.present();

    // Load backups in background
    let dest_mount = destination_mount.to_string();
    let dialog_clone = dialog.clone();
    let content_clone = content.clone();
    let parent_clone = parent.clone();

    // Use thread + channel pattern
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = list_backups(&dest_mount);
        let _ = tx.send(result);
    });

    // Poll for result
    gtk::glib::spawn_future_local(async move {
        let result = loop {
            match rx.try_recv() {
                Ok(result) => break result,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    gtk::glib::timeout_future(std::time::Duration::from_millis(50)).await;
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    content_clone.remove(&loading_label);
                    dialog_clone.close();
                    dialogs::show_error(
                        &parent_clone,
                        "Load Failed",
                        "List backups thread disconnected unexpectedly",
                    );
                    return;
                }
            }
        };

        // Remove loading label
        content_clone.remove(&loading_label);

        match result {
            Ok(backups) => {
                if backups.is_empty() {
                    let empty_status = adw::StatusPage::new();
                    empty_status.set_title("No Backups");
                    empty_status.set_description(Some("No backups found at this destination"));
                    empty_status.set_icon_name(Some("folder-symbolic"));
                    empty_status.set_vexpand(true);
                    content_clone.append(&empty_status);
                } else {
                    let scrolled = gtk::ScrolledWindow::new();
                    scrolled.set_vexpand(true);

                    let list_box = gtk::ListBox::new();
                    list_box.add_css_class("boxed-list");
                    list_box.set_margin_top(12);
                    list_box.set_margin_bottom(12);
                    list_box.set_margin_start(12);
                    list_box.set_margin_end(12);

                    for backup_path in &backups {
                        let row = adw::ActionRow::new();

                        // Extract snapshot name from path
                        let name = std::path::Path::new(backup_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(backup_path);

                        row.set_title(name);
                        row.set_subtitle(backup_path);

                        list_box.append(&row);
                    }

                    scrolled.set_child(Some(&list_box));
                    content_clone.append(&scrolled);
                }
            }
            Err(e) => {
                dialog_clone.close();
                dialogs::show_error(
                    &parent_clone,
                    "Load Failed",
                    &format!("Failed to list backups: {}", e),
                );
            }
        }
    });
}

/// List backups at a destination
fn list_backups(destination_mount: &str) -> anyhow::Result<Vec<String>> {
    let client = WaypointHelperClient::new()?;
    let (success, result) = client.list_backups(destination_mount.to_string())?;

    if !success {
        return Err(anyhow::anyhow!(result));
    }

    // Parse JSON response
    let backups: Vec<String> = serde_json::from_str(&result)?;
    Ok(backups)
}

/// Update backup status summary
fn update_backup_status_summary(row: &adw::ActionRow, backup_manager: Rc<RefCell<BackupManager>>) {
    use waypoint_common::BackupStatus;

    let config = match backup_manager.borrow().get_config() {
        Ok(c) => c,
        Err(_) => {
            row.set_subtitle("Unable to load queue status");
            return;
        }
    };

    let pending_backups = &config.pending_backups;

    if pending_backups.is_empty() {
        row.set_subtitle("No pending backups");
        return;
    }

    let mut pending_count = 0;
    let mut in_progress_count = 0;
    let mut failed_count = 0;

    for pb in pending_backups {
        match pb.status {
            BackupStatus::Pending => pending_count += 1,
            BackupStatus::InProgress => in_progress_count += 1,
            BackupStatus::Failed => failed_count += 1,
            _ => {}
        }
    }

    let parts = vec![
        if in_progress_count > 0 {
            Some(format!("{} in progress", in_progress_count))
        } else {
            None
        },
        if pending_count > 0 {
            Some(format!("{} pending", pending_count))
        } else {
            None
        },
        if failed_count > 0 {
            Some(format!("{} failed", failed_count))
        } else {
            None
        },
    ];

    let subtitle = parts.into_iter().flatten().collect::<Vec<_>>().join(", ");
    row.set_subtitle(&subtitle);
}

/// Create pending backups list widget
fn create_pending_backups_list(backup_manager: Rc<RefCell<BackupManager>>) -> gtk::Box {
    use waypoint_common::BackupStatus;

    let container = gtk::Box::new(Orientation::Vertical, 6);

    // Load pending backups
    let config = match backup_manager.borrow().get_config() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load backup config: {}", e);
            let error_label = Label::new(Some(&format!("Failed to load pending backups: {}", e)));
            error_label.add_css_class("dim-label");
            container.append(&error_label);
            return container;
        }
    };

    let pending_backups = &config.pending_backups;

    if pending_backups.is_empty() {
        let empty_label = Label::new(Some("No pending backups"));
        empty_label.add_css_class("dim-label");
        container.append(&empty_label);
        return container;
    }

    // Group by status for better organization
    let mut pending = Vec::new();
    let mut failed = Vec::new();
    let mut in_progress = Vec::new();

    for pb in pending_backups {
        match pb.status {
            BackupStatus::Pending => pending.push(pb),
            BackupStatus::Failed => failed.push(pb),
            BackupStatus::InProgress => in_progress.push(pb),
            _ => {}
        }
    }

    // Show in-progress backups first
    if !in_progress.is_empty() {
        for pb in in_progress {
            // Create a container for the row and progress bar
            let item_box = gtk::Box::new(Orientation::Vertical, 6);
            item_box.set_margin_bottom(12);

            let row = adw::ActionRow::new();
            row.set_title(&pb.snapshot_id);

            // Calculate elapsed time
            let elapsed = if let Some(start_time) = pb.last_attempt {
                let now = chrono::Utc::now().timestamp();
                let elapsed_secs = (now - start_time).max(0);
                format_elapsed_time(elapsed_secs)
            } else {
                "Just started".to_string()
            };

            // Get friendly name for destination
            let dest_name = config.destinations
                .get(&pb.destination_uuid)
                .map(|d| d.display_name().to_string())
                .unwrap_or_else(|| pb.destination_uuid.clone());

            row.set_subtitle(&format!("Backing up to {} • {}", dest_name, elapsed));

            let status_icon = gtk::Image::from_icon_name("emblem-synchronizing-symbolic");
            status_icon.set_pixel_size(16);
            status_icon.set_tooltip_text(Some("Backup in progress"));
            row.add_prefix(&status_icon);

            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            spinner.set_valign(gtk::Align::Center);
            row.add_suffix(&spinner);

            item_box.append(&row);

            // Add progress bar
            let progress_bar = gtk::ProgressBar::new();
            progress_bar.set_show_text(true);
            progress_bar.set_margin_start(12);
            progress_bar.set_margin_end(12);
            progress_bar.set_margin_top(6);

            // Check if we have live progress data
            if let Some(live_progress) = backup_manager.borrow().get_progress(&pb.snapshot_id, &pb.destination_uuid) {
                // Map stage to progress percentage
                let (fraction, text) = match live_progress.stage.as_str() {
                    "preparing" => (0.10, "Preparing..."),
                    "transferring" => (0.50, "Transferring..."),
                    "complete" => (1.0, "Complete"),
                    _ => (0.0, "In progress..."),
                };

                progress_bar.set_fraction(fraction);
                progress_bar.set_text(Some(text));
            } else {
                // Fallback to indeterminate progress
                progress_bar.set_text(Some("Transferring..."));
                progress_bar.pulse();
            }

            item_box.append(&progress_bar);

            container.append(&item_box);
        }
    }

    // Show failed backups with retry button
    if !failed.is_empty() {
        for pb in failed {
            let row = adw::ActionRow::new();
            row.set_title(&pb.snapshot_id);

            // Get friendly name for destination
            let dest_name = config.destinations
                .get(&pb.destination_uuid)
                .map(|d| d.display_name().to_string())
                .unwrap_or_else(|| pb.destination_uuid.clone());

            row.set_subtitle(&format!(
                "Destination: {} • {} attempts",
                dest_name, pb.retry_count
            ));

            let status_icon = gtk::Image::from_icon_name("dialog-error-symbolic");
            status_icon.set_pixel_size(16);
            status_icon.add_css_class("error");
            row.add_prefix(&status_icon);

            // Add retry button
            let retry_btn = Button::with_label("Retry");
            retry_btn.set_valign(gtk::Align::Center);
            retry_btn.add_css_class("flat");

            let dest_uuid = pb.destination_uuid.clone();
            let bm_retry = backup_manager.clone();
            retry_btn.connect_clicked(move |_| {
                if let Err(e) = bm_retry.borrow().retry_failed_backups(&dest_uuid) {
                    log::error!("Failed to retry backups: {}", e);
                } else {
                    log::info!("Retrying failed backups for destination {}", dest_uuid);
                }
            });

            row.add_suffix(&retry_btn);

            container.append(&row);
        }
    }

    // Show pending backups
    if !pending.is_empty() {
        for pb in pending {
            let row = adw::ActionRow::new();
            row.set_title(&pb.snapshot_id);

            // Get friendly name for destination
            let dest_name = config.destinations
                .get(&pb.destination_uuid)
                .map(|d| d.display_name().to_string())
                .unwrap_or_else(|| pb.destination_uuid.clone());

            row.set_subtitle(&format!("Waiting for: {}", dest_name));

            let status_icon = gtk::Image::from_icon_name("document-save-symbolic");
            status_icon.set_pixel_size(16);
            status_icon.add_css_class("dim-label");
            row.add_prefix(&status_icon);

            container.append(&row);
        }
    }

    container
}

/// Create backup statistics rows (currently unused - may be useful for future features)
#[allow(dead_code)]
fn create_backup_statistics(backup_manager: Rc<RefCell<BackupManager>>) -> Vec<adw::ActionRow> {
    use crate::snapshot::format_bytes;

    let config = match backup_manager.borrow().get_config() {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let mut rows = Vec::new();

    // Total backups count
    let total_row = adw::ActionRow::new();
    total_row.set_title("Total Backups");
    total_row.set_subtitle(&format!("{} completed", config.backup_history.len()));
    rows.push(total_row);

    // Unique snapshots backed up
    let unique_snapshots: std::collections::HashSet<_> = config
        .backup_history
        .iter()
        .map(|r| r.snapshot_id.as_str())
        .collect();
    let unique_row = adw::ActionRow::new();
    unique_row.set_title("Snapshots with Backups");
    unique_row.set_subtitle(&format!("{} snapshots", unique_snapshots.len()));
    rows.push(unique_row);

    // Total storage used
    let total_bytes: u64 = config
        .backup_history
        .iter()
        .filter_map(|r| r.size_bytes)
        .sum();
    if total_bytes > 0 {
        let storage_row = adw::ActionRow::new();
        storage_row.set_title("Total Backup Size");
        storage_row.set_subtitle(&format_bytes(total_bytes));
        rows.push(storage_row);
    }

    // Incremental backup ratio
    let incremental_count = config
        .backup_history
        .iter()
        .filter(|r| r.is_incremental)
        .count();
    if config.backup_history.len() > 0 {
        let ratio = (incremental_count as f64 / config.backup_history.len() as f64) * 100.0;
        let ratio_row = adw::ActionRow::new();
        ratio_row.set_title("Incremental Backups");
        ratio_row.set_subtitle(&format!(
            "{:.1}% ({} of {})",
            ratio,
            incremental_count,
            config.backup_history.len()
        ));
        rows.push(ratio_row);
    }

    rows
}

/// Create backup history rows (currently unused - may be useful for future features)
#[allow(dead_code)]
fn create_backup_history(backup_manager: Rc<RefCell<BackupManager>>) -> Vec<adw::ActionRow> {
    use crate::snapshot::format_bytes;

    let config = match backup_manager.borrow().get_config() {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    if config.backup_history.is_empty() {
        let empty_row = adw::ActionRow::new();
        empty_row.set_title("No backup history yet");
        empty_row.set_subtitle("Backups will appear here after completion");
        return vec![empty_row];
    }

    let mut rows = Vec::new();

    // Sort by most recent first
    let mut history = config.backup_history.clone();
    history.sort_by_key(|r| std::cmp::Reverse(r.completed_at));

    // Show last 10
    for record in history.iter().take(10) {
        let row = adw::ActionRow::new();
        row.set_title(&record.snapshot_id);

        // Format timestamp
        let datetime = chrono::DateTime::from_timestamp(record.completed_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "Unknown date".to_string());

        // Build subtitle
        let mut subtitle_parts = vec![datetime];

        if let Some(size) = record.size_bytes {
            subtitle_parts.push(format_bytes(size));
        }

        if record.is_incremental {
            subtitle_parts.push("Incremental".to_string());
        } else {
            subtitle_parts.push("Full".to_string());
        }

        row.set_subtitle(&subtitle_parts.join(" • "));

        // Add icon
        let icon = gtk::Image::from_icon_name("emblem-ok-symbolic");
        icon.add_css_class("success");
        row.add_prefix(&icon);

        rows.push(row);
    }

    rows
}

/// Format elapsed time in human-readable format
/// Examples: "2s", "1m 30s", "2h 15m", "3d 4h"
fn format_elapsed_time(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        if secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {}s", mins, secs)
        }
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        if mins == 0 {
            format!("{}h", hours)
        } else {
            format!("{}h {}m", hours, mins)
        }
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        if hours == 0 {
            format!("{}d", days)
        } else {
            format!("{}d {}h", days, hours)
        }
    }
}

/// Create drive health status section showing space usage, backup count, and timestamps
fn create_drive_health_section(mount_point: &str) -> gtk::Box {
    use crate::dbus_client::{DriveStats, WaypointHelperClient};

    let container = gtk::Box::new(Orientation::Vertical, 12);
    container.set_margin_top(12);
    container.set_margin_bottom(12);
    container.set_margin_start(12);
    container.set_margin_end(12);

    // Header
    let header_box = gtk::Box::new(Orientation::Horizontal, 8);
    let health_icon = gtk::Image::from_icon_name("emblem-ok-symbolic");
    health_icon.add_css_class("success");
    let header_label = Label::new(Some("Drive Health"));
    header_label.add_css_class("heading");
    header_box.append(&health_icon);
    header_box.append(&header_label);
    container.append(&header_box);

    // Try to get drive stats
    let stats_result = std::thread::spawn({
        let mount_point = mount_point.to_string();
        move || -> anyhow::Result<DriveStats> {
            let client = WaypointHelperClient::new()?;
            client.get_drive_stats(mount_point)
        }
    })
    .join();

    match stats_result {
        Ok(Ok(stats)) => {
            // Calculate percentages and health
            let used_pct = if stats.total_bytes > 0 {
                (stats.used_bytes as f64 / stats.total_bytes as f64 * 100.0) as u32
            } else {
                0
            };

            // Health indicator: green if <75%, yellow if 75-90%, red if >90%
            let (health_class, health_text) = if used_pct < 75 {
                ("success", "Healthy")
            } else if used_pct < 90 {
                ("warning", "Running Low")
            } else {
                ("error", "Nearly Full")
            };

            // Update header icon based on health
            health_icon.remove_css_class("success");
            health_icon.add_css_class(health_class);

            // Space usage bar
            let space_box = gtk::Box::new(Orientation::Vertical, 4);
            let space_label = Label::new(Some("Storage"));
            space_label.set_xalign(0.0);
            space_label.add_css_class("dim-label");
            space_label.add_css_class("caption");

            let progress_bar = gtk::ProgressBar::new();
            progress_bar.set_fraction(used_pct as f64 / 100.0);
            progress_bar.set_show_text(false);

            // Add appropriate CSS class based on usage
            if used_pct >= 90 {
                progress_bar.add_css_class("error");
            } else if used_pct >= 75 {
                progress_bar.add_css_class("warning");
            }

            let space_text = Label::new(Some(&format!(
                "{} / {} ({} free) • {}",
                format_bytes(stats.used_bytes),
                format_bytes(stats.total_bytes),
                format_bytes(stats.available_bytes),
                health_text
            )));
            space_text.set_xalign(0.0);
            space_text.add_css_class("caption");

            space_box.append(&space_label);
            space_box.append(&progress_bar);
            space_box.append(&space_text);
            container.append(&space_box);

            // Backup statistics
            let stats_grid = gtk::Grid::new();
            stats_grid.set_row_spacing(6);
            stats_grid.set_column_spacing(24);
            stats_grid.set_margin_top(8);

            // Backup count
            let count_label = Label::new(Some("Backups Stored"));
            count_label.set_xalign(0.0);
            count_label.add_css_class("dim-label");
            count_label.add_css_class("caption");
            let count_value = Label::new(Some(&stats.backup_count.to_string()));
            count_value.set_xalign(0.0);
            count_value.add_css_class("caption");
            stats_grid.attach(&count_label, 0, 0, 1, 1);
            stats_grid.attach(&count_value, 1, 0, 1, 1);

            // Last backup time
            if let Some(last_time) = stats.last_backup_timestamp {
                let last_label = Label::new(Some("Last Backup"));
                last_label.set_xalign(0.0);
                last_label.add_css_class("dim-label");
                last_label.add_css_class("caption");

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let age_secs = (now - last_time).max(0) as u64;
                let age_text = if age_secs < 60 {
                    "Just now".to_string()
                } else {
                    format!("{} ago", format_elapsed_time(age_secs as i64))
                };

                let last_value = Label::new(Some(&age_text));
                last_value.set_xalign(0.0);
                last_value.add_css_class("caption");
                stats_grid.attach(&last_label, 0, 1, 1, 1);
                stats_grid.attach(&last_value, 1, 1, 1, 1);
            }

            // Oldest backup
            if let Some(oldest_time) = stats.oldest_backup_timestamp {
                let oldest_label = Label::new(Some("Oldest Backup"));
                oldest_label.set_xalign(0.0);
                oldest_label.add_css_class("dim-label");
                oldest_label.add_css_class("caption");

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let age_secs = (now - oldest_time).max(0) as u64;
                let age_text = format_elapsed_time(age_secs as i64);

                let oldest_value = Label::new(Some(&format!("{} old", age_text)));
                oldest_value.set_xalign(0.0);
                oldest_value.add_css_class("caption");
                stats_grid.attach(&oldest_label, 0, 2, 1, 1);
                stats_grid.attach(&oldest_value, 1, 2, 1, 1);
            }

            container.append(&stats_grid);
        }
        Ok(Err(e)) => {
            let error_label = Label::new(Some(&format!("Failed to get drive stats: {}", e)));
            error_label.set_xalign(0.0);
            error_label.add_css_class("dim-label");
            error_label.add_css_class("caption");
            container.append(&error_label);
        }
        Err(_) => {
            let error_label = Label::new(Some("Failed to retrieve drive statistics"));
            error_label.set_xalign(0.0);
            error_label.add_css_class("dim-label");
            error_label.add_css_class("caption");
            container.append(&error_label);
        }
    }

    container
}

/// Format bytes into human-readable string (e.g., "1.5 GB")
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Result of verifying all backups on a destination
#[derive(Debug)]
struct VerificationResults {
    total: usize,
    passed: usize,
    failed: usize,
    details: Vec<(String, bool, String)>, // (snapshot_id, success, message)
}

/// Verify all backups on a destination
fn verify_all_backups(destination_mount: &str) -> VerificationResults {
    use waypoint_common::WaypointConfig;

    let config = WaypointConfig::new();
    let snapshot_dir = config.snapshot_dir;

    let client = match WaypointHelperClient::new() {
        Ok(c) => c,
        Err(e) => {
            return VerificationResults {
                total: 0,
                passed: 0,
                failed: 1,
                details: vec![("Error".to_string(), false, format!("Failed to connect to helper: {}", e))],
            };
        }
    };

    // List backups on the destination
    let backups = match client.list_backups(destination_mount.to_string()) {
        Ok((true, json)) => {
            match serde_json::from_str::<Vec<String>>(&json) {
                Ok(b) => b,
                Err(e) => {
                    return VerificationResults {
                        total: 0,
                        passed: 0,
                        failed: 1,
                        details: vec![("Error".to_string(), false, format!("Failed to parse backups: {}", e))],
                    };
                }
            }
        }
        Ok((false, err)) => {
            return VerificationResults {
                total: 0,
                passed: 0,
                failed: 1,
                details: vec![("Error".to_string(), false, err)],
            };
        }
        Err(e) => {
            return VerificationResults {
                total: 0,
                passed: 0,
                failed: 1,
                details: vec![("Error".to_string(), false, format!("Failed to list backups: {}", e))],
            };
        }
    };

    if backups.is_empty() {
        return VerificationResults {
            total: 0,
            passed: 0,
            failed: 0,
            details: vec![("Info".to_string(), true, "No backups found on this destination".to_string())],
        };
    }

    let mut results = VerificationResults {
        total: backups.len(),
        passed: 0,
        failed: 0,
        details: Vec::new(),
    };

    for backup_id in backups {
        let snapshot_path = snapshot_dir.join(&backup_id);

        let result = client.verify_backup(
            snapshot_path.to_string_lossy().to_string(),
            destination_mount.to_string(),
            backup_id.clone(),
        );

        match result {
            Ok((true, json)) => {
                // Parse the verification result
                match serde_json::from_str::<crate::dbus_client::BackupVerificationResult>(&json) {
                    Ok(verify_result) => {
                        if verify_result.success {
                            results.passed += 1;
                            results.details.push((backup_id, true, verify_result.message));
                        } else {
                            results.failed += 1;
                            results.details.push((backup_id, false, verify_result.message));
                        }
                    }
                    Err(e) => {
                        results.failed += 1;
                        results.details.push((backup_id, false, format!("Failed to parse result: {}", e)));
                    }
                }
            }
            Ok((false, err)) => {
                results.failed += 1;
                results.details.push((backup_id, false, err));
            }
            Err(e) => {
                results.failed += 1;
                results.details.push((backup_id, false, format!("Verification error: {}", e)));
            }
        }
    }

    results
}

/// Show verification results dialog
fn show_verification_results_dialog(parent: &adw::ApplicationWindow, results: VerificationResults) {
    let dialog = adw::Window::new();
    dialog.set_title(Some("Backup Verification Results"));
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));
    dialog.set_default_size(600, 500);

    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Verification Results", "")));
    content.append(&header);

    // Summary section
    let summary_group = adw::PreferencesGroup::new();
    summary_group.set_title("Summary");
    summary_group.set_margin_top(12);
    summary_group.set_margin_start(12);
    summary_group.set_margin_end(12);

    let summary_row = adw::ActionRow::new();
    if results.failed == 0 && results.passed > 0 {
        summary_row.set_title(&format!("✓ All {} backup(s) verified successfully", results.passed));
        summary_row.add_css_class("success");
    } else if results.failed > 0 {
        summary_row.set_title(&format!("⚠ {} passed, {} failed out of {} total",
            results.passed, results.failed, results.total));
        summary_row.add_css_class("warning");
    } else {
        summary_row.set_title("No backups to verify");
    }

    summary_group.add(&summary_row);
    content.append(&summary_group);

    // Details section
    if !results.details.is_empty() {
        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_hexpand(true);
        scrolled.set_margin_top(12);
        scrolled.set_margin_start(12);
        scrolled.set_margin_end(12);
        scrolled.set_margin_bottom(12);

        let list_box = gtk::ListBox::new();
        list_box.add_css_class("boxed-list");

        for (snapshot_id, success, message) in results.details {
            let row = adw::ActionRow::new();
            row.set_title(&snapshot_id);
            row.set_subtitle(&message);

            let icon_name = if success {
                "emblem-ok-symbolic"
            } else {
                "dialog-warning-symbolic"
            };

            let icon = gtk::Image::from_icon_name(icon_name);
            if !success {
                icon.add_css_class("warning");
            }
            row.add_prefix(&icon);

            list_box.append(&row);
        }

        scrolled.set_child(Some(&list_box));
        content.append(&scrolled);
    }

    dialog.set_content(Some(&content));
    dialog.present();
}
