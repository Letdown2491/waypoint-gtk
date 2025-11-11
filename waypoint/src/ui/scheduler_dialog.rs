use crate::dbus_client::WaypointHelperClient;
use gtk::prelude::*;
use gtk::{Box, Button, Label, Orientation, SpinButton};
use libadwaita as adw;
use adw::prelude::*;
use super::dialogs;

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

    // Next snapshot preview
    let preview_group = adw::PreferencesGroup::new();
    preview_group.set_title("Schedule Preview");
    content_box.append(&preview_group);

    let preview_label = Label::new(Some("Calculating next snapshot time..."));
    preview_label.set_wrap(true);
    preview_label.set_halign(gtk::Align::Start);
    preview_label.add_css_class("title-4");
    preview_label.set_margin_top(12);
    preview_label.set_margin_bottom(12);
    preview_label.set_margin_start(12);
    preview_label.set_margin_end(12);
    preview_group.add(&preview_label);

    // Function to update preview
    let update_preview = {
        let freq_dropdown = freq_dropdown.clone();
        let hour_spin = hour_spin.clone();
        let minute_spin = minute_spin.clone();
        let day_dropdown = day_dropdown.clone();
        let preview_label = preview_label.clone();

        move || {
            let freq_selected = freq_dropdown.selected();
            let hour_val = hour_spin.value() as u32;
            let minute_val = minute_spin.value() as u32;
            let day_val = day_dropdown.selected();

            let next_time = calculate_next_snapshot_time(freq_selected, hour_val, minute_val, day_val);
            preview_label.set_text(&next_time);
        }
    };

    // Initial preview
    update_preview();

    // Update preview when values change
    let update_preview_clone1 = update_preview.clone();
    freq_dropdown.connect_selected_notify(move |_| update_preview_clone1());

    let update_preview_clone2 = update_preview.clone();
    hour_spin.connect_value_changed(move |_| update_preview_clone2());

    let update_preview_clone3 = update_preview.clone();
    minute_spin.connect_value_changed(move |_| update_preview_clone3());

    let update_preview_clone4 = update_preview.clone();
    day_dropdown.connect_selected_notify(move |_| update_preview_clone4());

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

        // Save configuration via D-Bus (run in thread to avoid blocking UI)
        let dialog_for_error = dialog_for_save.clone();
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                let client = WaypointHelperClient::new()?;
                let (success, message) = client.update_scheduler_config(config_content)?;
                if !success {
                    return Err(anyhow::anyhow!(message));
                }

                // Restart service
                let (success, message) = client.restart_scheduler()?;
                if !success {
                    return Err(anyhow::anyhow!(message));
                }

                Ok(())
            })();

            let _ = tx.send(result);
        });

        // Handle result on main thread
        glib::spawn_future_local(async move {
            loop {
                match rx.try_recv() {
                    Ok(result) => {
                        match result {
                            Ok(_) => {
                                // Show success toast
                                if let Some(window) = parent_for_save.downcast_ref::<adw::ApplicationWindow>() {
                                    dialogs::show_toast(window, "Scheduler updated and restarted successfully");
                                }

                                dialog_for_save.close();
                            }
                            Err(e) => {
                                log::error!("Failed to save scheduler configuration: {}", e);
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
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        glib::timeout_future(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        log::error!("Channel disconnected");
                        break;
                    }
                }
            }
        });
    });

    // Update status in thread to avoid blocking UI
    let status_label_clone = status_label.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = WaypointHelperClient::new()
            .and_then(|client| client.get_scheduler_status());
        let _ = tx.send(result);
    });

    glib::spawn_future_local(async move {
        loop {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(status) => {
                            match status.as_str() {
                                "running" => {
                                    status_label_clone.set_text("● Running");
                                    status_label_clone.add_css_class("success");
                                }
                                "stopped" => {
                                    status_label_clone.set_text("○ Stopped");
                                    status_label_clone.add_css_class("warning");
                                }
                                "disabled" => {
                                    status_label_clone.set_text("○ Disabled");
                                    status_label_clone.add_css_class("dim-label");
                                }
                                _ => {
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
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    glib::timeout_future(std::time::Duration::from_millis(50)).await;
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    status_label_clone.set_text("✗ Error");
                    status_label_clone.add_css_class("error");
                    break;
                }
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

/// Calculate when the next snapshot will be created based on the schedule
fn calculate_next_snapshot_time(frequency: u32, hour: u32, minute: u32, day_of_week: u32) -> String {
    use chrono::{Local, Datelike, Timelike, Duration};

    let now = Local::now();

    match frequency {
        0 => {
            // Hourly
            let next = now + Duration::hours(1);
            format!("📅 Next snapshot: {} at {:02}:{:02} (in about 1 hour)",
                    next.format("%A, %B %d"),
                    next.hour(),
                    next.minute())
        }
        1 => {
            // Daily
            let mut next = now
                .date_naive()
                .and_hms_opt(hour, minute, 0)
                .unwrap();

            // If today's time has passed, schedule for tomorrow
            if now.time() > next.time() {
                next = (now.date_naive() + Duration::days(1))
                    .and_hms_opt(hour, minute, 0)
                    .unwrap();
            }

            let time_until = next.signed_duration_since(now.naive_local());
            let hours_until = time_until.num_hours();
            let minutes_until = time_until.num_minutes();

            if minutes_until < 60 {
                format!("📅 Next snapshot: Today at {:02}:{:02} (in {} minutes)",
                        hour, minute, minutes_until)
            } else if hours_until < 24 {
                format!("📅 Next snapshot: Today at {:02}:{:02} (in {} hours)",
                        hour, minute, hours_until)
            } else {
                format!("📅 Next snapshot: Tomorrow at {:02}:{:02}",
                        hour, minute)
            }
        }
        2 => {
            // Weekly
            let current_day = now.weekday().num_days_from_sunday();
            let target_day = day_of_week;

            let mut days_until = (target_day as i64 - current_day as i64 + 7) % 7;

            // If it's the same day but time has passed, add 7 days
            if days_until == 0 {
                let target_time = now.date_naive()
                    .and_hms_opt(hour, minute, 0)
                    .unwrap();
                if now.time() >= target_time.time() {
                    days_until = 7;
                }
            }

            let day_names = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
            let day_name = day_names.get(target_day as usize).unwrap_or(&"Unknown");

            if days_until == 0 {
                // Calculate time until for today
                let target_time = now.date_naive()
                    .and_hms_opt(hour, minute, 0)
                    .unwrap();
                let time_until = target_time.signed_duration_since(now.naive_local());
                let hours_until = time_until.num_hours();
                let minutes_until = time_until.num_minutes();

                if minutes_until < 60 {
                    format!("📅 Next snapshot: Today ({}) at {:02}:{:02} (in {} minutes)",
                            day_name, hour, minute, minutes_until)
                } else {
                    format!("📅 Next snapshot: Today ({}) at {:02}:{:02} (in {} hours)",
                            day_name, hour, minute, hours_until)
                }
            } else if days_until == 1 {
                format!("📅 Next snapshot: Tomorrow ({}) at {:02}:{:02}",
                        day_name, hour, minute)
            } else {
                format!("📅 Next snapshot: {} at {:02}:{:02} (in {} days)",
                        day_name, hour, minute, days_until)
            }
        }
        3 => {
            // Custom
            "📅 Custom schedule - refer to configuration file".to_string()
        }
        _ => {
            "📅 Unknown schedule".to_string()
        }
    }
}
