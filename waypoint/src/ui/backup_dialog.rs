//! Backup management UI
//!
//! Provides interface for backing up snapshots to external drives

use gtk::prelude::*;
use gtk::{Button, Label, Orientation};
use libadwaita as adw;
use adw::prelude::*;
use serde::{Deserialize, Serialize};

use crate::dbus_client::WaypointHelperClient;
use super::dialogs;

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
}

use crate::backup_manager::BackupManager;
use std::rc::Rc;
use std::cell::RefCell;

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
        "Create incremental backups of your snapshots to external drives using btrfs send/receive. \
         This provides disaster recovery in case of system failure."
    ));

    content_box.append(&header_group);

    // Destinations section
    let dest_group = adw::PreferencesGroup::new();
    dest_group.set_title("Backup Destinations");
    dest_group.set_description(Some("Available external drives for backups"));
    dest_group.set_margin_top(18);

    // Scan button
    let scan_row = adw::ActionRow::new();
    scan_row.set_title("Scan for Destinations");
    scan_row.set_subtitle("Detect available external drives");

    let scan_button = Button::with_label("Scan");
    scan_button.set_valign(gtk::Align::Center);
    scan_button.add_css_class("suggested-action");
    scan_row.add_suffix(&scan_button);

    dest_group.add(&scan_row);

    // Destinations list container
    let destinations_list = adw::PreferencesGroup::new();
    destinations_list.set_margin_top(12);
    destinations_list.set_visible(false); // Hidden until scan is clicked

    let dest_list_clone = destinations_list.clone();
    let parent_clone = parent.clone();
    let backup_manager_clone = backup_manager.clone();

    scan_button.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        btn.set_label("Scanning...");

        let dest_list = dest_list_clone.clone();
        let parent_ref = parent_clone.clone();
        let btn_clone = btn.clone();
        let bm_clone = backup_manager_clone.clone();

        // Use thread + channel pattern instead of tokio
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = scan_destinations();
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
                        dialogs::show_error(&parent_ref, "Scan Failed", "Scan thread disconnected unexpectedly");
                        btn_clone.set_sensitive(true);
                        btn_clone.set_label("Scan");
                        return;
                    }
                }
            };

            btn_clone.set_sensitive(true);
            btn_clone.set_label("Scan");

            // Clear existing destinations
            while let Some(child) = dest_list.first_child() {
                dest_list.remove(&child);
            }

            match result {
                Ok(destinations) => {
                    if destinations.is_empty() {
                        let empty_row = adw::ActionRow::new();
                        empty_row.set_title("No external drives found");
                        empty_row.set_subtitle("Connect an external btrfs drive and scan again");
                        dest_list.add(&empty_row);
                    } else {
                        for dest in &destinations {
                            let row = create_destination_row(dest, &parent_ref, bm_clone.clone());
                            dest_list.add(&row);
                        }
                    }

                    dest_list.set_visible(true);
                }
                Err(e) => {
                    dialogs::show_error(&parent_ref, "Scan Failed", &format!("Failed to scan for destinations: {}", e));
                }
            }
        });
    });

    content_box.append(&dest_group);
    content_box.append(&destinations_list);

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

    // Info section
    let info_group = adw::PreferencesGroup::new();
    info_group.set_title("About Backups");
    info_group.set_margin_top(18);

    let info_row = adw::ActionRow::new();
    info_row.set_title("Incremental Backups");
    info_row.set_subtitle("Backups use btrfs send/receive for efficient incremental transfers");
    info_group.add(&info_row);

    let dest_row = adw::ActionRow::new();
    dest_row.set_title("Backup Location");
    dest_row.set_subtitle("Backups are stored in waypoint-backups/ at the drive root");
    info_group.add(&dest_row);

    content_box.append(&info_group);

    // Settings section
    let settings_group = adw::PreferencesGroup::new();
    settings_group.set_title("Backup Settings");
    settings_group.set_margin_top(18);

    // Mount check interval setting
    let interval_row = adw::ActionRow::new();
    interval_row.set_title("Mount Check Interval");
    interval_row.set_subtitle("How often to check for newly mounted backup drives (in seconds)");

    let current_interval = backup_manager.borrow()
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
            log::info!("Updated mount check interval to {} seconds (requires restart to take effect)", new_value);
        }
    });

    interval_row.add_suffix(&interval_spin);
    settings_group.add(&interval_row);

    content_box.append(&settings_group);

    // Statistics section
    let stats_group = adw::PreferencesGroup::new();
    stats_group.set_title("Backup Statistics");
    stats_group.set_margin_top(18);

    let stats_content = create_backup_statistics(backup_manager.clone());
    for row in stats_content {
        stats_group.add(&row);
    }

    content_box.append(&stats_group);

    // Recent backups section
    let history_group = adw::PreferencesGroup::new();
    history_group.set_title("Recent Backups");
    history_group.set_description(Some("Last 10 completed backups"));
    history_group.set_margin_top(18);

    let history_content = create_backup_history(backup_manager);
    for row in history_content {
        history_group.add(&row);
    }

    content_box.append(&history_group);

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

/// Create a row for a backup destination
fn create_destination_row(
    dest: &BackupDestination,
    parent: &adw::ApplicationWindow,
    backup_manager: Rc<RefCell<BackupManager>>,
) -> adw::ExpanderRow {
    use waypoint_common::{BackupDestinationConfig, BackupFilter};

    let row = adw::ExpanderRow::new();

    // Add drive type badge to title
    let type_badge = match dest.drive_type {
        DriveType::Removable => " (USB)",
        DriveType::Network => " (Network)",
        DriveType::Internal => " (Internal)",
    };
    row.set_title(&format!("{}{}", dest.label, type_badge));

    // Show pending count in subtitle if available
    let subtitle = if let Some(ref uuid) = dest.uuid {
        let pending_count = backup_manager.borrow().get_pending_count(uuid);
        if pending_count > 0 {
            format!("{} • {} pending backup{}", dest.mount_point, pending_count, if pending_count == 1 { "" } else { "s" })
        } else {
            dest.mount_point.clone()
        }
    } else {
        dest.mount_point.clone()
    };
    row.set_subtitle(&subtitle);

    // Add icon based on drive type
    let icon_name = match dest.drive_type {
        DriveType::Removable => "media-removable-symbolic",
        DriveType::Network => "network-server-symbolic",
        DriveType::Internal => "drive-harddisk-symbolic",
    };
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_margin_start(6);
    icon.set_margin_end(6);
    row.add_prefix(&icon);

    // Get current configuration if UUID exists
    let uuid = dest.uuid.clone();
    let (is_enabled, current_filter, on_snapshot_creation, on_drive_mount) = if let Some(ref uuid) = uuid {
        let config = backup_manager.borrow().get_config().unwrap_or_default();
        if let Some(dest_config) = config.destinations.get(uuid) {
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

        // View backups button row
        let view_row = adw::ActionRow::new();
        view_row.set_title("View Existing Backups");

        let view_button = Button::with_label("View");
        view_button.set_valign(gtk::Align::Center);
        let dest_mount = dest.mount_point.clone();
        let parent_clone = parent.clone();
        view_button.connect_clicked(move |_| {
            show_backups_list_dialog(&parent_clone, &dest_mount);
        });
        view_row.add_suffix(&view_button);

        row.add_row(&view_row);

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
                let bm = backup_manager.clone();
                let enable_sw = enable_switch.clone();
                let filter_dd = filter_combo.clone();
                let on_creation_sw = on_creation_switch.clone();
                let on_mount_sw = on_mount_switch.clone();

                move || {
                    let filter_index = filter_dd.selected();
                    let filter = if filter_index == 0 {
                        BackupFilter::All
                    } else {
                        BackupFilter::Favorites
                    };

                    let dest_config = BackupDestinationConfig {
                        uuid: uuid.clone(),
                        label: label.clone(),
                        last_mount_point: mount.clone(),
                        enabled: enable_sw.is_active(),
                        filter,
                        on_snapshot_creation: on_creation_sw.is_active(),
                        on_drive_mount: on_mount_sw.is_active(),
                    };

                    if let Err(e) = bm.borrow().add_destination(uuid.clone(), dest_config) {
                        log::error!("Failed to save backup configuration: {}", e);
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
            on_mount_switch.connect_active_notify(move |_| {
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
                    dialogs::show_error(&parent_clone, "Load Failed", "List backups thread disconnected unexpectedly");
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
                dialogs::show_error(&parent_clone, "Load Failed", &format!("Failed to list backups: {}", e));
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
fn create_pending_backups_list(
    backup_manager: Rc<RefCell<BackupManager>>,
) -> gtk::Box {
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
            let row = adw::ActionRow::new();
            row.set_title(&pb.snapshot_id);
            row.set_subtitle(&format!("Destination: {}", pb.destination_uuid));

            let status_icon = gtk::Image::from_icon_name("emblem-synchronizing-symbolic");
            status_icon.set_pixel_size(16);
            status_icon.set_tooltip_text(Some("Backup in progress"));
            row.add_prefix(&status_icon);

            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            spinner.set_valign(gtk::Align::Center);
            row.add_suffix(&spinner);

            container.append(&row);
        }
    }

    // Show failed backups with retry button
    if !failed.is_empty() {
        for pb in failed {
            let row = adw::ActionRow::new();
            row.set_title(&pb.snapshot_id);
            row.set_subtitle(&format!("Destination: {} • {} attempts", pb.destination_uuid, pb.retry_count));

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
            row.set_subtitle(&format!("Waiting for drive: {}", pb.destination_uuid));

            let status_icon = gtk::Image::from_icon_name("document-save-symbolic");
            status_icon.set_pixel_size(16);
            status_icon.add_css_class("dim-label");
            row.add_prefix(&status_icon);

            container.append(&row);
        }
    }

    container
}

/// Create backup statistics rows
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
    let incremental_count = config.backup_history.iter().filter(|r| r.is_incremental).count();
    if config.backup_history.len() > 0 {
        let ratio = (incremental_count as f64 / config.backup_history.len() as f64) * 100.0;
        let ratio_row = adw::ActionRow::new();
        ratio_row.set_title("Incremental Backups");
        ratio_row.set_subtitle(&format!("{:.1}% ({} of {})", ratio, incremental_count, config.backup_history.len()));
        rows.push(ratio_row);
    }

    rows
}

/// Create backup history rows
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
