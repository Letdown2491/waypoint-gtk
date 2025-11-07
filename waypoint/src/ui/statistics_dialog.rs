use gtk::prelude::*;
use gtk::{Button, Label, Orientation};
use libadwaita as adw;
use adw::prelude::*;
use std::rc::Rc;
use std::cell::RefCell;

use crate::snapshot::{SnapshotManager, format_bytes};
use crate::retention::RetentionPolicy;
use crate::btrfs;

/// Show statistics dialog with disk space and retention info
pub fn show_statistics_dialog(parent: &adw::ApplicationWindow, manager: &Rc<RefCell<SnapshotManager>>) {
    let dialog = adw::Window::new();
    dialog.set_title(Some("Snapshot Statistics"));
    dialog.set_default_size(600, 500);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));

    // Main content
    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Snapshot Statistics", "")));
    content.append(&header);

    // Scrollable content
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);

    let main_box = gtk::Box::new(Orientation::Vertical, 24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);

    // Get statistics
    let stats = match manager.borrow().get_statistics() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to get statistics: {}", e);
            let error = Label::new(Some(&format!("Error loading statistics: {}", e)));
            error.add_css_class("error");
            main_box.append(&error);
            scrolled.set_child(Some(&main_box));
            content.append(&scrolled);
            dialog.set_content(Some(&content));
            dialog.present();
            return;
        }
    };

    // Disk Space Section
    let disk_group = adw::PreferencesGroup::new();
    disk_group.set_title("Disk Space Usage");
    disk_group.set_description(Some("Total space used by all snapshots"));

    // Total snapshots count
    let count_row = adw::ActionRow::new();
    count_row.set_title("Total Snapshots");
    count_row.set_subtitle(&format!("{} snapshots", stats.total_count));
    let count_icon = gtk::Image::from_icon_name("folder-documents-symbolic");
    count_row.add_prefix(&count_icon);
    disk_group.add(&count_row);

    // Total size
    let size_row = adw::ActionRow::new();
    size_row.set_title("Total Size");
    size_row.set_subtitle(&format_bytes(stats.total_size));
    let size_icon = gtk::Image::from_icon_name("drive-harddisk-symbolic");
    size_row.add_prefix(&size_icon);
    disk_group.add(&size_row);

    // Oldest snapshot age
    let age_row = adw::ActionRow::new();
    age_row.set_title("Oldest Snapshot");
    age_row.set_subtitle(&format!("{} days old", stats.oldest_age_days));
    let age_icon = gtk::Image::from_icon_name("document-open-recent-symbolic");
    age_row.add_prefix(&age_icon);
    disk_group.add(&age_row);

    // Available space
    if let Ok(available) = btrfs::get_available_space(&std::path::PathBuf::from("/")) {
        let avail_row = adw::ActionRow::new();
        avail_row.set_title("Available Space");
        avail_row.set_subtitle(&format_bytes(available));
        let avail_icon = gtk::Image::from_icon_name("drive-harddisk-symbolic");
        avail_row.add_prefix(&avail_icon);
        disk_group.add(&avail_row);
    }

    main_box.append(&disk_group);

    // Largest Snapshots Section
    let snapshots = match manager.borrow().load_snapshots() {
        Ok(s) => s,
        Err(_) => Vec::new(),
    };

    // Filter snapshots that have size data and sort by size (largest first)
    let mut sized_snapshots: Vec<_> = snapshots.iter()
        .filter(|s| s.size_bytes.is_some())
        .collect();
    sized_snapshots.sort_by(|a, b| b.size_bytes.unwrap().cmp(&a.size_bytes.unwrap()));

    if !sized_snapshots.is_empty() {
        let largest_group = adw::PreferencesGroup::new();
        largest_group.set_title("Largest Snapshots");
        largest_group.set_description(Some("Top snapshots by disk usage"));

        // Show top 3 largest snapshots
        for (i, snapshot) in sized_snapshots.iter().take(3).enumerate() {
            let row = adw::ActionRow::new();
            row.set_title(&snapshot.name);
            if let Some(desc) = &snapshot.description {
                row.set_subtitle(desc);
            }

            let size_label = Label::new(Some(&format_bytes(snapshot.size_bytes.unwrap())));
            size_label.add_css_class("dim-label");
            row.add_suffix(&size_label);

            let rank_icon = match i {
                0 => gtk::Image::from_icon_name("emblem-important-symbolic"),
                1 => gtk::Image::from_icon_name("emblem-default-symbolic"),
                _ => gtk::Image::from_icon_name("emblem-documents-symbolic"),
            };
            row.add_prefix(&rank_icon);

            largest_group.add(&row);
        }

        main_box.append(&largest_group);
    }

    // Retention Policy Section
    let retention = match RetentionPolicy::load() {
        Ok(p) => p,
        Err(_) => RetentionPolicy::default(),
    };

    let retention_group = adw::PreferencesGroup::new();
    retention_group.set_title("Retention Policy");
    retention_group.set_description(Some("Automatic cleanup settings"));

    let policy_row = adw::ActionRow::new();
    policy_row.set_title("Current Policy");
    policy_row.set_subtitle(&retention.description());
    let policy_icon = gtk::Image::from_icon_name("emblem-system-symbolic");
    policy_row.add_prefix(&policy_icon);
    retention_group.add(&policy_row);

    // Show which snapshots would be cleaned up
    if let Ok(to_cleanup) = manager.borrow().get_snapshots_to_cleanup() {
        let cleanup_row = adw::ActionRow::new();
        cleanup_row.set_title("Snapshots to Clean Up");
        if to_cleanup.is_empty() {
            cleanup_row.set_subtitle("No snapshots will be automatically deleted");
        } else {
            cleanup_row.set_subtitle(&format!("{} snapshots will be deleted on next cleanup", to_cleanup.len()));
        }
        let cleanup_icon = gtk::Image::from_icon_name("user-trash-symbolic");
        cleanup_row.add_prefix(&cleanup_icon);
        retention_group.add(&cleanup_row);
    }

    main_box.append(&retention_group);

    // Maintenance section
    let maintenance_group = adw::PreferencesGroup::new();
    maintenance_group.set_title("Maintenance");
    maintenance_group.set_description(Some("Tools for managing snapshot metadata"));

    // Calculate sizes button
    let calc_button_row = adw::ActionRow::new();
    calc_button_row.set_title("Calculate Missing Sizes");
    calc_button_row.set_subtitle("Calculate disk usage for snapshots without size data");

    let calc_btn = Button::with_label("Calculate");
    calc_btn.set_valign(gtk::Align::Center);
    calc_btn.add_css_class("suggested-action");

    // Clone for the button callback
    let dialog_clone = dialog.clone();
    let manager_clone = manager.clone();
    calc_btn.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        btn.set_label("Calculating...");

        let btn_clone = btn.clone();
        let dialog_clone2 = dialog_clone.clone();
        let manager_clone2 = manager_clone.clone();

        gtk::glib::spawn_future_local(async move {
            calculate_missing_sizes(&manager_clone2).await;
            btn_clone.set_sensitive(true);
            btn_clone.set_label("Calculate");

            // Close and reopen dialog to refresh stats
            dialog_clone2.close();
            show_statistics_dialog(&dialog_clone2.transient_for().unwrap().downcast().unwrap(), &manager_clone2);
        });
    });

    calc_button_row.add_suffix(&calc_btn);
    calc_button_row.set_activatable_widget(Some(&calc_btn));
    maintenance_group.add(&calc_button_row);

    main_box.append(&maintenance_group);

    // Settings hint
    let settings_group = adw::PreferencesGroup::new();
    settings_group.set_title("Configuration");

    let config_info = Label::new(Some(
        "Retention policy can be configured in:\n~/.config/waypoint/retention.json"
    ));
    config_info.set_wrap(true);
    config_info.add_css_class("dim-label");
    config_info.set_halign(gtk::Align::Start);
    config_info.set_margin_top(6);
    config_info.set_margin_bottom(6);
    settings_group.add(&config_info);

    let example = Label::new(Some(
        "Example:\n{\n  \"max_snapshots\": 10,\n  \"max_age_days\": 30,\n  \"min_snapshots\": 3\n}"
    ));
    example.set_wrap(true);
    example.add_css_class("monospace");
    example.set_halign(gtk::Align::Start);
    example.set_margin_top(6);
    example.set_margin_bottom(6);
    settings_group.add(&example);

    main_box.append(&settings_group);

    scrolled.set_child(Some(&main_box));
    content.append(&scrolled);
    dialog.set_content(Some(&content));
    dialog.present();
}

/// Calculate sizes for all snapshots that don't have size data
async fn calculate_missing_sizes(manager: &Rc<RefCell<SnapshotManager>>) {
    let mut snapshots = match manager.borrow().load_snapshots() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load snapshots: {}", e);
            return;
        }
    };

    let mut updated = false;
    let mut calculated_count = 0;

    for snapshot in &mut snapshots {
        if snapshot.size_bytes.is_none() {
            eprintln!("Calculating size for snapshot: {}", snapshot.name);
            match btrfs::get_snapshot_size(&snapshot.path) {
                Ok(size) => {
                    snapshot.size_bytes = Some(size);
                    updated = true;
                    calculated_count += 1;
                    eprintln!("  Size: {}", format_bytes(size));
                }
                Err(e) => {
                    eprintln!("  Failed to calculate size: {}", e);
                }
            }
        }
    }

    if updated {
        if let Err(e) = manager.borrow().save_snapshots(&snapshots) {
            eprintln!("Failed to save snapshot metadata: {}", e);
        } else {
            eprintln!("Successfully calculated {} snapshot sizes", calculated_count);
        }
    } else {
        eprintln!("All snapshots already have size data");
    }
}
