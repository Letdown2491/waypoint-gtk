use adw::prelude::*;
use gtk::prelude::*;
use gtk::{Box, Label, Orientation, SpinButton};
use libadwaita as adw;
use waypoint_common::{Schedule, ScheduleType};

/// Create a modal dialog for editing a schedule
pub fn create_schedule_edit_dialog(
    parent: &adw::ApplicationWindow,
    schedule: Schedule,
) -> adw::PreferencesWindow {
    let dialog = adw::PreferencesWindow::new();
    let title = format!(
        "Edit {} Schedule",
        get_schedule_name(&schedule.schedule_type)
    );
    dialog.set_title(Some(&title));
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));

    // Create preferences page
    let page = adw::PreferencesPage::new();

    // Schedule configuration group
    let config_group = adw::PreferencesGroup::new();
    config_group.set_title("Schedule");
    page.add(&config_group);

    // Time setting (for all except hourly)
    let time_row_opt = if schedule.schedule_type != ScheduleType::Hourly {
        let time_row = create_time_row(&schedule);
        config_group.add(&time_row);
        Some(time_row)
    } else {
        None
    };

    // Day of week selector (for weekly)
    let day_of_week_row_opt = if schedule.schedule_type == ScheduleType::Weekly {
        let day_row = create_day_of_week_row(&schedule);
        config_group.add(&day_row);
        Some(day_row)
    } else {
        None
    };

    // Day of month selector (for monthly)
    let day_of_month_row_opt = if schedule.schedule_type == ScheduleType::Monthly {
        let day_row = create_day_of_month_row(&schedule);
        config_group.add(&day_row);
        Some(day_row)
    } else {
        None
    };

    // Naming group
    let naming_group = adw::PreferencesGroup::new();
    naming_group.set_title("Naming");
    page.add(&naming_group);

    let prefix_row = create_prefix_row(&schedule);
    naming_group.add(&prefix_row);

    // Add preview label
    let preview_label = Label::new(None);
    preview_label.set_halign(gtk::Align::Start);
    preview_label.add_css_class("dim-label");
    preview_label.add_css_class("caption");
    preview_label.set_margin_top(6);
    preview_label.set_margin_bottom(12);
    preview_label.set_margin_start(12);
    preview_label.set_margin_end(12);
    update_preview_label(&preview_label, &schedule.prefix);

    // Get the prefix entry and connect to update preview
    if let Some(entry_row) = prefix_row.downcast_ref::<adw::EntryRow>() {
        let preview_clone = preview_label.clone();
        entry_row.connect_changed(move |row| {
            let text = row.text();
            update_preview_label(&preview_clone, text.as_str());
        });
    }

    naming_group.add(&preview_label);

    // Retention group
    let retention_group = adw::PreferencesGroup::new();
    retention_group.set_title("Retention Policy");
    retention_group.set_description(Some(
        "Snapshots will be deleted when either limit is reached",
    ));
    page.add(&retention_group);

    let keep_count_row = create_keep_count_row(&schedule);
    retention_group.add(&keep_count_row);

    let keep_days_row = create_keep_days_row(&schedule);
    retention_group.add(&keep_days_row);

    dialog.add(&page);

    // Store widget references for later data extraction
    unsafe {
        dialog.set_data("schedule_type", schedule.schedule_type as u32);

        if let Some(time_row) = time_row_opt {
            dialog.set_data("time_row", time_row);
        }
        if let Some(day_row) = day_of_week_row_opt {
            dialog.set_data("day_of_week_row", day_row);
        }
        if let Some(day_row) = day_of_month_row_opt {
            dialog.set_data("day_of_month_row", day_row);
        }
        dialog.set_data("prefix_row", prefix_row.clone());
        dialog.set_data("keep_count_row", keep_count_row.clone());
        dialog.set_data("keep_days_row", keep_days_row.clone());
    }

    dialog
}

/// Get the display name for a schedule type
fn get_schedule_name(schedule_type: &ScheduleType) -> &str {
    match schedule_type {
        ScheduleType::Hourly => "Hourly",
        ScheduleType::Daily => "Daily",
        ScheduleType::Weekly => "Weekly",
        ScheduleType::Monthly => "Monthly",
    }
}

/// Create time selection row
fn create_time_row(schedule: &Schedule) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title("Time");
    row.set_subtitle("Time of day to create snapshot (24-hour format)");

    let default_time = "02:00".to_string();
    let time = schedule.time.as_ref().unwrap_or(&default_time);
    let time_parts: Vec<&str> = time.split(':').collect();
    let hour = time_parts
        .first()
        .and_then(|h| h.parse::<f64>().ok())
        .unwrap_or(2.0);
    let minute = time_parts
        .get(1)
        .and_then(|m| m.parse::<f64>().ok())
        .unwrap_or(0.0);

    let time_box = Box::new(Orientation::Horizontal, 6);

    let hour_spin = SpinButton::with_range(0.0, 23.0, 1.0);
    hour_spin.set_value(hour);
    hour_spin.set_width_chars(3);
    hour_spin.set_valign(gtk::Align::Center);
    time_box.append(&hour_spin);

    let colon_label = Label::new(Some(":"));
    colon_label.set_valign(gtk::Align::Center);
    time_box.append(&colon_label);

    let minute_spin = SpinButton::with_range(0.0, 59.0, 1.0);
    minute_spin.set_value(minute);
    minute_spin.set_width_chars(3);
    minute_spin.set_valign(gtk::Align::Center);
    time_box.append(&minute_spin);

    row.add_suffix(&time_box);

    // Store for later retrieval
    unsafe {
        row.set_data("hour_spin", hour_spin);
        row.set_data("minute_spin", minute_spin);
    }

    row
}

/// Create day of week selection row
fn create_day_of_week_row(schedule: &Schedule) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_title("Day of Week");
    row.set_subtitle("Which day to create weekly snapshots");

    let day_items = gtk::StringList::new(&[
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ]);
    row.set_model(Some(&day_items));
    row.set_selected(schedule.day_of_week.unwrap_or(0) as u32);

    row
}

/// Create day of month selection row
fn create_day_of_month_row(schedule: &Schedule) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title("Day of Month");
    row.set_subtitle("Which day of the month to create snapshots");

    let day_spin = SpinButton::with_range(1.0, 31.0, 1.0);
    day_spin.set_value(schedule.day_of_month.unwrap_or(1) as f64);
    day_spin.set_width_chars(3);
    day_spin.set_valign(gtk::Align::Center);
    row.add_suffix(&day_spin);

    // Store for later retrieval
    unsafe {
        row.set_data("day_spin", day_spin);
    }

    row
}

/// Create prefix entry row
fn create_prefix_row(schedule: &Schedule) -> adw::EntryRow {
    let row = adw::EntryRow::new();
    row.set_title("Prefix");
    row.set_text(&schedule.prefix);
    row
}

/// Create keep count row
fn create_keep_count_row(schedule: &Schedule) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title("Keep Count");
    row.set_subtitle("Maximum number of snapshots to keep (0 = unlimited)");

    let spin = SpinButton::with_range(0.0, 100.0, 1.0);
    spin.set_value(schedule.keep_count as f64);
    spin.set_width_chars(5);
    spin.set_valign(gtk::Align::Center);
    row.add_suffix(&spin);

    // Store for later retrieval
    unsafe {
        row.set_data("keep_count_spin", spin);
    }

    row
}

/// Create keep days row
fn create_keep_days_row(schedule: &Schedule) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title("Keep Days");
    row.set_subtitle("Maximum age of snapshots in days (0 = unlimited)");

    let spin = SpinButton::with_range(0.0, 365.0, 1.0);
    spin.set_value(schedule.keep_days as f64);
    spin.set_width_chars(5);
    spin.set_valign(gtk::Align::Center);
    row.add_suffix(&spin);

    // Store for later retrieval
    unsafe {
        row.set_data("keep_days_spin", spin);
    }

    row
}

/// Update the preview label with current prefix
fn update_preview_label(label: &Label, prefix: &str) {
    let now = chrono::Local::now();
    let timestamp = now.format("%Y-%m-%d_%H%M").to_string();
    let preview = format!("Preview: {}-{}", prefix, timestamp);
    label.set_text(&preview);
}

/// Extract schedule data from the edit dialog
pub fn extract_schedule_from_dialog(dialog: &adw::PreferencesWindow) -> Option<Schedule> {
    unsafe {
        let schedule_type_ptr = dialog.data::<u32>("schedule_type")?;
        let schedule_type = match schedule_type_ptr.as_ref() {
            0 => ScheduleType::Hourly,
            1 => ScheduleType::Daily,
            2 => ScheduleType::Weekly,
            3 => ScheduleType::Monthly,
            _ => return None,
        };

        let mut schedule = Schedule {
            enabled: true, // Will be set by the card's switch
            schedule_type,
            time: None,
            day_of_week: None,
            day_of_month: None,
            prefix: String::new(),
            description: format!("{:?} snapshot", schedule_type),
            keep_count: 0,
            keep_days: 0,
        };

        // Extract prefix
        if let Some(prefix_row) = dialog.data::<adw::EntryRow>("prefix_row") {
            schedule.prefix = prefix_row.as_ref().text().to_string();
        }

        // Extract keep count
        if let Some(keep_count_row) = dialog.data::<adw::ActionRow>("keep_count_row") {
            if let Some(keep_count_spin) = keep_count_row
                .as_ref()
                .data::<SpinButton>("keep_count_spin")
            {
                schedule.keep_count = keep_count_spin.as_ref().value() as u32;
            }
        }

        // Extract keep days
        if let Some(keep_days_row) = dialog.data::<adw::ActionRow>("keep_days_row") {
            if let Some(keep_days_spin) =
                keep_days_row.as_ref().data::<SpinButton>("keep_days_spin")
            {
                schedule.keep_days = keep_days_spin.as_ref().value() as u32;
            }
        }

        // Extract time (for non-hourly)
        if let Some(time_row) = dialog.data::<adw::ActionRow>("time_row") {
            if let Some(hour_spin) = time_row.as_ref().data::<SpinButton>("hour_spin") {
                if let Some(minute_spin) = time_row.as_ref().data::<SpinButton>("minute_spin") {
                    let hour = hour_spin.as_ref().value() as u32;
                    let minute = minute_spin.as_ref().value() as u32;
                    schedule.time = Some(format!("{:02}:{:02}", hour, minute));
                }
            }
        }

        // Extract day of week (for weekly)
        if let Some(day_row) = dialog.data::<adw::ComboRow>("day_of_week_row") {
            schedule.day_of_week = Some(day_row.as_ref().selected() as u8);
        }

        // Extract day of month (for monthly)
        if let Some(day_row) = dialog.data::<adw::ActionRow>("day_of_month_row") {
            if let Some(day_spin) = day_row.as_ref().data::<SpinButton>("day_spin") {
                schedule.day_of_month = Some(day_spin.as_ref().value() as u8);
            }
        }

        Some(schedule)
    }
}
