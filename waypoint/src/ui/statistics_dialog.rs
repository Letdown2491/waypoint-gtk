use gtk::prelude::*;
use gtk::{Button, Label, Orientation};
use libadwaita as adw;
use adw::prelude::*;
use std::rc::Rc;
use std::cell::RefCell;

use crate::snapshot::{SnapshotManager, format_bytes};
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
    sized_snapshots.sort_by(|a, b| {
        let a_size = a.size_bytes.unwrap_or(0);
        let b_size = b.size_bytes.unwrap_or(0);
        b_size.cmp(&a_size)
    });

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

            let size_label = Label::new(Some(&format_bytes(snapshot.size_bytes.unwrap_or(0))));
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

    // Progress spinner (initially hidden)
    let spinner = gtk::Spinner::new();
    spinner.set_valign(gtk::Align::Center);
    spinner.set_visible(false);

    // Clone for the button callback
    let dialog_clone = dialog.clone();
    let manager_clone = manager.clone();
    let spinner_clone = spinner.clone();
    let calc_button_row_clone = calc_button_row.clone();
    calc_btn.connect_clicked(move |btn| {
        btn.set_visible(false);
        spinner_clone.set_visible(true);
        spinner_clone.start();
        calc_button_row_clone.set_subtitle("Calculating disk usage, this may take a while...");

        let dialog_clone2 = dialog_clone.clone();
        let manager_clone2 = manager_clone.clone();
        let btn_clone = btn.clone();
        let spinner_clone2 = spinner_clone.clone();
        let calc_button_row_clone2 = calc_button_row_clone.clone();

        // Run calculation asynchronously
        glib::spawn_future_local(async move {
            calculate_missing_sizes_async(&manager_clone2, &calc_button_row_clone2).await;

            // Restore button state
            spinner_clone2.stop();
            spinner_clone2.set_visible(false);
            btn_clone.set_visible(true);
            calc_button_row_clone2.set_subtitle("Calculate disk usage for snapshots without size data");

            // Close and reopen dialog to refresh stats
            dialog_clone2.close();
            if let Some(parent) = dialog_clone2.transient_for()
                .and_then(|w| w.downcast::<adw::ApplicationWindow>().ok()) {
                show_statistics_dialog(&parent, &manager_clone2);
            }
        });
    });

    calc_button_row.add_suffix(&spinner);
    calc_button_row.add_suffix(&calc_btn);
    calc_button_row.set_activatable_widget(Some(&calc_btn));
    maintenance_group.add(&calc_button_row);

    main_box.append(&maintenance_group);

    scrolled.set_child(Some(&main_box));
    content.append(&scrolled);
    dialog.set_content(Some(&content));
    dialog.present();
}

/// Calculate sizes for all snapshots that don't have size data (asynchronously)
async fn calculate_missing_sizes_async(
    manager: &Rc<RefCell<SnapshotManager>>,
    progress_row: &adw::ActionRow,
) {
    let mut snapshots = match manager.borrow().load_snapshots() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to load snapshots: {}", e);
            return;
        }
    };

    // Count how many need calculation
    let total_to_calculate = snapshots.iter().filter(|s| s.size_bytes.is_none()).count();

    if total_to_calculate == 0 {
        eprintln!("All snapshots already have size data");
        return;
    }

    let mut updated = false;
    let mut calculated_count = 0;

    for snapshot in &mut snapshots {
        if snapshot.size_bytes.is_none() {
            eprintln!("Calculating size for snapshot: {}", snapshot.name);

            // Update progress
            progress_row.set_subtitle(&format!(
                "Calculating {} ({}/{})...",
                snapshot.name,
                calculated_count + 1,
                total_to_calculate
            ));

            // Run du command - since we're in an async function running in glib's event loop,
            // we need to yield to prevent blocking. Use glib::spawn_future to run the
            // blocking operation
            let path = snapshot.path.clone();

            // Create a oneshot channel manually
            let (tx, rx) = std::sync::mpsc::channel();

            // Spawn blocking task in thread pool
            std::thread::spawn(move || {
                let result = btrfs::get_snapshot_size(&path);
                let _ = tx.send(result);
            });

            // Poll for result without blocking the UI
            loop {
                match rx.try_recv() {
                    Ok(Ok(size)) => {
                        snapshot.size_bytes = Some(size);
                        updated = true;
                        calculated_count += 1;
                        eprintln!("  Size: {}", format_bytes(size));
                        break;
                    }
                    Ok(Err(e)) => {
                        eprintln!("  Failed to calculate size: {}", e);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        // Wait a bit and try again
                        glib::timeout_future(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        eprintln!("  Channel disconnected");
                        break;
                    }
                }
            }

            // Small delay to allow UI updates
            glib::timeout_future(std::time::Duration::from_millis(10)).await;
        }
    }

    if updated {
        if let Err(e) = manager.borrow().save_snapshots(&snapshots) {
            eprintln!("Failed to save snapshot metadata: {}", e);
        } else {
            eprintln!("Successfully calculated {} snapshot sizes", calculated_count);
        }
    }
}
