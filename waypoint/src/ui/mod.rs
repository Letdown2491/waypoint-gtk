mod snapshot_row;
mod dialogs;
mod package_diff_dialog;
pub mod preferences;
mod statistics_dialog;
mod create_snapshot_dialog;
mod retention_editor_dialog;
mod scheduler_dialog;
mod toolbar;
mod snapshot_list;

use crate::btrfs;
use crate::dbus_client::WaypointHelperClient;
use crate::snapshot::{Snapshot, SnapshotManager};
use gtk::prelude::*;
use gtk::{Application, Button, Label, ListBox, Orientation, ScrolledWindow, SearchEntry, ToggleButton};
use gtk::glib;
use libadwaita as adw;
use std::sync::mpsc;
use adw::prelude::*;
use snapshot_row::SnapshotAction;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use snapshot_list::DateFilter;

/// Validate that a path is safe to open with xdg-open
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// `Ok(())` if path is safe to open, `Err` with description if invalid
///
/// # Security
/// Only allows paths within known snapshot directories to prevent opening
/// arbitrary files or directories that could be malicious.
fn validate_path_for_open(path: &std::path::Path) -> Result<(), String> {
    // Canonicalize the path to resolve symlinks and get absolute path
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => return Err(format!("Cannot resolve path: {}", e)),
    };

    // Define allowed base directories
    let allowed_dirs = [
        PathBuf::from("/.snapshots"),
        PathBuf::from("/mnt/btrfs-root/@snapshots"),
    ];

    // Check if the canonical path starts with any allowed directory
    for allowed_dir in &allowed_dirs {
        // Try to canonicalize the allowed dir (it might not exist)
        if let Ok(canonical_allowed) = allowed_dir.canonicalize() {
            if canonical.starts_with(&canonical_allowed) {
                return Ok(());
            }
        } else {
            // If allowed dir doesn't exist yet, do string comparison
            if canonical.starts_with(allowed_dir) {
                return Ok(());
            }
        }
    }

    Err(format!(
        "Path '{}' is outside allowed snapshot directories",
        canonical.display()
    ))
}

pub struct MainWindow {
    window: adw::ApplicationWindow,
    snapshot_manager: Rc<RefCell<SnapshotManager>>,
    snapshot_list: ListBox,
    compare_btn: Button,
    _search_entry: SearchEntry,
    _match_label: Label,
    _date_filter: Rc<RefCell<DateFilter>>,
}

impl MainWindow {
    pub fn new(app: &Application) -> adw::ApplicationWindow {
        let snapshot_manager = match SnapshotManager::new() {
            Ok(sm) => Rc::new(RefCell::new(sm)),
            Err(e) => {
                eprintln!("Failed to initialize snapshot manager: {}", e);

                // Create a temporary window to show the error dialog
                let temp_window = adw::ApplicationWindow::builder()
                    .application(app)
                    .build();

                // Show error dialog to user
                let dialog = adw::MessageDialog::new(
                    Some(&temp_window),
                    Some("Failed to Initialize Waypoint"),
                    Some(&format!(
                        "Could not initialize the snapshot manager:\n\n{}\n\n\
                        Please check that:\n\
                        • Btrfs filesystem is available\n\
                        • /.snapshots directory exists and is mounted\n\
                        • D-Bus service is running",
                        e
                    ))
                );

                dialog.add_response("ok", "OK");
                dialog.set_default_response(Some("ok"));

                let app_clone = app.clone();
                dialog.connect_response(None, move |_, _| {
                    app_clone.quit();
                });

                dialog.present();

                // Return the temp window - it will be cleaned up when app quits
                return temp_window;
            }
        };

        // Create header bar
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new("Waypoint", "")));

        // Add application icon to header bar
        let app_icon = if let Ok(icon_path) = std::fs::canonicalize("assets/icons/hicolor/scalable/waypoint.svg") {
            gtk::Image::from_file(&icon_path)
        } else {
            // Fallback to system icon if assets folder not found (installed version)
            gtk::Image::from_icon_name("waypoint")
        };
        app_icon.set_pixel_size(24);
        app_icon.set_margin_start(6);
        header.pack_start(&app_icon);

        // Create hamburger menu
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .build();

        let popover = gtk::Popover::new();
        let popover_box = gtk::Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        // Theme section (using ListBox for proper styling)
        let theme_list = ListBox::new();
        theme_list.set_selection_mode(gtk::SelectionMode::None);
        theme_list.add_css_class("boxed-list");

        let theme_row = adw::ActionRow::builder()
            .title("Switch theme")
            .build();

        // Theme buttons
        let theme_buttons_box = gtk::Box::new(Orientation::Horizontal, 6);

        let system_btn = gtk::Button::builder()
            .label("◐")
            .tooltip_text("Match system theme")
            .build();
        system_btn.add_css_class("flat");
        system_btn.add_css_class("circular");

        let light_btn = gtk::Button::builder()
            .label("○")
            .tooltip_text("Light theme")
            .build();
        light_btn.add_css_class("flat");
        light_btn.add_css_class("circular");

        let dark_btn = gtk::Button::builder()
            .label("●")
            .tooltip_text("Dark theme")
            .build();
        dark_btn.add_css_class("flat");
        dark_btn.add_css_class("circular");

        theme_buttons_box.append(&system_btn);
        theme_buttons_box.append(&light_btn);
        theme_buttons_box.append(&dark_btn);

        theme_row.add_suffix(&theme_buttons_box);
        theme_list.append(&theme_row);
        popover_box.append(&theme_list);

        // Menu items section
        let menu_list = ListBox::new();
        menu_list.set_selection_mode(gtk::SelectionMode::None);
        menu_list.add_css_class("boxed-list");

        let retention_row = adw::ActionRow::builder()
            .title("Retention Policy")
            .activatable(true)
            .build();
        menu_list.append(&retention_row);

        let schedule_row = adw::ActionRow::builder()
            .title("Snapshot Schedule")
            .activatable(true)
            .build();
        menu_list.append(&schedule_row);

        let preferences_row = adw::ActionRow::builder()
            .title("Snapshot Preferences")
            .activatable(true)
            .build();
        menu_list.append(&preferences_row);

        let about_row = adw::ActionRow::builder()
            .title("About Waypoint")
            .activatable(true)
            .build();
        menu_list.append(&about_row);

        popover_box.append(&menu_list);

        popover.set_child(Some(&popover_box));
        menu_button.set_popover(Some(&popover));
        header.pack_end(&menu_button);

        // Disk space indicator
        let disk_space_label = Label::new(Some("Checking space..."));
        disk_space_label.add_css_class("caption");
        disk_space_label.add_css_class("dim-label");
        disk_space_label.set_margin_end(12);
        header.pack_end(&disk_space_label);

        // Status banner - also returns whether Btrfs is available
        let (banner, is_btrfs) = Self::create_status_banner();

        // Toolbar with buttons
        let (toolbar, create_btn, compare_btn, statistics_btn) = toolbar::create_toolbar();

        // Disable create button if not on Btrfs
        if !is_btrfs {
            create_btn.set_sensitive(false);
            create_btn.set_tooltip_text(Some("Btrfs filesystem required"));
        }

        // Search and filter UI
        let search_box = gtk::Box::new(Orientation::Vertical, 12);
        search_box.set_margin_top(12);
        search_box.set_margin_bottom(6);
        search_box.set_margin_start(12);
        search_box.set_margin_end(12);

        // Search entry
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search snapshots..."));
        search_entry.set_hexpand(true);
        search_box.append(&search_entry);

        // Date filter buttons
        let filter_box = gtk::Box::new(Orientation::Horizontal, 6);
        filter_box.add_css_class("linked");

        let all_btn = ToggleButton::with_label("All");
        let week_btn = ToggleButton::with_label("Last 7 days");
        let month_btn = ToggleButton::with_label("Last 30 days");
        let quarter_btn = ToggleButton::with_label("Last 90 days");

        all_btn.set_active(true); // Default to "All"

        filter_box.append(&all_btn);
        filter_box.append(&week_btn);
        filter_box.append(&month_btn);
        filter_box.append(&quarter_btn);

        search_box.append(&filter_box);

        // Match count label
        let match_label = Label::new(None);
        match_label.set_halign(gtk::Align::Start);
        match_label.add_css_class("dim-label");
        match_label.add_css_class("caption");
        search_box.append(&match_label);

        // Snapshot list
        let snapshot_list = ListBox::new();
        snapshot_list.set_selection_mode(gtk::SelectionMode::None);
        snapshot_list.add_css_class("boxed-list");

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_child(Some(&snapshot_list));

        // Use Clamp to constrain content width for better readability (GNOME HIG)
        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(800);
        clamp.set_tightening_threshold(600);
        clamp.set_child(Some(&scrolled));
        clamp.set_margin_top(24);
        clamp.set_margin_bottom(24);
        clamp.set_margin_start(12);
        clamp.set_margin_end(12);

        // Main content box
        let content_box = gtk::Box::new(Orientation::Vertical, 0);
        content_box.append(&banner);
        content_box.append(&toolbar);
        content_box.append(&search_box);
        content_box.append(&clamp);

        // Use ToolbarView for proper GNOME layout
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&content_box));

        // Wrap in ToastOverlay for toast notifications (GNOME HIG)
        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&toolbar_view));

        // Create window
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Waypoint")
            .default_width(800)
            .default_height(600)
            .content(&toast_overlay)
            .build();

        let date_filter = Rc::new(RefCell::new(DateFilter::All));

        let main_window = Self {
            window: window.clone(),
            snapshot_manager: snapshot_manager.clone(),
            snapshot_list: snapshot_list.clone(),
            compare_btn: compare_btn.clone(),
            _search_entry: search_entry.clone(),
            _match_label: match_label.clone(),
            _date_filter: date_filter.clone(),
        };

        // Load snapshots and update button states
        main_window.refresh_snapshot_list();

        // Connect search entry to filter snapshots
        let win_clone_search = window.clone();
        let sm_clone_search = snapshot_manager.clone();
        let list_clone_search = snapshot_list.clone();
        let compare_btn_clone_search = compare_btn.clone();
        let match_label_clone = match_label.clone();
        let date_filter_clone = date_filter.clone();

        search_entry.connect_search_changed(move |entry| {
            let search_text = entry.text().to_string();
            Self::refresh_with_filter(
                &win_clone_search,
                &sm_clone_search,
                &list_clone_search,
                &compare_btn_clone_search,
                &match_label_clone,
                &search_text,
                *date_filter_clone.borrow(),
            );
        });

        // Connect date filter buttons
        let win_clone_all = window.clone();
        let sm_clone_all = snapshot_manager.clone();
        let list_clone_all = snapshot_list.clone();
        let compare_btn_clone_all = compare_btn.clone();
        let match_label_clone_all = match_label.clone();
        let search_entry_clone_all = search_entry.clone();
        let date_filter_clone_all = date_filter.clone();
        let week_btn_clone = week_btn.clone();
        let month_btn_clone = month_btn.clone();
        let quarter_btn_clone = quarter_btn.clone();

        all_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                *date_filter_clone_all.borrow_mut() = DateFilter::All;
                week_btn_clone.set_active(false);
                month_btn_clone.set_active(false);
                quarter_btn_clone.set_active(false);
                let search_text = search_entry_clone_all.text().to_string();
                Self::refresh_with_filter(
                    &win_clone_all,
                    &sm_clone_all,
                    &list_clone_all,
                    &compare_btn_clone_all,
                    &match_label_clone_all,
                    &search_text,
                    DateFilter::All,
                );
            }
        });

        let win_clone_week = window.clone();
        let sm_clone_week = snapshot_manager.clone();
        let list_clone_week = snapshot_list.clone();
        let compare_btn_clone_week = compare_btn.clone();
        let match_label_clone_week = match_label.clone();
        let search_entry_clone_week = search_entry.clone();
        let date_filter_clone_week = date_filter.clone();
        let all_btn_clone = all_btn.clone();
        let month_btn_clone2 = month_btn.clone();
        let quarter_btn_clone2 = quarter_btn.clone();

        week_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                *date_filter_clone_week.borrow_mut() = DateFilter::Last7Days;
                all_btn_clone.set_active(false);
                month_btn_clone2.set_active(false);
                quarter_btn_clone2.set_active(false);
                let search_text = search_entry_clone_week.text().to_string();
                Self::refresh_with_filter(
                    &win_clone_week,
                    &sm_clone_week,
                    &list_clone_week,
                    &compare_btn_clone_week,
                    &match_label_clone_week,
                    &search_text,
                    DateFilter::Last7Days,
                );
            }
        });

        let win_clone_month = window.clone();
        let sm_clone_month = snapshot_manager.clone();
        let list_clone_month = snapshot_list.clone();
        let compare_btn_clone_month = compare_btn.clone();
        let match_label_clone_month = match_label.clone();
        let search_entry_clone_month = search_entry.clone();
        let date_filter_clone_month = date_filter.clone();
        let all_btn_clone2 = all_btn.clone();
        let week_btn_clone2 = week_btn.clone();
        let quarter_btn_clone3 = quarter_btn.clone();

        month_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                *date_filter_clone_month.borrow_mut() = DateFilter::Last30Days;
                all_btn_clone2.set_active(false);
                week_btn_clone2.set_active(false);
                quarter_btn_clone3.set_active(false);
                let search_text = search_entry_clone_month.text().to_string();
                Self::refresh_with_filter(
                    &win_clone_month,
                    &sm_clone_month,
                    &list_clone_month,
                    &compare_btn_clone_month,
                    &match_label_clone_month,
                    &search_text,
                    DateFilter::Last30Days,
                );
            }
        });

        let win_clone_quarter = window.clone();
        let sm_clone_quarter = snapshot_manager.clone();
        let list_clone_quarter = snapshot_list.clone();
        let compare_btn_clone_quarter = compare_btn.clone();
        let match_label_clone_quarter = match_label.clone();
        let search_entry_clone_quarter = search_entry.clone();
        let date_filter_clone_quarter = date_filter.clone();
        let all_btn_clone3 = all_btn.clone();
        let week_btn_clone3 = week_btn.clone();
        let month_btn_clone3 = month_btn.clone();

        quarter_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                *date_filter_clone_quarter.borrow_mut() = DateFilter::Last90Days;
                all_btn_clone3.set_active(false);
                week_btn_clone3.set_active(false);
                month_btn_clone3.set_active(false);
                let search_text = search_entry_clone_quarter.text().to_string();
                Self::refresh_with_filter(
                    &win_clone_quarter,
                    &sm_clone_quarter,
                    &list_clone_quarter,
                    &compare_btn_clone_quarter,
                    &match_label_clone_quarter,
                    &search_text,
                    DateFilter::Last90Days,
                );
            }
        });

        // Connect create button
        let sm_clone = snapshot_manager.clone();
        let list_clone = snapshot_list.clone();
        let win_clone = window.clone();
        let compare_btn_clone = compare_btn.clone();

        create_btn.connect_clicked(move |_| {
            Self::on_create_snapshot(&win_clone, sm_clone.clone(), list_clone.clone(), compare_btn_clone.clone());
        });

        // Connect compare button
        let sm_clone2 = snapshot_manager.clone();
        let win_clone2 = window.clone();

        compare_btn.connect_clicked(move |_| {
            Self::show_compare_dialog(&win_clone2, &sm_clone2);
        });

        // Connect statistics button
        let sm_clone3 = snapshot_manager.clone();
        let win_clone3 = window.clone();

        statistics_btn.connect_clicked(move |_| {
            Self::show_statistics_dialog(&win_clone3, &sm_clone3);
        });

        // Connect theme buttons
        let style_manager = adw::StyleManager::default();
        system_btn.connect_clicked(move |_| {
            style_manager.set_color_scheme(adw::ColorScheme::Default);
        });

        let style_manager_light = adw::StyleManager::default();
        light_btn.connect_clicked(move |_| {
            style_manager_light.set_color_scheme(adw::ColorScheme::ForceLight);
        });

        let style_manager_dark = adw::StyleManager::default();
        dark_btn.connect_clicked(move |_| {
            style_manager_dark.set_color_scheme(adw::ColorScheme::ForceDark);
        });

        // Connect hamburger menu items
        let win_clone_menu_schedule = window.clone();
        let popover_clone_schedule = popover.clone();
        schedule_row.connect_activated(move |_| {
            popover_clone_schedule.popdown();
            scheduler_dialog::show_scheduler_dialog(&win_clone_menu_schedule);
        });

        let win_clone_menu_prefs = window.clone();
        let popover_clone_prefs = popover.clone();
        preferences_row.connect_activated(move |_| {
            popover_clone_prefs.popdown();
            Self::show_preferences_dialog(&win_clone_menu_prefs);
        });

        let win_clone_menu_retention = window.clone();
        let sm_clone_menu_retention = snapshot_manager.clone();
        let popover_clone_retention = popover.clone();
        retention_row.connect_activated(move |_| {
            popover_clone_retention.popdown();
            retention_editor_dialog::show_retention_editor(&win_clone_menu_retention, &sm_clone_menu_retention);
        });

        let win_clone_menu_about = window.clone();
        let popover_clone_about = popover.clone();
        about_row.connect_activated(move |_| {
            popover_clone_about.popdown();
            Self::show_about_dialog(&win_clone_menu_about);
        });

        // Initialize disk space monitoring
        Self::update_disk_space_label(&disk_space_label);

        // Set up periodic disk space updates (every 30 seconds)
        let disk_space_label_clone = disk_space_label.clone();
        glib::timeout_add_seconds_local(30, move || {
            Self::update_disk_space_label(&disk_space_label_clone);
            glib::ControlFlow::Continue
        });

        window
    }

    /// Update the disk space label with current usage
    ///
    /// Queries the available space for the root filesystem and updates the label
    /// with color-coded text based on remaining space percentage.
    fn update_disk_space_label(label: &Label) {
        use std::path::PathBuf;

        // Query disk space for root (where snapshots are stored)
        let space_result = btrfs::get_available_space(&PathBuf::from("/"));

        match space_result {
            Ok(available_bytes) => {
                // Get total filesystem size by reading from df
                let total_result = std::process::Command::new("df")
                    .arg("-B1")
                    .arg("--output=size")
                    .arg("/")
                    .output();

                let (available_gb, total_gb, percent_free) = match total_result {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = stdout.lines().collect();

                        if lines.len() >= 2 {
                            if let Ok(total_bytes) = lines[1].trim().parse::<u64>() {
                                let available_gb = available_bytes as f64 / 1_073_741_824.0; // Convert to GB
                                let total_gb = total_bytes as f64 / 1_073_741_824.0;
                                let percent = (available_bytes as f64 / total_bytes as f64) * 100.0;
                                (available_gb, total_gb, percent)
                            } else {
                                // Fallback: just show available
                                let available_gb = available_bytes as f64 / 1_073_741_824.0;
                                (available_gb, 0.0, 0.0)
                            }
                        } else {
                            let available_gb = available_bytes as f64 / 1_073_741_824.0;
                            (available_gb, 0.0, 0.0)
                        }
                    }
                    _ => {
                        // Fallback: just show available
                        let available_gb = available_bytes as f64 / 1_073_741_824.0;
                        (available_gb, 0.0, 0.0)
                    }
                };

                // Format the label text
                let text = if total_gb > 0.0 {
                    format!("{:.1} GB free of {:.1} GB ({:.0}% free)",
                            available_gb, total_gb, percent_free)
                } else {
                    format!("{:.1} GB free", available_gb)
                };

                label.set_text(&text);

                // Remove any existing warning/error classes
                label.remove_css_class("warning");
                label.remove_css_class("error");

                // Color-code based on percentage (if we have total)
                if total_gb > 0.0 {
                    if percent_free < 10.0 {
                        // Critical: < 10% free - red
                        label.add_css_class("error");
                        label.set_tooltip_text(Some("Low disk space! Consider deleting old snapshots."));
                    } else if percent_free < 20.0 {
                        // Warning: < 20% free - yellow
                        label.add_css_class("warning");
                        label.set_tooltip_text(Some("Disk space running low. Monitor snapshot usage."));
                    } else {
                        // OK: >= 20% free - normal
                        label.set_tooltip_text(Some("Available disk space for snapshots"));
                    }
                } else {
                    label.set_tooltip_text(Some("Available disk space"));
                }
            }
            Err(e) => {
                label.set_text("Space: Unknown");
                label.set_tooltip_text(Some(&format!("Failed to query disk space: {}", e)));
            }
        }
    }

    fn create_status_banner() -> (adw::Banner, bool) {
        let banner = adw::Banner::new("");

        // Check if running on Btrfs
        let is_btrfs = match btrfs::is_btrfs(&std::path::PathBuf::from("/")) {
            Ok(true) => {
                // Btrfs detected - don't show banner
                banner.set_revealed(false);
                true
            }
            Ok(false) => {
                banner.set_title("Btrfs is required to create system restore points");
                banner.set_button_label(Some("Learn More"));
                banner.set_revealed(true);

                // Connect "Learn More" button to open documentation
                banner.connect_button_clicked(|_| {
                    // Open Btrfs wiki page
                    let _ = std::process::Command::new("xdg-open")
                        .arg("https://btrfs.readthedocs.io/")
                        .spawn();
                });

                false
            }
            Err(e) => {
                banner.set_title(&format!("Unable to detect filesystem type: {}", e));
                banner.set_revealed(true);
                false
            }
        };

        (banner, is_btrfs)
    }

    fn refresh_snapshot_list(&self) {
        let window = self.window.clone();
        let manager = self.snapshot_manager.clone();
        let list = self.snapshot_list.clone();
        let compare_btn = self.compare_btn.clone();

        snapshot_list::refresh_snapshot_list_internal(
            &self.window,
            &self.snapshot_manager,
            &self.snapshot_list,
            &self.compare_btn,
            None,  // No search filter
            None,  // No date filter
            None,  // No match label
            move |id, action| {
                Self::handle_snapshot_action(&window, &manager, &list, &compare_btn, id, action);
            },
        );
    }

    fn refresh_with_filter(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        list: &ListBox,
        compare_btn: &Button,
        match_label: &Label,
        search_text: &str,
        date_filter: DateFilter,
    ) {
        let window_clone = window.clone();
        let manager_clone = manager.clone();
        let list_clone = list.clone();
        let compare_btn_clone = compare_btn.clone();

        snapshot_list::refresh_snapshot_list_internal(
            window,
            manager,
            list,
            compare_btn,
            Some(search_text),
            Some(date_filter),
            Some(match_label),
            move |id, action| {
                Self::handle_snapshot_action(&window_clone, &manager_clone, &list_clone, &compare_btn_clone, id, action);
            },
        );
    }

    fn on_create_snapshot(
        window: &adw::ApplicationWindow,
        manager: Rc<RefCell<SnapshotManager>>,
        list: ListBox,
        compare_btn: Button,
    ) {
        // Check if root is on Btrfs (can check without root)
        match btrfs::is_btrfs(&std::path::PathBuf::from("/")) {
            Ok(false) => {
                Self::show_error_dialog(
                    window,
                    "Btrfs Not Detected",
                    "Root filesystem must be Btrfs to create snapshots.",
                );
                return;
            }
            Err(e) => {
                Self::show_error_dialog(window, "Error", &format!("Failed to check filesystem: {}", e));
                return;
            }
            _ => {}
        }

        // Check available disk space in background (can check without root)
        const MIN_SPACE_GB: u64 = 1; // Minimum 1 GB free space
        const MIN_SPACE_BYTES: u64 = MIN_SPACE_GB * 1024 * 1024 * 1024;

        let window_clone = window.clone();
        let list_clone = list.clone();
        let manager_clone = manager.clone();
        let compare_btn_clone = compare_btn.clone();

        // Run disk space check in background
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = btrfs::get_available_space(&std::path::PathBuf::from("/"));
            let _ = tx.send(result);
        });

        // Poll for result and proceed based on available space
        glib::spawn_future_local(async move {
            let space_result = loop {
                match rx.try_recv() {
                    Ok(result) => break result,
                    Err(mpsc::TryRecvError::Empty) => {
                        glib::timeout_future(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        eprintln!("Disk space check thread disconnected");
                        break Ok(MIN_SPACE_BYTES + 1); // Assume sufficient space
                    }
                }
            };

            // Check if we have enough space
            match space_result {
                Ok(available) => {
                    if available < MIN_SPACE_BYTES {
                        let available_gb = available as f64 / (1024.0 * 1024.0 * 1024.0);
                        Self::show_error_dialog(
                            &window_clone,
                            "Insufficient Disk Space",
                            &format!(
                                "Only {:.2} GB available. At least {} GB is recommended for snapshot creation.",
                                available_gb, MIN_SPACE_GB
                            ),
                        );
                        return;
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Could not check available disk space: {}", e);
                    // Continue anyway - this is just a warning
                }
            }

            // Show custom description dialog
            let window_clone2 = window_clone.clone();
            let list_clone2 = list_clone.clone();
            let manager_clone2 = manager_clone.clone();
            let compare_btn_clone2 = compare_btn_clone.clone();

            create_snapshot_dialog::show_create_snapshot_dialog_async(&window_clone, move |result| {
                if let Some((snapshot_name, description)) = result {
                    // User confirmed, create the snapshot
                    Self::create_snapshot_with_description(
                        &window_clone2,
                        manager_clone2.clone(),
                        list_clone2.clone(),
                        compare_btn_clone2.clone(),
                        snapshot_name,
                        description,
                    );
                }
                // If None, user cancelled - do nothing
            });
        });
    }

    fn create_snapshot_with_description(
        window: &adw::ApplicationWindow,
        manager: Rc<RefCell<SnapshotManager>>,
        list: ListBox,
        compare_btn: Button,
        snapshot_name: String,
        description: String,
    ) {
        let window_clone = window.clone();
        let list_clone = list.clone();
        let manager_clone = manager.clone();
        let compare_btn_clone = compare_btn.clone();
        let snapshot_name_clone = snapshot_name.clone();
        let description_clone = description.clone();

        // Show loading state
        dialogs::show_toast(&window_clone, "Creating snapshot...");

        // Create channel for thread communication
        let (sender, receiver) = mpsc::channel();

        // Spawn blocking operation in thread
        std::thread::spawn(move || {
            // Load subvolume configuration
            let subvolume_paths = preferences::load_config();
            let subvolumes: Vec<String> = subvolume_paths
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();

            // Connect to D-Bus helper
            let client = match WaypointHelperClient::new() {
                Ok(c) => c,
                Err(e) => {
                    let error = format!("Failed to connect to snapshot service: {}\n\nTry: sudo sv reload dbus", e);
                    let _ = sender.send((None, Some(("Connection Error".to_string(), error)), vec![]));
                    return;
                }
            };

            // Create snapshot (password prompt happens here)
            let result = client.create_snapshot(snapshot_name_clone, description_clone, subvolumes);

            // Send result back to main thread
            let _ = sender.send((Some((result, client)), None, subvolume_paths));
        });

        // Receive results on main thread
        glib::source::idle_add_local_once(move || {
            if let Ok(msg) = receiver.recv() {
                let (result_opt, error_opt, subvolume_paths) = msg;

                // Handle connection error
                if let Some((title, error)) = error_opt {
                    Self::show_error_dialog(&window_clone, &title, &error);
                    return;
                }

                // Handle snapshot result
                if let Some((result, client)) = result_opt {
                    match result {
                        Ok((true, message)) => {
                            dialogs::show_toast(&window_clone, &message);

                            // Calculate snapshot size and save metadata
                            Self::save_snapshot_metadata(
                                &snapshot_name,
                                &description,
                                &subvolume_paths,
                                &manager_clone,
                            );

                            // Apply retention policy and cleanup old snapshots
                            Self::apply_retention_cleanup(&window_clone, &manager_clone, &client);

                            // Refresh snapshot list
                            Self::refresh_list_static(&window_clone, &manager_clone, &list_clone, &compare_btn_clone);
                        }
                        Ok((false, message)) => {
                            Self::show_error_dialog(&window_clone, "Snapshot Failed", &message);
                        }
                        Err(e) => {
                            let error_msg = e.to_string();
                            if error_msg.contains("NotAuthorized") || error_msg.contains("Dismissed") {
                                Self::show_error_dialog(
                                    &window_clone,
                                    "Authentication Required",
                                    "Administrator privileges are required to create snapshots.\nAuthentication was cancelled."
                                );
                            } else {
                                Self::show_error_dialog(
                                    &window_clone,
                                    "Snapshot Failed",
                                    &format!("Error: {}", error_msg)
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    fn save_snapshot_metadata(
        snapshot_name: &str,
        description: &str,
        subvolume_paths: &[PathBuf],
        manager: &Rc<RefCell<SnapshotManager>>,
    ) {
        // Construct snapshot path
        // Use /.snapshots if mounted, otherwise fall back to /mnt/btrfs-root/@snapshots
        let snapshot_path = if PathBuf::from("/.snapshots").exists() {
            PathBuf::from(format!("/.snapshots/{}", snapshot_name))
        } else {
            PathBuf::from(format!("/mnt/btrfs-root/@snapshots/{}", snapshot_name))
        };

        // Create snapshot metadata without size first (size calculation can be slow)
        let snapshot = Snapshot {
            id: snapshot_name.to_string(),
            name: snapshot_name.to_string(),
            timestamp: chrono::Utc::now(),
            path: snapshot_path.clone(),
            description: Some(description.to_string()),
            kernel_version: None, // Could add this later
            package_count: None,  // Could add this later
            size_bytes: None,     // Will be calculated in background
            packages: Rc::new(Vec::new()),
            subvolumes: Rc::new(subvolume_paths.to_vec()),
        };

        // Save metadata immediately
        if let Err(e) = manager.borrow().add_snapshot(snapshot) {
            eprintln!("Warning: Failed to save snapshot metadata: {}", e);
        }

        // Calculate snapshot size in background thread (non-blocking)
        let snapshot_name_clone = snapshot_name.to_string();
        let manager_clone = manager.clone();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let size_result = btrfs::get_snapshot_size(&snapshot_path);
            let _ = tx.send((snapshot_name_clone, size_result));
        });

        // Poll for result and update metadata when available
        glib::spawn_future_local(async move {
            loop {
                match rx.try_recv() {
                    Ok((name, size_result)) => {
                        match size_result {
                            Ok(size) => {
                                eprintln!("Calculated snapshot size: {} bytes", size);
                                // Update snapshot with size
                                if let Ok(Some(mut snapshot)) = manager_clone.borrow().get_snapshot(&name) {
                                    snapshot.size_bytes = Some(size);
                                    if let Err(e) = manager_clone.borrow().add_snapshot(snapshot) {
                                        eprintln!("Warning: Failed to update snapshot size: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Warning: Failed to calculate snapshot size: {}", e);
                            }
                        }
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        glib::timeout_future(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        eprintln!("Size calculation thread disconnected");
                        break;
                    }
                }
            }
        });
    }

    fn apply_retention_cleanup(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        client: &WaypointHelperClient,
    ) {
        // Get list of snapshots to clean up based on retention policy
        let to_delete = match manager.borrow().get_snapshots_to_cleanup() {
            Ok(list) => list,
            Err(e) => {
                eprintln!("Warning: Failed to check retention policy: {}", e);
                return;
            }
        };

        if to_delete.is_empty() {
            return;
        }

        // Delete old snapshots
        let delete_count = to_delete.len();
        for snapshot_name in to_delete {
            match client.delete_snapshot(snapshot_name.clone()) {
                Ok((true, _)) => {
                    eprintln!("Retention policy: deleted snapshot {}", snapshot_name);
                }
                Ok((false, msg)) => {
                    eprintln!("Warning: Failed to delete snapshot {}: {}", snapshot_name, msg);
                }
                Err(e) => {
                    eprintln!("Warning: Error deleting snapshot {}: {}", snapshot_name, e);
                }
            }
        }

        // Show notification about cleanup
        if delete_count > 0 {
            let message = format!("Retention policy: cleaned up {} old snapshot{}",
                delete_count,
                if delete_count == 1 { "" } else { "s" }
            );
            dialogs::show_toast(window, &message);
        }
    }

    fn refresh_list_static(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        list: &ListBox,
        compare_btn: &Button,
    ) {
        let window_clone = window.clone();
        let manager_clone = manager.clone();
        let list_clone = list.clone();
        let compare_btn_clone = compare_btn.clone();

        snapshot_list::refresh_snapshot_list_internal(
            window,
            manager,
            list,
            compare_btn,
            None,  // No search filter
            None,  // No date filter
            None,  // No match label
            move |id, action| {
                Self::handle_snapshot_action(&window_clone, &manager_clone, &list_clone, &compare_btn_clone, id, action);
            },
        );
    }

    fn show_error_dialog(window: &adw::ApplicationWindow, title: &str, message: &str) {
        dialogs::show_error(window, title, message);
    }

    fn handle_snapshot_action(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        list: &ListBox,
        compare_btn: &Button,
        snapshot_id: &str,
        action: SnapshotAction,
    ) {
        match action {
            SnapshotAction::Browse => {
                Self::browse_snapshot(window, manager, snapshot_id);
            }
            SnapshotAction::Verify => {
                Self::verify_snapshot(window, snapshot_id);
            }
            SnapshotAction::Restore => {
                Self::restore_snapshot(window, manager, list, snapshot_id);
            }
            SnapshotAction::Delete => {
                Self::delete_snapshot(window, manager, list, compare_btn, snapshot_id);
            }
        }
    }

    fn verify_snapshot(window: &adw::ApplicationWindow, snapshot_id: &str) {
        let window_clone = window.clone();
        let snapshot_id_owned = snapshot_id.to_string();

        // Run verification in background thread
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<crate::dbus_client::VerificationResult> {
                let client = WaypointHelperClient::new()?;
                client.verify_snapshot(snapshot_id_owned)
            })();
            let _ = tx.send(result);
        });

        // Poll for result
        glib::spawn_future_local(async move {
            let result = loop {
                match rx.try_recv() {
                    Ok(result) => break result,
                    Err(mpsc::TryRecvError::Empty) => {
                        glib::timeout_future(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Self::show_error_dialog(
                            &window_clone,
                            "Verification Failed",
                            "Verification thread disconnected unexpectedly",
                        );
                        return;
                    }
                }
            };

            match result {
                Ok(verification) => {
                    if verification.is_valid {
                        let mut message = "✓ Snapshot is valid and intact".to_string();
                        if !verification.warnings.is_empty() {
                            message.push_str("\n\nWarnings:\n");
                            for warning in &verification.warnings {
                                message.push_str(&format!("• {}\n", warning));
                            }
                        }

                        let dialog = adw::MessageDialog::new(
                            Some(&window_clone),
                            Some("Verification Successful"),
                            Some(&message),
                        );
                        dialog.add_response("ok", "OK");
                        dialog.set_default_response(Some("ok"));
                        dialog.present();
                    } else {
                        let mut message = "✗ Snapshot verification failed\n\nErrors found:\n".to_string();
                        for error in &verification.errors {
                            message.push_str(&format!("• {}\n", error));
                        }

                        if !verification.warnings.is_empty() {
                            message.push_str("\nWarnings:\n");
                            for warning in &verification.warnings {
                                message.push_str(&format!("• {}\n", warning));
                            }
                        }

                        Self::show_error_dialog(
                            &window_clone,
                            "Verification Failed",
                            &message,
                        );
                    }
                }
                Err(e) => {
                    Self::show_error_dialog(
                        &window_clone,
                        "Verification Error",
                        &format!("Failed to verify snapshot: {}", e),
                    );
                }
            }
        });
    }

    fn browse_snapshot(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        snapshot_id: &str,
    ) {
        let snapshot = match manager.borrow().get_snapshot(snapshot_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                dialogs::show_error(window, "Not Found", "Snapshot not found");
                return;
            }
            Err(e) => {
                dialogs::show_error(window, "Error", &format!("Failed to load snapshot: {}", e));
                return;
            }
        };

        // Validate path before opening
        if let Err(e) = validate_path_for_open(&snapshot.path) {
            dialogs::show_error(
                window,
                "Cannot Open Path",
                &format!("Security validation failed: {}", e),
            );
            return;
        }

        // Open file manager at snapshot path
        let path_str = snapshot.path.to_string_lossy();
        let result = std::process::Command::new("xdg-open")
            .arg(path_str.as_ref())
            .spawn();

        match result {
            Ok(_) => {
                dialogs::show_toast(window, "Opening file manager...");
            }
            Err(e) => {
                dialogs::show_error(
                    window,
                    "Failed to Open",
                    &format!("Could not open file manager: {}", e),
                );
            }
        }
    }

    fn delete_snapshot(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        list: &ListBox,
        compare_btn: &Button,
        snapshot_id: &str,
    ) {
        let snapshot = match manager.borrow().get_snapshot(snapshot_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                dialogs::show_error(window, "Not Found", "Snapshot not found");
                return;
            }
            Err(e) => {
                dialogs::show_error(window, "Error", &format!("Failed to load snapshot: {}", e));
                return;
            }
        };

        let snapshot_name = snapshot.name.clone();
        // Extract just the snapshot name without the @snapshots/ prefix
        let snapshot_basename = snapshot.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&snapshot_name)
            .to_string();

        let window_clone = window.clone();
        let manager_clone = manager.clone();
        let list_clone = list.clone();
        let compare_btn_clone = compare_btn.clone();

        dialogs::show_confirmation(
            window,
            "Delete Snapshot?",
            &format!(
                "Are you sure you want to delete '{}'?\n\nThis action cannot be undone.",
                snapshot_name
            ),
            "Delete",
            true,
            move || {
                let window = window_clone.clone();
                let manager = manager_clone.clone();
                let list = list_clone.clone();
                let compare_btn = compare_btn_clone.clone();
                let name = snapshot_basename.clone();

                // Show loading state
                dialogs::show_toast(&window, "Deleting snapshot...");

                // Create channel for thread communication
                let (sender, receiver) = mpsc::channel();

                // Spawn blocking operation in thread
                std::thread::spawn(move || {
                    // Connect to D-Bus helper
                    let client = match WaypointHelperClient::new() {
                        Ok(c) => c,
                        Err(e) => {
                            let error = format!("Failed to connect to snapshot service: {}", e);
                            let _ = sender.send((None, Some(("Connection Error".to_string(), error))));
                            return;
                        }
                    };

                    // Delete snapshot via D-Bus
                    let result = client.delete_snapshot(name);

                    // Send result back to main thread
                    let _ = sender.send((Some(result), None));
                });

                // Receive results on main thread
                glib::source::idle_add_local_once(move || {
                    if let Ok(msg) = receiver.recv() {
                        let (result_opt, error_opt) = msg;

                        // Handle connection error
                        if let Some((title, error)) = error_opt {
                            dialogs::show_error(&window, &title, &error);
                            return;
                        }

                        // Handle delete result
                        if let Some(result) = result_opt {
                            match result {
                                Ok((true, message)) => {
                                    dialogs::show_toast(&window, &message);
                                    // Refresh the list
                                    Self::refresh_list_static(&window, &manager, &list, &compare_btn);
                                }
                                Ok((false, message)) => {
                                    dialogs::show_error(&window, "Deletion Failed", &message);
                                }
                                Err(e) => {
                                    let error_msg = e.to_string();
                                    if error_msg.contains("NotAuthorized") || error_msg.contains("Dismissed") {
                                        dialogs::show_error(
                                            &window,
                                            "Authentication Required",
                                            "Administrator privileges are required.\nAuthentication was cancelled."
                                        );
                                    } else {
                                        dialogs::show_error(&window, "Deletion Failed", &format!("Error: {}", error_msg));
                                    }
                                }
                            }
                        }
                    }
                });
            },
        );
    }

    fn restore_snapshot(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        _list: &ListBox,
        snapshot_id: &str,
    ) {
        let snapshot = match manager.borrow().get_snapshot(snapshot_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                dialogs::show_error(window, "Not Found", "Snapshot not found");
                return;
            }
            Err(e) => {
                dialogs::show_error(window, "Error", &format!("Failed to load snapshot: {}", e));
                return;
            }
        };

        // Extract snapshot basename for D-Bus call
        let snapshot_basename = snapshot.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&snapshot.name)
            .to_string();

        let window_clone = window.clone();
        let snapshot_id_owned = snapshot_basename.clone();

        // Show loading toast while fetching preview
        dialogs::show_toast(window, "Loading restore preview...");

        // Create channel for background thread communication
        let (tx, rx) = mpsc::channel();

        // Fetch preview in background thread
        std::thread::spawn(move || {
            let client = match WaypointHelperClient::new() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(anyhow::anyhow!("Failed to connect to snapshot service: {}", e)));
                    return;
                }
            };

            let result = client.preview_restore(snapshot_id_owned);
            let _ = tx.send(result);
        });

        // Poll for preview result
        glib::source::idle_add_local(move || {
            match rx.try_recv() {
                Ok(Ok(preview)) => {
                    // Show preview dialog with package changes
                    Self::show_restore_preview_dialog(&window_clone, &snapshot_basename, preview);
                    glib::ControlFlow::Break
                }
                Ok(Err(e)) => {
                    dialogs::show_error(&window_clone, "Preview Failed",
                        &format!("Failed to generate restore preview: {}", e));
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    dialogs::show_error(&window_clone, "Error", "Preview thread disconnected");
                    glib::ControlFlow::Break
                }
            }
        });
    }

    fn show_restore_preview_dialog(
        window: &adw::ApplicationWindow,
        snapshot_basename: &str,
        preview: crate::dbus_client::RestorePreview,
    ) {
        // Build comprehensive preview message
        let mut preview_parts = Vec::new();

        // Header info
        preview_parts.push(format!(
            "📸 Snapshot: {}\n📅 Created: {}",
            preview.snapshot_name,
            preview.snapshot_timestamp
        ));

        if let Some(desc) = &preview.snapshot_description {
            preview_parts.push(format!("📝 {}", desc));
        }

        // Kernel changes
        let kernel_current = preview.current_kernel.as_deref().unwrap_or("unknown");
        let kernel_snapshot = preview.snapshot_kernel.as_deref().unwrap_or("unknown");
        if kernel_current != kernel_snapshot {
            preview_parts.push(format!(
                "\n🐧 Kernel: {} → {}",
                kernel_current, kernel_snapshot
            ));
        } else {
            preview_parts.push(format!("\n🐧 Kernel: {} (no change)", kernel_current));
        }

        // Package changes summary
        preview_parts.push(format!("\n📦 Package Changes: {}", preview.total_package_changes));

        if !preview.packages_to_add.is_empty() {
            preview_parts.push(format!("  ➕ {} to install", preview.packages_to_add.len()));
            // Show first few examples
            for pkg in preview.packages_to_add.iter().take(3) {
                let version = pkg.snapshot_version.as_deref().unwrap_or("unknown");
                preview_parts.push(format!("     • {} ({})", pkg.name, version));
            }
            if preview.packages_to_add.len() > 3 {
                preview_parts.push(format!("     • ... and {} more", preview.packages_to_add.len() - 3));
            }
        }
        if !preview.packages_to_remove.is_empty() {
            preview_parts.push(format!("  ➖ {} to remove", preview.packages_to_remove.len()));
            for pkg in preview.packages_to_remove.iter().take(3) {
                let version = pkg.current_version.as_deref().unwrap_or("unknown");
                preview_parts.push(format!("     • {} ({})", pkg.name, version));
            }
            if preview.packages_to_remove.len() > 3 {
                preview_parts.push(format!("     • ... and {} more", preview.packages_to_remove.len() - 3));
            }
        }
        if !preview.packages_to_upgrade.is_empty() {
            preview_parts.push(format!("  ⬆️  {} to upgrade", preview.packages_to_upgrade.len()));
            for pkg in preview.packages_to_upgrade.iter().take(3) {
                let curr = pkg.current_version.as_deref().unwrap_or("?");
                let snap = pkg.snapshot_version.as_deref().unwrap_or("?");
                preview_parts.push(format!("     • {} ({} → {})", pkg.name, curr, snap));
            }
            if preview.packages_to_upgrade.len() > 3 {
                preview_parts.push(format!("     • ... and {} more", preview.packages_to_upgrade.len() - 3));
            }
        }
        if !preview.packages_to_downgrade.is_empty() {
            preview_parts.push(format!("  ⬇️  {} to downgrade", preview.packages_to_downgrade.len()));
            for pkg in preview.packages_to_downgrade.iter().take(3) {
                let curr = pkg.current_version.as_deref().unwrap_or("?");
                let snap = pkg.snapshot_version.as_deref().unwrap_or("?");
                preview_parts.push(format!("     • {} ({} → {})", pkg.name, curr, snap));
            }
            if preview.packages_to_downgrade.len() > 3 {
                preview_parts.push(format!("     • ... and {} more", preview.packages_to_downgrade.len() - 3));
            }
        }

        // Affected subvolumes
        if !preview.affected_subvolumes.is_empty() {
            preview_parts.push(format!("\n💾 Affected: {}", preview.affected_subvolumes.join(", ")));
        }

        // Warning footer
        preview_parts.push(
            "\n⚠️  WARNING:\n\
            • All changes after this snapshot will be LOST\n\
            • System will require a REBOOT\n\
            • A backup snapshot will be created first".to_string()
        );

        let preview_message = preview_parts.join("\n");

        let dialog = adw::MessageDialog::new(
            Some(window),
            Some("Restore System Snapshot?"),
            Some(&preview_message),
        );

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("restore", "Restore and Reboot");
        dialog.set_response_appearance("restore", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let window_clone = window.clone();
        let snapshot_name = snapshot_basename.to_string();

        dialog.connect_response(None, move |_, response| {
            if response == "restore" {
                let window = window_clone.clone();
                let name = snapshot_name.clone();

                // Show loading state
                dialogs::show_toast(&window, "Restoring snapshot...");

                // Create channel for thread communication
                let (sender, receiver) = mpsc::channel();

                // Spawn blocking operation in thread
                std::thread::spawn(move || {
                    // Connect to D-Bus helper
                    let client = match WaypointHelperClient::new() {
                        Ok(c) => c,
                        Err(e) => {
                            let error = format!("Failed to connect to snapshot service: {}", e);
                            let _ = sender.send((None, Some(("Connection Error".to_string(), error))));
                            return;
                        }
                    };

                    // Restore snapshot via D-Bus (password prompt happens here)
                    let result = client.restore_snapshot(name);

                    // Send result back to main thread
                    let _ = sender.send((Some(result), None));
                });

                // Receive results on main thread
                glib::source::idle_add_local_once(move || {
                    if let Ok(msg) = receiver.recv() {
                        let (result_opt, error_opt) = msg;

                        // Handle connection error
                        if let Some((title, error)) = error_opt {
                            dialogs::show_error(&window, &title, &error);
                            return;
                        }

                        // Handle restore result
                        if let Some(result) = result_opt {
                            match result {
                                Ok((true, message)) => {
                                    // Show success message with reboot instructions
                                    let success_dialog = adw::MessageDialog::new(
                                        Some(&window),
                                        Some("Rollback Successful"),
                                        Some(&format!(
                                            "{}\n\n\
                                            You MUST reboot for the changes to take effect.\n\n\
                                            After reboot, your system will be restored to the snapshot state.\n\n\
                                            Reboot now?",
                                            message
                                        )),
                                    );

                                    success_dialog.add_response("later", "Reboot Later");
                                    success_dialog.add_response("now", "Reboot Now");
                                    success_dialog.set_response_appearance("now", adw::ResponseAppearance::Suggested);
                                    success_dialog.set_default_response(Some("now"));
                                    success_dialog.set_close_response("later");

                                    success_dialog.connect_response(None, |_, response| {
                                        if response == "now" {
                                            // Attempt to reboot
                                            let _ = std::process::Command::new("reboot")
                                                .spawn();
                                        }
                                    });

                                    success_dialog.present();
                                }
                                Ok((false, message)) => {
                                    dialogs::show_error(
                                        &window,
                                        "Rollback Failed",
                                        &format!("{}\n\nYour system has NOT been changed.", message)
                                    );
                                }
                                Err(e) => {
                                    let error_msg = e.to_string();
                                    if error_msg.contains("NotAuthorized") || error_msg.contains("Dismissed") {
                                        dialogs::show_error(
                                            &window,
                                            "Authentication Required",
                                            "Administrator privileges are required.\nAuthentication was cancelled."
                                        );
                                    } else {
                                        dialogs::show_error(
                                            &window,
                                            "Rollback Failed",
                                            &format!("Error: {}\n\nYour system has NOT been changed.", error_msg)
                                        );
                                    }
                                }
                            }
                        }
                    }
                });
            }
        });

        dialog.present();
    }

    /// Show dialog to compare two snapshots
    fn show_compare_dialog(window: &adw::ApplicationWindow, manager: &Rc<RefCell<SnapshotManager>>) {
        let snapshots = match manager.borrow().load_snapshots() {
            Ok(s) => s,
            Err(e) => {
                dialogs::show_error(window, "Error", &format!("Failed to load snapshots: {}", e));
                return;
            }
        };

        if snapshots.len() < 2 {
            dialogs::show_error(
                window,
                "Not Enough Snapshots",
                "You need at least 2 snapshots to compare.\n\nCreate more snapshots first.",
            );
            return;
        }

        // Create selection dialog
        let dialog = adw::MessageDialog::new(
            Some(window),
            Some("Compare Snapshots"),
            Some("Select two snapshots to compare their packages:"),
        );

        // Add snapshot list as custom widget
        let content = gtk::Box::new(Orientation::Vertical, 12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);

        // First snapshot dropdown
        let label1 = Label::new(Some("First Snapshot (older):"));
        label1.set_halign(gtk::Align::Start);
        content.append(&label1);

        let snapshot_names: Vec<String> = snapshots
            .iter()
            .map(|s| format!("{} - {}", s.name, s.format_timestamp()))
            .collect();

        let snapshot_strs: Vec<&str> = snapshot_names.iter().map(|s| s.as_str()).collect();
        let dropdown1 = gtk::DropDown::from_strings(&snapshot_strs);
        content.append(&dropdown1);

        // Second snapshot dropdown
        let label2 = Label::new(Some("Second Snapshot (newer):"));
        label2.set_halign(gtk::Align::Start);
        label2.set_margin_top(12);
        content.append(&label2);

        let dropdown2 = gtk::DropDown::from_strings(&snapshot_strs);
        // Select last snapshot by default
        if !snapshots.is_empty() {
            dropdown2.set_selected(snapshots.len() as u32 - 1);
        }
        content.append(&dropdown2);

        dialog.set_extra_child(Some(&content));

        dialog.add_response("cancel", "Cancel");
        dialog.add_response("compare", "Compare");
        dialog.set_response_appearance("compare", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("compare"));
        dialog.set_close_response("cancel");

        let window_clone = window.clone();
        let snapshots_clone = snapshots.clone();

        dialog.connect_response(None, move |_, response| {
            if response == "compare" {
                let idx1 = dropdown1.selected() as usize;
                let idx2 = dropdown2.selected() as usize;

                if idx1 == idx2 {
                    dialogs::show_error(
                        &window_clone,
                        "Same Snapshot",
                        "Please select two different snapshots to compare.",
                    );
                    return;
                }

                let snap1 = &snapshots_clone[idx1];
                let snap2 = &snapshots_clone[idx2];

                // Show the comparison
                package_diff_dialog::show_package_diff_dialog(
                    &window_clone,
                    &snap1.name,
                    &snap1.packages,
                    &snap2.name,
                    &snap2.packages,
                );
            }
        });

        dialog.present();
    }

    /// Show statistics dialog
    fn show_statistics_dialog(window: &adw::ApplicationWindow, manager: &Rc<RefCell<SnapshotManager>>) {
        statistics_dialog::show_statistics_dialog(window, manager);
    }

    /// Show preferences dialog for subvolume selection
    fn show_preferences_dialog(window: &adw::ApplicationWindow) {
        // Load current configuration
        let current_config = preferences::load_config();

        // Show preferences dialog
        let prefs = preferences::show_preferences_dialog(window, current_config);

        // The dialog will be shown immediately and preferences will be saved
        // when the user closes it. We save on close by connecting to the dialog's
        // close signal in a more complete implementation. For now, we save
        // whenever the checkboxes change, which happens in preferences.rs.
        //
        // Save the current preferences
        if let Err(e) = preferences::save_config(&prefs.get_enabled()) {
            eprintln!("Failed to save preferences: {}", e);
        }
    }

    fn show_about_dialog(window: &adw::ApplicationWindow) {
        let dialog = adw::Window::new();
        dialog.set_title(Some("About Waypoint"));
        dialog.set_default_size(400, 500);
        dialog.set_modal(true);
        dialog.set_transient_for(Some(window));

        // Create header bar with close button
        let header = adw::HeaderBar::new();
        header.set_show_end_title_buttons(true);

        // Main container
        let main_box = gtk::Box::new(Orientation::Vertical, 0);
        main_box.append(&header);

        // Main content box
        let content = gtk::Box::new(Orientation::Vertical, 24);
        content.set_margin_top(48);
        content.set_margin_bottom(48);
        content.set_margin_start(48);
        content.set_margin_end(48);
        content.set_valign(gtk::Align::Center);

        // Application icon
        let icon = if let Ok(icon_path) = std::fs::canonicalize("assets/icons/hicolor/scalable/waypoint.svg") {
            gtk::Image::from_file(&icon_path)
        } else {
            gtk::Image::from_icon_name("waypoint")
        };
        icon.set_pixel_size(96);
        content.append(&icon);

        // Application name
        let name_label = Label::new(Some("Waypoint"));
        name_label.add_css_class("title-1");
        content.append(&name_label);

        // Version
        let version_label = Label::new(Some(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
        version_label.add_css_class("dim-label");
        content.append(&version_label);

        // Description
        let description = Label::new(Some(
            "A GTK-based snapshot and rollback tool for Btrfs filesystems on Void Linux."
        ));
        description.set_wrap(true);
        description.set_justify(gtk::Justification::Center);
        description.set_max_width_chars(40);
        content.append(&description);

        // Links section
        let links_box = gtk::Box::new(Orientation::Vertical, 12);
        links_box.set_margin_top(12);

        // GitHub link
        let github_btn = gtk::Button::with_label("View on GitHub");
        github_btn.add_css_class("flat");
        github_btn.connect_clicked(|_| {
            let _ = std::process::Command::new("xdg-open")
                .arg("https://github.com/Letdown2491/waypoint-gtk/")
                .spawn();
        });
        links_box.append(&github_btn);

        // Report issue link
        let issue_btn = gtk::Button::with_label("Report an issue");
        issue_btn.add_css_class("flat");
        issue_btn.connect_clicked(|_| {
            let _ = std::process::Command::new("xdg-open")
                .arg("https://github.com/Letdown2491/waypoint-gtk/issues")
                .spawn();
        });
        links_box.append(&issue_btn);

        content.append(&links_box);

        main_box.append(&content);
        dialog.set_content(Some(&main_box));
        dialog.present();
    }

    #[allow(dead_code)]
    pub fn present(&self) {
        self.window.present();
    }
}
