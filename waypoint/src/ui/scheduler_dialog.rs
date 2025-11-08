use crate::dbus_client::WaypointHelperClient;
use gtk::prelude::*;
use gtk::{Box, Button, Label, Orientation, SpinButton};
use libadwaita as adw;
use adw::prelude::*;

/// Show the scheduler configuration dialog
pub fn show_scheduler_dialog(parent: &adw::ApplicationWindow) {
    let dialog = adw::Window::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_title(Some("Scheduled Snapshots"));
    dialog.set_default_size(550, 600);

    // Main container
    let main_box = Box::new(Orientation::Vertical, 0);

    // Header bar
    let header = adw::HeaderBar::new();
    main_box.append(&header);

    // Scrolled window for content
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);
    main_box.append(&scrolled);

    // Content box
    let content_box = Box::new(Orientation::Vertical, 24);
    content_box.set_margin_top(24);
    content_box.set_margin_bottom(24);
    content_box.set_margin_start(24);
    content_box.set_margin_end(24);
    scrolled.set_child(Some(&content_box));

    // Title and description
    let title = Label::new(Some("Scheduled Snapshots"));
    title.add_css_class("title-2");
    title.set_halign(gtk::Align::Start);
    content_box.append(&title);

    let description = Label::new(Some(
        "Configure automatic periodic snapshots using the runit scheduler service."
    ));
    description.set_wrap(true);
    description.set_halign(gtk::Align::Start);
    description.add_css_class("dim-label");
    content_box.append(&description);

    // Service status group
    let status_group = adw::PreferencesGroup::new();
    status_group.set_title("Service Status");
    content_box.append(&status_group);

    let status_row = adw::ActionRow::new();
    status_row.set_title("Scheduler Service");
    let status_label = Label::new(Some("Checking..."));
    status_label.add_css_class("dim-label");
    status_label.set_valign(gtk::Align::Center);
    status_row.add_suffix(&status_label);
    status_group.add(&status_row);

    // Load current config
    let (frequency, time, day, prefix) = load_scheduler_config();

    // Settings group
    let settings_group = adw::PreferencesGroup::new();
    settings_group.set_title("Schedule Settings");
    content_box.append(&settings_group);

    // Frequency dropdown
    let freq_row = adw::ActionRow::new();
    freq_row.set_title("Frequency");
    freq_row.set_subtitle("How often to create snapshots");

    let freq_items = ["Hourly", "Daily", "Weekly", "Custom"];
    let freq_dropdown = gtk::DropDown::from_strings(&freq_items);
    let initial_freq = match frequency.as_str() {
        "hourly" => 0,
        "daily" => 1,
        "weekly" => 2,
        "custom" => 3,
        _ => 1,
    };
    freq_dropdown.set_selected(initial_freq);
    freq_dropdown.set_valign(gtk::Align::Center);
    freq_row.add_suffix(&freq_dropdown);
    settings_group.add(&freq_row);

    // Time picker row (for daily/weekly)
    let time_row = adw::ActionRow::new();
    time_row.set_title("Time of Day");
    time_row.set_subtitle("When to create the snapshot (HH:MM)");

    let time_parts: Vec<&str> = time.split(':').collect();
    let hour = time_parts.get(0).and_then(|h| h.parse::<f64>().ok()).unwrap_or(2.0);
    let minute = time_parts.get(1).and_then(|m| m.parse::<f64>().ok()).unwrap_or(0.0);

    let time_box = Box::new(Orientation::Horizontal, 6);
    let hour_spin = SpinButton::with_range(0.0, 23.0, 1.0);
    hour_spin.set_value(hour);
    hour_spin.set_width_chars(3);
    time_box.append(&hour_spin);
    time_box.append(&Label::new(Some(":")));
    let minute_spin = SpinButton::with_range(0.0, 59.0, 1.0);
    minute_spin.set_value(minute);
    minute_spin.set_width_chars(3);
    time_box.append(&minute_spin);
    time_box.set_valign(gtk::Align::Center);

    time_row.add_suffix(&time_box);
    settings_group.add(&time_row);

    // Day picker row (for weekly)
    let day_row = adw::ActionRow::new();
    day_row.set_title("Day of Week");
    day_row.set_subtitle("Which day for weekly snapshots");

    let day_items = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let day_dropdown = gtk::DropDown::from_strings(&day_items);
    day_dropdown.set_selected(day.parse::<u32>().unwrap_or(0));
    day_dropdown.set_valign(gtk::Align::Center);
    day_row.add_suffix(&day_dropdown);
    settings_group.add(&day_row);

    // Snapshot prefix
    let prefix_row = adw::ActionRow::new();
    prefix_row.set_title("Snapshot Prefix");
    prefix_row.set_subtitle("Prefix for automatic snapshot names");

    let prefix_entry = gtk::Entry::new();
    prefix_entry.set_text(&prefix);
    prefix_entry.set_width_chars(10);
    prefix_entry.set_valign(gtk::Align::Center);
    prefix_row.add_suffix(&prefix_entry);
    settings_group.add(&prefix_row);

    // Update visibility based on frequency
    let freq_dropdown_clone = freq_dropdown.clone();
    let time_row_clone = time_row.clone();
    let day_row_clone = day_row.clone();

    freq_dropdown.connect_selected_notify(move |dropdown| {
        let selected = dropdown.selected();
        // Show time for daily (1) and weekly (2)
        time_row_clone.set_visible(selected == 1 || selected == 2);
        // Show day only for weekly (2)
        day_row_clone.set_visible(selected == 2);
    });

    // Initial visibility
    time_row.set_visible(initial_freq == 1 || initial_freq == 2);
    day_row.set_visible(initial_freq == 2);

    // Info group
    let info_group = adw::PreferencesGroup::new();
    info_group.set_title("Information");
    content_box.append(&info_group);

    let info_label = Label::new(Some(
        "After saving, the scheduler service will be restarted automatically.\n\n\
         Snapshots will be named: [prefix]-YYYYMMDD-HHMM\n\
         Example: auto-20251107-0200\n\n\
         To enable the service: sudo ln -s /etc/sv/waypoint-scheduler /var/service/\n\
         To disable: sudo rm /var/service/waypoint-scheduler"
    ));
    info_label.set_wrap(true);
    info_label.set_halign(gtk::Align::Start);
    info_label.add_css_class("dim-label");
    info_group.add(&info_label);

    // Button box at bottom
    let button_box = Box::new(Orientation::Horizontal, 12);
    button_box.set_margin_top(12);
    button_box.set_margin_bottom(12);
    button_box.set_margin_start(12);
    button_box.set_margin_end(12);
    button_box.set_halign(gtk::Align::End);
    main_box.append(&button_box);

    let cancel_btn = Button::with_label("Cancel");
    button_box.append(&cancel_btn);

    let save_btn = Button::with_label("Save & Restart Service");
    save_btn.add_css_class("suggested-action");
    button_box.append(&save_btn);

    // Cancel button handler
    let dialog_for_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });

    // Save button handler
    let dialog_for_save = dialog.clone();
    let parent_for_save = parent.clone();
    let freq_dropdown_for_save = freq_dropdown_clone.clone();
    let hour_spin_for_save = hour_spin.clone();
    let minute_spin_for_save = minute_spin.clone();
    let day_dropdown_for_save = day_dropdown.clone();
    let prefix_entry_for_save = prefix_entry.clone();

    save_btn.connect_clicked(move |_| {
        let freq_dropdown_clone = freq_dropdown_for_save.clone();
        let hour_spin = hour_spin_for_save.clone();
        let minute_spin = minute_spin_for_save.clone();
        let day_dropdown = day_dropdown_for_save.clone();
        let prefix_entry = prefix_entry_for_save.clone();
        let dialog_for_save = dialog_for_save.clone();
        let parent_for_save = parent_for_save.clone();
        let freq_selected = freq_dropdown_clone.selected();
        let frequency = match freq_selected {
            0 => "hourly",
            1 => "daily",
            2 => "weekly",
            3 => "custom",
            _ => "daily",
        };

        let hour_val = hour_spin.value() as u32;
        let minute_val = minute_spin.value() as u32;
        let time_str = format!("{:02}:{:02}", hour_val, minute_val);

        let day_val = day_dropdown.selected();

        let prefix_str = prefix_entry.text().to_string();
        let prefix_str = if prefix_str.is_empty() { "auto".to_string() } else { prefix_str };

        // Build config file content
        let config_content = format!(
            "# Waypoint Snapshot Scheduler Configuration\n\
             \n\
             SCHEDULE_FREQUENCY=\"{}\"\n\
             SCHEDULE_TIME=\"{}\"\n\
             SCHEDULE_DAY=\"{}\"\n\
             SCHEDULE_INTERVAL=\"86400\"\n\
             SNAPSHOT_PREFIX=\"{}\"\n\
             SNAPSHOT_DESCRIPTION=\"Automated snapshot\"\n",
            frequency, time_str, day_val, prefix_str
        );

        // Save configuration via D-Bus
        let dialog_for_error = dialog_for_save.clone();
        glib::spawn_future_local(async move {
            let result = async {
                let client = WaypointHelperClient::new_async().await?;
                let (success, message) = client.update_scheduler_config(config_content).await?;
                if !success {
                    return Err(anyhow::anyhow!(message));
                }

                // Restart service
                let (success, message) = client.restart_scheduler().await?;
                if !success {
                    return Err(anyhow::anyhow!(message));
                }

                Ok::<(), anyhow::Error>(())
            }.await;

            match result {
                Ok(_) => {
                    println!("✓ Scheduler configuration saved and service restarted");

                    // Show success toast
                    let toast = adw::Toast::new("Scheduler updated and restarted successfully");
                    toast.set_timeout(3);

                    if let Some(window) = parent_for_save.downcast_ref::<adw::ApplicationWindow>() {
                        if let Some(toast_overlay) = window.content()
                            .and_then(|w| w.downcast::<adw::ToastOverlay>().ok()) {
                            toast_overlay.add_toast(toast);
                        }
                    }

                    dialog_for_save.close();
                }
                Err(e) => {
                    eprintln!("Failed to save scheduler configuration: {}", e);
                    let error_dialog = adw::MessageDialog::new(
                        Some(&dialog_for_error),
                        Some("Save Failed"),
                        Some(&format!("Failed to save scheduler configuration: {}", e))
                    );
                    error_dialog.add_response("ok", "OK");
                    error_dialog.set_default_response(Some("ok"));
                    error_dialog.present();
                }
            }
        });
    });

    // Update status asynchronously
    let status_label_clone = status_label.clone();
    glib::spawn_future_local(async move {
        match WaypointHelperClient::new_async().await {
            Ok(client) => {
                match client.get_scheduler_status().await {
                    Ok(status) => {
                        if status == "running" {
                            status_label_clone.set_text("● Running");
                            status_label_clone.add_css_class("success");
                        } else {
                            status_label_clone.set_text("○ Stopped");
                            status_label_clone.add_css_class("warning");
                        }
                    }
                    Err(_) => {
                        status_label_clone.set_text("○ Unknown");
                        status_label_clone.add_css_class("dim-label");
                    }
                }
            }
            Err(_) => {
                status_label_clone.set_text("✗ Error");
                status_label_clone.add_css_class("error");
            }
        }
    });

    dialog.set_content(Some(&main_box));
    dialog.present();
}

/// Load scheduler configuration from file
fn load_scheduler_config() -> (String, String, String, String) {
    match std::fs::read_to_string("/etc/waypoint/scheduler.conf") {
        Ok(content) => {
            let mut frequency = "daily".to_string();
            let mut time = "02:00".to_string();
            let mut day = "0".to_string();
            let mut prefix = "auto".to_string();

            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }

                if let Some(value) = line.strip_prefix("SCHEDULE_FREQUENCY=") {
                    frequency = value.trim_matches('"').to_string();
                } else if let Some(value) = line.strip_prefix("SCHEDULE_TIME=") {
                    time = value.trim_matches('"').to_string();
                } else if let Some(value) = line.strip_prefix("SCHEDULE_DAY=") {
                    day = value.trim_matches('"').to_string();
                } else if let Some(value) = line.strip_prefix("SNAPSHOT_PREFIX=") {
                    prefix = value.trim_matches('"').to_string();
                }
            }

            (frequency, time, day, prefix)
        }
        Err(_) => {
            // Default values
            ("daily".to_string(), "02:00".to_string(), "0".to_string(), "auto".to_string())
        }
    }
}
