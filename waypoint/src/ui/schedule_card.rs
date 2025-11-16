use gtk::prelude::*;
use gtk::{Box, Button, DrawingArea, Label, Orientation, Switch};
use std::cell::RefCell;
use std::rc::Rc;
use waypoint_common::{Schedule, ScheduleType};

/// Data for rendering a sparkline (snapshot history)
#[derive(Debug, Clone)]
pub struct SparklineData {
    /// Success (true) or failure (false) for each time slot
    pub runs: Vec<bool>,
    /// Maximum number of runs to display
    pub max_runs: usize,
}

impl SparklineData {
    pub fn new(max_runs: usize) -> Self {
        Self {
            runs: Vec::new(),
            max_runs,
        }
    }

    /// Add a run result (true = success, false = failure)
    pub fn add_run(&mut self, success: bool) {
        self.runs.insert(0, success);
        if self.runs.len() > self.max_runs {
            self.runs.truncate(self.max_runs);
        }
    }
}

/// A card widget displaying a single schedule's status and configuration
pub struct ScheduleCard {
    /// The main container widget
    pub widget: Box,
    /// The schedule configuration
    schedule: Schedule,
    /// Enable/disable switch
    enable_switch: Switch,
    /// Info box (contains next run, last run, retention)
    info_box: Box,
    /// Sparkline box
    sparkline_box: Box,
    /// Next run label
    next_run_label: Label,
    /// Last run label
    last_run_label: Label,
    /// Retention summary label
    retention_label: Label,
    /// Sparkline drawing area
    sparkline: DrawingArea,
    /// Sparkline data
    sparkline_data: Rc<RefCell<SparklineData>>,
    /// Edit button
    edit_button: Button,
}

impl ScheduleCard {
    /// Create a new schedule card
    pub fn new(schedule: Schedule) -> Self {
        let widget = Box::new(Orientation::Vertical, 0);
        widget.add_css_class("card");
        widget.set_margin_top(4);
        widget.set_margin_bottom(4);
        widget.set_margin_start(6);
        widget.set_margin_end(6);

        // Header row with title and status
        let header_box = Box::new(Orientation::Horizontal, 12);
        header_box.set_margin_top(8);
        header_box.set_margin_bottom(6);
        header_box.set_margin_start(12);
        header_box.set_margin_end(12);

        // Enable switch
        let enable_switch = Switch::new();
        enable_switch.set_active(schedule.enabled);
        enable_switch.set_valign(gtk::Align::Center);
        header_box.append(&enable_switch);

        // Title (no icon)
        let title_label = Label::new(Some(&Self::get_schedule_title(&schedule.schedule_type)));
        title_label.add_css_class("title-4");
        title_label.set_halign(gtk::Align::Start);
        header_box.append(&title_label);

        // Spacer
        let spacer = Box::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header_box.append(&spacer);

        // Edit button (only visible when enabled)
        let edit_button = Button::with_label("Edit");
        edit_button.add_css_class("flat");
        edit_button.set_visible(schedule.enabled);
        header_box.append(&edit_button);

        widget.append(&header_box);

        // Information rows (only shown when enabled)
        let info_box = Box::new(Orientation::Vertical, 4);
        info_box.set_margin_start(12);
        info_box.set_margin_end(12);
        info_box.set_visible(schedule.enabled);

        // Next run
        let next_run_label = Label::new(Some("Next run: calculating..."));
        next_run_label.add_css_class("body");
        next_run_label.set_halign(gtk::Align::Start);
        info_box.append(&next_run_label);

        // Last run
        let last_run_label = Label::new(Some("Last success: never"));
        last_run_label.add_css_class("body");
        last_run_label.set_halign(gtk::Align::Start);
        info_box.append(&last_run_label);

        // Retention and prefix
        let prefix_display = if schedule.prefix.is_empty() {
            match schedule.schedule_type {
                ScheduleType::Hourly => "hourly",
                ScheduleType::Daily => "daily",
                ScheduleType::Weekly => "weekly",
                ScheduleType::Monthly => "monthly",
            }
        } else {
            &schedule.prefix
        };

        let retention_text = format!(
            "Retention: {} snapshots • Prefix: {}-",
            schedule.keep_count, prefix_display
        );
        let retention_label = Label::new(Some(&retention_text));
        retention_label.add_css_class("body");
        retention_label.add_css_class("dim-label");
        retention_label.set_halign(gtk::Align::Start);
        info_box.append(&retention_label);

        widget.append(&info_box);

        // Sparkline section (only shown when enabled)
        let sparkline_box = Box::new(Orientation::Vertical, 4);
        sparkline_box.set_margin_top(8);
        sparkline_box.set_margin_start(12);
        sparkline_box.set_margin_end(12);
        sparkline_box.set_visible(schedule.enabled);

        let max_runs = match schedule.schedule_type {
            ScheduleType::Hourly => 24,
            ScheduleType::Daily => 30,
            ScheduleType::Weekly => 12,
            ScheduleType::Monthly => 12,
        };

        let sparkline = DrawingArea::new();
        sparkline.set_height_request(32);
        sparkline.set_content_height(32);

        let sparkline_data = Rc::new(RefCell::new(SparklineData::new(max_runs)));

        // Set up drawing function
        let sparkline_data_clone = sparkline_data.clone();
        sparkline.set_draw_func(move |_, cr, width, height| {
            let data = sparkline_data_clone.borrow();
            Self::draw_sparkline(cr, width, height, &data);
        });

        sparkline_box.append(&sparkline);
        sparkline_box.set_margin_bottom(8);

        widget.append(&sparkline_box);

        Self {
            widget,
            schedule,
            enable_switch,
            info_box,
            sparkline_box,
            next_run_label,
            last_run_label,
            retention_label,
            sparkline,
            sparkline_data,
            edit_button,
        }
    }

    /// Get the title for a schedule type
    fn get_schedule_title(schedule_type: &ScheduleType) -> String {
        match schedule_type {
            ScheduleType::Hourly => "Hourly Snapshots".to_string(),
            ScheduleType::Daily => "Daily Snapshots".to_string(),
            ScheduleType::Weekly => "Weekly Snapshots".to_string(),
            ScheduleType::Monthly => "Monthly Snapshots".to_string(),
        }
    }

    /// Draw the sparkline visualization
    fn draw_sparkline(cr: &gtk::cairo::Context, width: i32, height: i32, data: &SparklineData) {
        let baseline_y = (height as f64 / 2.0).round();

        // Subtle baseline to anchor the sparkline
        cr.set_source_rgba(0.55, 0.55, 0.55, 0.35);
        cr.set_line_width(1.0);
        cr.move_to(0.0, baseline_y);
        cr.line_to(width as f64, baseline_y);
        let _ = cr.stroke();

        if data.runs.is_empty() {
            cr.set_source_rgba(0.5, 0.5, 0.5, 0.35);
            let dash: [f64; 2] = [4.0, 4.0];
            cr.set_dash(&dash, 0.0);
            cr.move_to(6.0, baseline_y);
            cr.line_to(width as f64 - 6.0, baseline_y);
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
            return;
        }

        let bar_count = data.runs.len().min(data.max_runs);
        if bar_count == 0 {
            return;
        }

        let slot_width = (width as f64 / bar_count as f64).max(4.0);
        let radius = (slot_width * 0.35).clamp(2.0, 6.0);
        let success_color = (0.15, 0.66, 0.40); // GNOME success green
        let failure_color = (0.75, 0.11, 0.16); // GNOME destructive red

        for (i, &success) in data.runs.iter().take(bar_count).enumerate() {
            let x_center = width as f64 - ((i as f64 + 0.5) * slot_width);
            let age_factor = 1.0 - (i as f64 / bar_count as f64);
            let alpha = 0.35 + age_factor * 0.55;

            if success {
                cr.set_source_rgba(success_color.0, success_color.1, success_color.2, alpha);
            } else {
                cr.set_source_rgba(failure_color.0, failure_color.1, failure_color.2, alpha);
            }

            cr.arc(x_center, baseline_y, radius, 0.0, std::f64::consts::TAU);
            cr.fill().ok();

            if i == 0 {
                // Highlight the most recent run with a halo
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.85);
                cr.set_line_width(1.5);
                cr.arc(
                    x_center,
                    baseline_y,
                    radius + 1.5,
                    0.0,
                    std::f64::consts::TAU,
                );
                cr.stroke().ok();
            }
        }
    }

    /// Update the next run time display
    #[allow(dead_code)]
    pub fn set_next_run(&mut self, text: &str) {
        self.next_run_label.set_text(&format!("Next run: {}", text));
    }

    /// Update the last run display
    #[allow(dead_code)]
    pub fn set_last_run(&mut self, text: &str, success: bool) {
        let icon = if success { "✓" } else { "✗" };
        self.last_run_label
            .set_text(&format!("Last success: {} {}", text, icon));
    }

    /// Add sparkline data point
    #[allow(dead_code)]
    pub fn add_sparkline_run(&mut self, success: bool) {
        self.sparkline_data.borrow_mut().add_run(success);
        self.sparkline.queue_draw();
    }

    /// Update retention display
    #[allow(dead_code)]
    pub fn set_retention(&mut self, keep_count: u32, prefix: &str) {
        let text = format!("Retention: {} snapshots • Prefix: {}-", keep_count, prefix);
        self.retention_label.set_text(&text);
    }

    /// Get the enable switch widget
    pub fn enable_switch(&self) -> &Switch {
        &self.enable_switch
    }

    /// Get the edit button widget
    pub fn edit_button(&self) -> &Button {
        &self.edit_button
    }

    /// Get the schedule
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// Update the schedule
    pub fn set_schedule(&mut self, schedule: Schedule) {
        self.schedule = schedule.clone();
        self.enable_switch.set_active(schedule.enabled);

        let prefix_display = if schedule.prefix.is_empty() {
            match schedule.schedule_type {
                ScheduleType::Hourly => "hourly",
                ScheduleType::Daily => "daily",
                ScheduleType::Weekly => "weekly",
                ScheduleType::Monthly => "monthly",
            }
        } else {
            &schedule.prefix
        };

        let retention_text = format!(
            "Retention: {} snapshots • Prefix: {}-",
            schedule.keep_count, prefix_display
        );
        self.retention_label.set_text(&retention_text);

        // Show/hide elements based on enabled state
        self.info_box.set_visible(schedule.enabled);
        self.sparkline_box.set_visible(schedule.enabled);
        self.edit_button.set_visible(schedule.enabled);
    }

    /// Get the main widget for this card
    pub fn widget(&self) -> &Box {
        &self.widget
    }
}
