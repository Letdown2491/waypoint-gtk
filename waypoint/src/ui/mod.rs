mod about_preferences;
mod analytics_dialog;
mod backup_dialog;
mod comparison_dialog;
mod comparison_view;
mod create_snapshot_dialog;
mod dialogs;
mod error_helpers;
mod exclude_preferences;
mod file_diff_dialog;
mod file_restore_dialog;
pub mod notifications;
mod package_diff_dialog;
pub mod preferences;
mod preferences_window;
mod quota_preferences;
mod schedule_card;
mod schedule_edit_dialog;
mod scheduler_dialog;
mod snapshot_list;
mod snapshot_row;
mod toolbar;
mod validation;

use crate::backup_manager::BackupManager;
use crate::btrfs;
use crate::dbus_client::WaypointHelperClient;
use crate::snapshot::{Snapshot, SnapshotManager};
use crate::user_preferences::UserPreferencesManager;
use adw::prelude::*;
use anyhow::Context;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Application, Button, Label, ListBox, Orientation, ScrolledWindow, SearchEntry, ToggleButton,
};
use libadwaita as adw;
use snapshot_row::SnapshotAction;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

use snapshot_list::DateFilter;

// Path validation moved to validation module

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
enum DriveType {
    Removable,
    Network,
    Internal,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct BackupDestination {
    mount_point: String,
    label: String,
    drive_type: DriveType,
    uuid: Option<String>,
}

pub struct MainWindow {
    window: adw::ApplicationWindow,
    snapshot_manager: Rc<RefCell<SnapshotManager>>,
    user_prefs_manager: Rc<RefCell<UserPreferencesManager>>,
    backup_manager: Rc<RefCell<BackupManager>>,
    snapshot_list: ListBox,
    create_btn: Button,
    compare_btn: Button,
    disk_space_label: Label,
    disk_space_bar: gtk::LevelBar,
    _search_entry: SearchEntry,
    _match_label: Label,
    _date_filter: Rc<RefCell<DateFilter>>,
}

impl MainWindow {
    pub fn new(app: &Application) -> adw::ApplicationWindow {
        let snapshot_manager = match SnapshotManager::new() {
            Ok(sm) => Rc::new(RefCell::new(sm)),
            Err(e) => {
                log::error!("Failed to initialize snapshot manager: {}", e);

                // Create a temporary window to show the error dialog
                let temp_window = adw::ApplicationWindow::builder().application(app).build();

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
                    )),
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

        // Initialize user preferences manager
        let user_prefs_manager = match UserPreferencesManager::new() {
            Ok(pm) => Rc::new(RefCell::new(pm)),
            Err(e) => {
                log::error!("Failed to initialize user preferences manager: {}", e);
                log::warn!("User preferences (favorites, notes) will not be saved");
                // Continue anyway with a fallback - create temp manager
                Rc::new(RefCell::new(UserPreferencesManager::new().unwrap_or_else(
                    |_| panic!("Could not create user preferences manager"),
                )))
            }
        };

        // Initialize backup manager
        let backup_manager = match BackupManager::new() {
            Ok(bm) => Rc::new(RefCell::new(bm)),
            Err(e) => {
                log::error!("Failed to initialize backup manager: {}", e);
                log::warn!("Automatic backups will not be available");
                // Continue anyway - backups are optional
                Rc::new(RefCell::new(
                    BackupManager::new()
                        .unwrap_or_else(|_| panic!("Could not create backup manager")),
                ))
            }
        };

        // Create header bar
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new("Waypoint", "")));

        // Add application icon to header bar
        let app_icon = if let Ok(icon_path) =
            std::fs::canonicalize("assets/icons/hicolor/scalable/waypoint.svg")
        {
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
            .width_request(220)
            .build();

        // Theme section (using ListBox for proper styling)
        let theme_list = ListBox::new();
        theme_list.set_selection_mode(gtk::SelectionMode::None);
        theme_list.add_css_class("boxed-list");

        let theme_row = adw::ActionRow::builder().title("Switch theme").build();

        // Theme buttons
        let theme_buttons_box = gtk::Box::new(Orientation::Horizontal, 12);
        theme_buttons_box.set_valign(gtk::Align::Center);

        let system_btn = gtk::Button::builder()
            .label("")
            .tooltip_text("Match system theme")
            .width_request(16)
            .height_request(16)
            .build();
        system_btn.add_css_class("flat");
        system_btn.add_css_class("theme-circle");
        system_btn.add_css_class("theme-circle-system");

        let light_btn = gtk::Button::builder()
            .label("")
            .tooltip_text("Light theme")
            .width_request(16)
            .height_request(16)
            .build();
        light_btn.add_css_class("flat");
        light_btn.add_css_class("theme-circle");
        light_btn.add_css_class("theme-circle-light");

        let dark_btn = gtk::Button::builder()
            .label("")
            .tooltip_text("Dark theme")
            .width_request(16)
            .height_request(16)
            .build();
        dark_btn.add_css_class("flat");
        dark_btn.add_css_class("theme-circle");
        dark_btn.add_css_class("theme-circle-dark");

        system_btn.set_hexpand(false);
        system_btn.set_vexpand(false);
        system_btn.set_valign(gtk::Align::Center);
        light_btn.set_hexpand(false);
        light_btn.set_vexpand(false);
        light_btn.set_valign(gtk::Align::Center);
        dark_btn.set_hexpand(false);
        dark_btn.set_vexpand(false);
        dark_btn.set_valign(gtk::Align::Center);

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

        let analytics_row = adw::ActionRow::builder()
            .title("Analytics")
            .activatable(true)
            .build();
        menu_list.append(&analytics_row);

        let cleanup_row = adw::ActionRow::builder()
            .title("Clean Up Old Snapshots")
            .activatable(true)
            .build();
        menu_list.append(&cleanup_row);

        let preferences_row = adw::ActionRow::builder()
            .title("Preferences")
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

        // Status banner - also returns whether Btrfs is available
        let (banner, is_btrfs) = Self::create_status_banner();

        // Toolbar with buttons
        let (toolbar, create_btn, compare_btn, search_btn) = toolbar::create_toolbar();

        // Disable create button if not on Btrfs
        if !is_btrfs {
            create_btn.set_sensitive(false);
            create_btn.set_tooltip_text(Some("Btrfs filesystem required"));
        }

        // Search and filter UI (wrapped in Revealer for smooth animations)
        let search_revealer = gtk::Revealer::new();
        search_revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        search_revealer.set_transition_duration(200); // 200ms animation
        search_revealer.set_reveal_child(false); // Hidden by default

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

        // Add search box to revealer
        search_revealer.set_child(Some(&search_box));

        // Snapshot list
        let snapshot_list = ListBox::new();
        snapshot_list.set_selection_mode(gtk::SelectionMode::None);
        snapshot_list.add_css_class("boxed-list");

        let scrolled = ScrolledWindow::new();
        scrolled.set_vexpand(true);
        scrolled.set_child(Some(&snapshot_list));

        // Add margins around snapshot list
        scrolled.set_margin_top(6);
        scrolled.set_margin_bottom(12);
        scrolled.set_margin_start(12);
        scrolled.set_margin_end(12);

        // Disk space indicator footer with progress bar
        let disk_space_box = gtk::Box::new(Orientation::Vertical, 6);
        disk_space_box.set_halign(gtk::Align::Center);
        disk_space_box.set_margin_top(6);
        disk_space_box.set_margin_bottom(12);

        let disk_space_bar = gtk::LevelBar::new();
        disk_space_bar.set_min_value(0.0);
        disk_space_bar.set_max_value(1.0);
        disk_space_bar.set_width_request(300);
        disk_space_bar.set_halign(gtk::Align::Center);

        // Add offset markers for color coding
        disk_space_bar.add_offset_value("full", 0.9); // > 90% used = critical
        disk_space_bar.add_offset_value("high", 0.8); // > 80% used = warning

        let disk_space_label = Label::new(Some("Checking space..."));
        disk_space_label.add_css_class("caption");
        disk_space_label.add_css_class("dim-label");
        disk_space_label.set_halign(gtk::Align::Center);

        disk_space_box.append(&disk_space_bar);
        disk_space_box.append(&disk_space_label);

        // Main content box
        let content_box = gtk::Box::new(Orientation::Vertical, 0);
        content_box.append(&banner);
        content_box.append(&toolbar);
        content_box.append(&search_revealer);
        content_box.append(&scrolled);
        content_box.append(&disk_space_box);

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
            .default_height(720)
            .content(&toast_overlay)
            .build();

        // Add Ctrl+F keyboard shortcut to open search
        let window_key_controller = gtk::EventControllerKey::new();
        let revealer_for_shortcut = search_revealer.clone();
        let search_entry_for_shortcut = search_entry.clone();
        let search_btn_for_shortcut = search_btn.clone();

        window_key_controller.connect_key_pressed(move |_, key, _code, modifier| {
            // Check for Ctrl+F (Cmd+F on macOS)
            let is_ctrl_f =
                key == gtk::gdk::Key::f && modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK);

            if is_ctrl_f && !revealer_for_shortcut.reveals_child() {
                // Open search
                revealer_for_shortcut.set_reveal_child(true);
                search_btn_for_shortcut.add_css_class("suggested-action");
                search_entry_for_shortcut.grab_focus();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(window_key_controller);

        let date_filter = Rc::new(RefCell::new(DateFilter::All));

        let main_window = Self {
            window: window.clone(),
            snapshot_manager: snapshot_manager.clone(),
            user_prefs_manager: user_prefs_manager.clone(),
            backup_manager: backup_manager.clone(),
            snapshot_list: snapshot_list.clone(),
            create_btn: create_btn.clone(),
            compare_btn: compare_btn.clone(),
            disk_space_label: disk_space_label.clone(),
            disk_space_bar: disk_space_bar.clone(),
            _search_entry: search_entry.clone(),
            _match_label: match_label.clone(),
            _date_filter: date_filter.clone(),
        };

        // Load snapshots and update button states
        main_window.refresh_snapshot_list();

        // Connect search entry to filter snapshots
        let win_clone_search = window.clone();
        let sm_clone_search = snapshot_manager.clone();
        let up_clone_search = user_prefs_manager.clone();
        let bm_clone_search = backup_manager.clone();
        let list_clone_search = snapshot_list.clone();
        let compare_btn_clone_search = compare_btn.clone();
        let disk_space_clone_search = disk_space_label.clone();
        let disk_space_bar_clone_search = disk_space_bar.clone();
        let match_label_clone = match_label.clone();
        let date_filter_clone = date_filter.clone();

        search_entry.connect_search_changed(move |entry| {
            let search_text = entry.text().to_string();
            Self::refresh_with_filter(
                &win_clone_search,
                &sm_clone_search,
                &up_clone_search,
                &bm_clone_search,
                &list_clone_search,
                &compare_btn_clone_search,
                &disk_space_clone_search,
                &disk_space_bar_clone_search,
                &match_label_clone,
                &search_text,
                *date_filter_clone.borrow(),
            );
        });

        // Connect date filter buttons
        let win_clone_all = window.clone();
        let sm_clone_all = snapshot_manager.clone();
        let up_clone_all = user_prefs_manager.clone();
        let bm_clone_all = backup_manager.clone();
        let list_clone_all = snapshot_list.clone();
        let compare_btn_clone_all = compare_btn.clone();
        let disk_space_clone_all = disk_space_label.clone();
        let disk_space_bar_clone_all = disk_space_bar.clone();
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
                    &up_clone_all,
                    &bm_clone_all,
                    &list_clone_all,
                    &compare_btn_clone_all,
                    &disk_space_clone_all,
                    &disk_space_bar_clone_all,
                    &match_label_clone_all,
                    &search_text,
                    DateFilter::All,
                );
            }
        });

        let win_clone_week = window.clone();
        let sm_clone_week = snapshot_manager.clone();
        let up_clone_week = user_prefs_manager.clone();
        let bm_clone_week = backup_manager.clone();
        let list_clone_week = snapshot_list.clone();
        let compare_btn_clone_week = compare_btn.clone();
        let disk_space_clone_week = disk_space_label.clone();
        let disk_space_bar_clone_week = disk_space_bar.clone();
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
                    &up_clone_week,
                    &bm_clone_week,
                    &list_clone_week,
                    &compare_btn_clone_week,
                    &disk_space_clone_week,
                    &disk_space_bar_clone_week,
                    &match_label_clone_week,
                    &search_text,
                    DateFilter::Last7Days,
                );
            }
        });

        let win_clone_month = window.clone();
        let sm_clone_month = snapshot_manager.clone();
        let up_clone_month = user_prefs_manager.clone();
        let bm_clone_month = backup_manager.clone();
        let list_clone_month = snapshot_list.clone();
        let compare_btn_clone_month = compare_btn.clone();
        let disk_space_clone_month = disk_space_label.clone();
        let disk_space_bar_clone_month = disk_space_bar.clone();
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
                    &up_clone_month,
                    &bm_clone_month,
                    &list_clone_month,
                    &compare_btn_clone_month,
                    &disk_space_clone_month,
                    &disk_space_bar_clone_month,
                    &match_label_clone_month,
                    &search_text,
                    DateFilter::Last30Days,
                );
            }
        });

        let win_clone_quarter = window.clone();
        let sm_clone_quarter = snapshot_manager.clone();
        let up_clone_quarter = user_prefs_manager.clone();
        let bm_clone_quarter = backup_manager.clone();
        let list_clone_quarter = snapshot_list.clone();
        let compare_btn_clone_quarter = compare_btn.clone();
        let disk_space_clone_quarter = disk_space_label.clone();
        let disk_space_bar_clone_quarter = disk_space_bar.clone();
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
                    &up_clone_quarter,
                    &bm_clone_quarter,
                    &list_clone_quarter,
                    &compare_btn_clone_quarter,
                    &disk_space_clone_quarter,
                    &disk_space_bar_clone_quarter,
                    &match_label_clone_quarter,
                    &search_text,
                    DateFilter::Last90Days,
                );
            }
        });

        // Connect create button
        let sm_clone = snapshot_manager.clone();
        let up_clone = user_prefs_manager.clone();
        let bm_clone = backup_manager.clone();
        let list_clone = snapshot_list.clone();
        let win_clone = window.clone();
        let compare_btn_clone = compare_btn.clone();
        let disk_space_clone = disk_space_label.clone();
        let disk_space_bar_clone = disk_space_bar.clone();

        create_btn.connect_clicked(move |_| {
            Self::on_create_snapshot(
                &win_clone,
                sm_clone.clone(),
                up_clone.clone(),
                bm_clone.clone(),
                list_clone.clone(),
                compare_btn_clone.clone(),
                disk_space_clone.clone(),
                disk_space_bar_clone.clone(),
            );
        });

        // Connect compare button
        let sm_clone2 = snapshot_manager.clone();
        let win_clone2 = window.clone();

        compare_btn.connect_clicked(move |_| {
            Self::show_compare_dialog(&win_clone2, &sm_clone2);
        });

        // Connect search button to toggle revealer
        let revealer_clone = search_revealer.clone();
        let search_entry_clone = search_entry.clone();
        let search_btn_clone = search_btn.clone();

        search_btn.connect_clicked(move |_| {
            let is_revealed = revealer_clone.reveals_child();
            revealer_clone.set_reveal_child(!is_revealed);

            // Update button state
            if !is_revealed {
                // Opening search - add "suggested-action" class to highlight button
                search_btn_clone.add_css_class("suggested-action");
                // Auto-focus search entry
                search_entry_clone.grab_focus();
            } else {
                // Closing search - remove highlight
                search_btn_clone.remove_css_class("suggested-action");
            }
        });

        // Add ESC key handler to close search
        let key_controller = gtk::EventControllerKey::new();
        let revealer_for_esc = search_revealer.clone();
        let search_btn_for_esc = search_btn.clone();

        key_controller.connect_key_pressed(move |_, key, _code, _modifier| {
            if key == gtk::gdk::Key::Escape && revealer_for_esc.reveals_child() {
                revealer_for_esc.set_reveal_child(false);
                search_btn_for_esc.remove_css_class("suggested-action");
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        search_entry.add_controller(key_controller);

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
        let win_clone_menu_analytics = window.clone();
        let sm_clone_menu_analytics = snapshot_manager.clone();
        let popover_clone_analytics = popover.clone();
        analytics_row.connect_activated(move |_| {
            popover_clone_analytics.popdown();
            Self::show_analytics_dialog(&win_clone_menu_analytics, &sm_clone_menu_analytics);
        });

        let win_clone_menu_prefs = window.clone();
        let bm_clone_menu_prefs = backup_manager.clone();
        let popover_clone_prefs = popover.clone();
        preferences_row.connect_activated(move |_| {
            popover_clone_prefs.popdown();
            Self::show_preferences_dialog(&win_clone_menu_prefs, &bm_clone_menu_prefs);
        });

        let win_clone_menu_cleanup = window.clone();
        let list_clone_menu_cleanup = snapshot_list.clone();
        let sm_clone_menu_cleanup = snapshot_manager.clone();
        let up_clone_menu_cleanup = user_prefs_manager.clone();
        let bm_clone_menu_cleanup = backup_manager.clone();
        let compare_clone_menu_cleanup = compare_btn.clone();
        let disk_clone_menu_cleanup = disk_space_label.clone();
        let disk_bar_clone_menu_cleanup = disk_space_bar.clone();
        let popover_clone_cleanup = popover.clone();
        cleanup_row.connect_activated(move |_| {
            popover_clone_cleanup.popdown();
            Self::trigger_cleanup_snapshots(
                &win_clone_menu_cleanup,
                &sm_clone_menu_cleanup,
                &up_clone_menu_cleanup,
                &bm_clone_menu_cleanup,
                &list_clone_menu_cleanup,
                &compare_clone_menu_cleanup,
                &disk_clone_menu_cleanup,
                &disk_bar_clone_menu_cleanup,
            );
        });

        let win_clone_menu_about = window.clone();
        let popover_clone_about = popover.clone();
        about_row.connect_activated(move |_| {
            popover_clone_about.popdown();
            Self::show_about_dialog(&win_clone_menu_about);
        });

        // Initialize disk space monitoring
        Self::update_disk_space_label(&disk_space_label, &disk_space_bar);

        // Set up periodic disk space updates (every 30 seconds)
        let disk_space_label_clone = disk_space_label.clone();
        let disk_space_bar_clone = disk_space_bar.clone();
        glib::timeout_add_seconds_local(30, move || {
            Self::update_disk_space_label(&disk_space_label_clone, &disk_space_bar_clone);
            glib::ControlFlow::Continue
        });

        // Set up periodic snapshot list refresh (every 30 seconds)
        // This ensures external snapshots (from scheduler) appear in the UI
        let window_refresh = window.clone();
        let manager_refresh = snapshot_manager.clone();
        let user_prefs_refresh = user_prefs_manager.clone();
        let backup_manager_refresh = backup_manager.clone();
        let list_refresh = snapshot_list.clone();
        let compare_refresh = compare_btn.clone();
        let disk_space_refresh = disk_space_label.clone();
        let disk_space_bar_refresh = disk_space_bar.clone();
        glib::timeout_add_seconds_local(30, move || {
            Self::refresh_list_static(
                &window_refresh,
                &manager_refresh,
                &user_prefs_refresh,
                &backup_manager_refresh,
                &list_refresh,
                &compare_refresh,
                &disk_space_refresh,
                &disk_space_bar_refresh,
            );
            glib::ControlFlow::Continue
        });

        // Initialize mount monitoring for automatic backups
        use crate::mount_monitor::MountMonitor;
        let mount_monitor = MountMonitor::new();
        if let Err(e) = mount_monitor.initialize() {
            log::warn!("Failed to initialize mount monitor: {}", e);
        } else {
            log::info!("Mount monitor initialized");

            // Start monitoring for new drive mounts
            let backup_manager_monitor = backup_manager.clone();
            let window_monitor = window.clone();
            let app_monitor = app.clone();

            // Get configured mount check interval
            let check_interval = backup_manager
                .borrow()
                .get_config()
                .map(|c| c.mount_check_interval_seconds)
                .unwrap_or(60);

            mount_monitor.start_monitoring(check_interval, move |uuid, mount_point| {
                log::info!("New backup drive detected: {} at {}", uuid, mount_point);

                // Get snapshot directory from config
                let snapshot_dir = waypoint_common::WaypointConfig::new()
                    .snapshot_dir
                    .to_string_lossy()
                    .to_string();

                // Get destination label for notifications
                let dest_label = backup_manager_monitor
                    .borrow()
                    .get_config()
                    .ok()
                    .and_then(|c| c.destinations.get(&uuid).map(|d| d.label.clone()))
                    .unwrap_or_else(|| mount_point.clone());

                // Process pending backups for this destination
                match backup_manager_monitor.borrow().process_pending_backups(
                    &uuid,
                    &mount_point,
                    &snapshot_dir,
                ) {
                    Ok((success_count, fail_count, errors)) => {
                        if success_count > 0 || fail_count > 0 {
                            log::info!(
                                "Backup processing complete: {} successful, {} failed",
                                success_count,
                                fail_count
                            );

                            // Send desktop notification
                            notifications::notify_backup_completed(
                                &app_monitor,
                                &dest_label,
                                success_count,
                                fail_count,
                            );

                            // Show notification to user
                            let message = if fail_count == 0 {
                                format!("Successfully backed up {} snapshot(s)", success_count)
                            } else if success_count == 0 {
                                format!("Failed to backup {} snapshot(s)", fail_count)
                            } else {
                                format!(
                                    "Backed up {} snapshot(s), {} failed",
                                    success_count, fail_count
                                )
                            };

                            let dialog = adw::MessageDialog::new(
                                Some(&window_monitor),
                                Some("Automatic Backup Complete"),
                                Some(&message),
                            );
                            dialog.add_response("ok", "OK");
                            dialog.set_default_response(Some("ok"));

                            if !errors.is_empty() {
                                dialog.add_response("details", "Show Details");
                                let errors_clone = errors.clone();
                                let window_clone = window_monitor.clone();
                                dialog.connect_response(None, move |_, response| {
                                    if response == "details" {
                                        dialogs::show_error_list(
                                            &window_clone,
                                            "Backup Errors",
                                            &errors_clone,
                                        );
                                    }
                                });
                            }

                            dialog.present();
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to process pending backups: {}", e);
                    }
                }
            });
        }

        window
    }

    /// Update the disk space label with current usage
    ///
    /// Queries the available space for the root filesystem and updates the label and level bar
    /// with color-coded visuals based on remaining space percentage.
    fn update_disk_space_label(label: &Label, level_bar: &gtk::LevelBar) {
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
                    format!(
                        "{:.1} GB free of {:.1} GB ({:.0}% free)",
                        available_gb, total_gb, percent_free
                    )
                } else {
                    format!("{:.1} GB free", available_gb)
                };

                label.set_text(&text);

                // Update level bar to show percentage used (inverted from percent_free)
                if total_gb > 0.0 {
                    let percent_used = 100.0 - percent_free;
                    level_bar.set_value(percent_used / 100.0);
                } else {
                    level_bar.set_value(0.0);
                }

                // Remove any existing warning/error classes
                label.remove_css_class("warning");
                label.remove_css_class("error");

                // Color-code based on percentage (if we have total)
                if total_gb > 0.0 {
                    if percent_free < 10.0 {
                        // Critical: < 10% free - red
                        label.add_css_class("error");
                        label.set_tooltip_text(Some(
                            "Low disk space! Consider deleting old snapshots.",
                        ));
                    } else if percent_free < 20.0 {
                        // Warning: < 20% free - yellow
                        label.add_css_class("warning");
                        label.set_tooltip_text(Some(
                            "Disk space running low. Monitor snapshot usage.",
                        ));
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
        let user_prefs = self.user_prefs_manager.clone();
        let backup_manager = self.backup_manager.clone();
        let list = self.snapshot_list.clone();
        let compare_btn = self.compare_btn.clone();
        let disk_space_label = self.disk_space_label.clone();
        let disk_space_bar = self.disk_space_bar.clone();

        snapshot_list::refresh_snapshot_list_internal(
            &self.window,
            &self.snapshot_manager,
            &self.user_prefs_manager,
            &self.backup_manager,
            &self.snapshot_list,
            &self.compare_btn,
            None, // No search filter
            None, // No date filter
            None, // No match label
            move |id, action| {
                Self::handle_snapshot_action(
                    &window,
                    &manager,
                    &user_prefs,
                    &backup_manager,
                    &list,
                    &compare_btn,
                    &disk_space_label,
                    &disk_space_bar,
                    id,
                    action,
                );
            },
            Some(&self.create_btn),
        );
    }

    fn refresh_with_filter(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        user_prefs_manager: &Rc<RefCell<UserPreferencesManager>>,
        backup_manager: &Rc<RefCell<BackupManager>>,
        list: &ListBox,
        compare_btn: &Button,
        disk_space_label: &Label,
        disk_space_bar: &gtk::LevelBar,
        match_label: &Label,
        search_text: &str,
        date_filter: DateFilter,
    ) {
        let window_clone = window.clone();
        let manager_clone = manager.clone();
        let user_prefs_clone = user_prefs_manager.clone();
        let backup_manager_clone = backup_manager.clone();
        let list_clone = list.clone();
        let compare_btn_clone = compare_btn.clone();
        let disk_space_clone = disk_space_label.clone();
        let disk_space_bar_clone = disk_space_bar.clone();

        snapshot_list::refresh_snapshot_list_internal(
            window,
            manager,
            user_prefs_manager,
            backup_manager,
            list,
            compare_btn,
            Some(search_text),
            Some(date_filter),
            Some(match_label),
            move |id, action| {
                Self::handle_snapshot_action(
                    &window_clone,
                    &manager_clone,
                    &user_prefs_clone,
                    &backup_manager_clone,
                    &list_clone,
                    &compare_btn_clone,
                    &disk_space_clone,
                    &disk_space_bar_clone,
                    id,
                    action,
                );
            },
            None, // No create button for filtered view
        );
    }

    fn on_create_snapshot(
        window: &adw::ApplicationWindow,
        manager: Rc<RefCell<SnapshotManager>>,
        user_prefs_manager: Rc<RefCell<UserPreferencesManager>>,
        backup_manager: Rc<RefCell<BackupManager>>,
        list: ListBox,
        compare_btn: Button,
        disk_space_label: Label,
        disk_space_bar: gtk::LevelBar,
    ) {
        // Check if root is on Btrfs (can check without root)
        match btrfs::is_btrfs(&std::path::PathBuf::from("/")) {
            Ok(false) => {
                error_helpers::show_error_with_context(
                    window,
                    error_helpers::ErrorContext::FilesystemCheck,
                    "not a btrfs filesystem",
                );
                return;
            }
            Err(e) => {
                error_helpers::show_error_with_context(
                    window,
                    error_helpers::ErrorContext::FilesystemCheck,
                    &e.to_string(),
                );
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
        let user_prefs_clone = user_prefs_manager.clone();
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
                        log::error!("Disk space check thread disconnected");
                        break Ok(MIN_SPACE_BYTES + 1); // Assume sufficient space
                    }
                }
            };

            // Check if we have enough space
            match space_result {
                Ok(available) => {
                    if available < MIN_SPACE_BYTES {
                        let available_gb = available as f64 / (1024.0 * 1024.0 * 1024.0);
                        error_helpers::show_error_with_context(
                            &window_clone,
                            error_helpers::ErrorContext::DiskSpace,
                            &format!(
                                "Only {:.2} GB available, need at least {} GB",
                                available_gb, MIN_SPACE_GB
                            ),
                        );
                        return;
                    }
                }
                Err(e) => {
                    log::warn!("Could not check available disk space: {}", e);
                    // Continue anyway - this is just a warning
                }
            }

            // Show custom description dialog
            let window_clone2 = window_clone.clone();
            let list_clone2 = list_clone.clone();
            let manager_clone2 = manager_clone.clone();
            let user_prefs_clone2 = user_prefs_clone.clone();
            let backup_manager_clone2 = backup_manager.clone();
            let compare_btn_clone2 = compare_btn_clone.clone();
            let disk_space_clone2 = disk_space_label.clone();
            let disk_space_bar_clone2 = disk_space_bar.clone();

            create_snapshot_dialog::show_create_snapshot_dialog_async(
                &window_clone,
                move |result| {
                    if let Some((snapshot_name, description)) = result {
                        // User confirmed, create the snapshot
                        Self::create_snapshot_with_description(
                            &window_clone2,
                            manager_clone2.clone(),
                            user_prefs_clone2.clone(),
                            backup_manager_clone2.clone(),
                            list_clone2.clone(),
                            compare_btn_clone2.clone(),
                            disk_space_clone2.clone(),
                            disk_space_bar_clone2.clone(),
                            snapshot_name,
                            description,
                        );
                    }
                    // If None, user cancelled - do nothing
                },
            );
        });
    }

    fn create_snapshot_with_description(
        window: &adw::ApplicationWindow,
        manager: Rc<RefCell<SnapshotManager>>,
        user_prefs_manager: Rc<RefCell<UserPreferencesManager>>,
        backup_manager: Rc<RefCell<BackupManager>>,
        list: ListBox,
        compare_btn: Button,
        disk_space_label: Label,
        disk_space_bar: gtk::LevelBar,
        snapshot_name: String,
        description: String,
    ) {
        let window_clone = window.clone();
        let list_clone = list.clone();
        let manager_clone = manager.clone();
        let user_prefs_clone = user_prefs_manager.clone();
        let backup_manager_clone = backup_manager.clone();
        let compare_btn_clone = compare_btn.clone();
        let disk_space_clone = disk_space_label.clone();
        let disk_space_bar_clone = disk_space_bar.clone();
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
                    let error = format!(
                        "Failed to connect to snapshot service: {}\n\nTry: sudo sv reload dbus",
                        e
                    );
                    let _ =
                        sender.send((None, Some(("Connection Error".to_string(), error)), vec![]));
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
                if let Some((result, _client)) = result_opt {
                    match result {
                        Ok((true, message)) => {
                            // Verify snapshot actually exists before saving metadata
                            let snapshot_path = if PathBuf::from("/.snapshots").exists() {
                                PathBuf::from(format!("/.snapshots/{}", snapshot_name))
                            } else {
                                PathBuf::from(format!(
                                    "/mnt/btrfs-root/@snapshots/{}",
                                    snapshot_name
                                ))
                            };

                            if !snapshot_path.exists() {
                                Self::show_error_dialog(
                                    &window_clone,
                                    "Snapshot Creation Failed",
                                    &format!(
                                        "The snapshot was reported as created, but the snapshot directory does not exist:\n{}\n\nThis may indicate a permission issue or filesystem error.",
                                        snapshot_path.display()
                                    ),
                                );
                                return;
                            }

                            dialogs::show_toast(&window_clone, &message);

                            // Send desktop notification
                            if let Some(app) = window_clone.application() {
                                notifications::notify_snapshot_created(&app, &snapshot_name);
                            }

                            // Calculate snapshot size and save metadata
                            Self::save_snapshot_metadata(
                                &snapshot_name,
                                &description,
                                &subvolume_paths,
                                &manager_clone,
                            );

                            // Queue snapshot for automatic backup
                            if let Ok(prefs) = user_prefs_clone.borrow().get(&snapshot_name) {
                                let is_favorite = prefs.is_favorite;
                                if let Err(e) = backup_manager_clone
                                    .borrow()
                                    .queue_snapshot_backup(snapshot_name.clone(), is_favorite)
                                {
                                    log::warn!(
                                        "Failed to queue automatic backup for {}: {}",
                                        snapshot_name,
                                        e
                                    );
                                } else {
                                    log::info!(
                                        "Queued snapshot {} for automatic backup",
                                        snapshot_name
                                    );
                                }
                            } else {
                                // No prefs yet, assume not favorite
                                if let Err(e) = backup_manager_clone
                                    .borrow()
                                    .queue_snapshot_backup(snapshot_name.clone(), false)
                                {
                                    log::warn!(
                                        "Failed to queue automatic backup for {}: {}",
                                        snapshot_name,
                                        e
                                    );
                                } else {
                                    log::info!(
                                        "Queued snapshot {} for automatic backup",
                                        snapshot_name
                                    );
                                }
                            }

                            // Note: Retention cleanup is handled automatically by the scheduler service
                            // Users can manually trigger cleanup via the menu if needed

                            // Refresh snapshot list
                            Self::refresh_list_static(
                                &window_clone,
                                &manager_clone,
                                &user_prefs_clone,
                                &backup_manager_clone,
                                &list_clone,
                                &compare_btn_clone,
                                &disk_space_clone,
                                &disk_space_bar_clone,
                            );

                            // Update disk space after creating snapshot
                            Self::update_disk_space_label(&disk_space_clone, &disk_space_bar_clone);
                        }
                        Ok((false, message)) => {
                            error_helpers::show_error_with_context(
                                &window_clone,
                                error_helpers::ErrorContext::SnapshotCreate,
                                &message,
                            );
                        }
                        Err(e) => {
                            error_helpers::show_error_with_context(
                                &window_clone,
                                error_helpers::ErrorContext::SnapshotCreate,
                                &e.to_string(),
                            );
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
            log::warn!("Failed to save snapshot metadata: {}", e);
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
                                log::debug!("Calculated snapshot size: {} bytes", size);
                                // Update snapshot with size
                                if let Ok(Some(mut snapshot)) =
                                    manager_clone.borrow().get_snapshot(&name)
                                {
                                    snapshot.size_bytes = Some(size);
                                    if let Err(e) = manager_clone.borrow().add_snapshot(snapshot) {
                                        log::warn!("Failed to update snapshot size: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to calculate snapshot size: {}", e);
                            }
                        }
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        glib::timeout_future(std::time::Duration::from_millis(100)).await;
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        log::error!("Size calculation thread disconnected");
                        break;
                    }
                }
            }
        });
    }

    /// Trigger cleanup of old snapshots using per-schedule retention policies
    fn trigger_cleanup_snapshots(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        user_prefs_manager: &Rc<RefCell<UserPreferencesManager>>,
        backup_manager: &Rc<RefCell<BackupManager>>,
        list: &ListBox,
        compare_btn: &Button,
        disk_space_label: &Label,
        disk_space_bar: &gtk::LevelBar,
    ) {
        let window_clone = window.clone();
        let manager_clone = manager.clone();
        let user_prefs_clone = user_prefs_manager.clone();
        let backup_manager_clone = backup_manager.clone();
        let list_clone = list.clone();
        let compare_clone = compare_btn.clone();
        let disk_clone = disk_space_label.clone();
        let disk_bar_clone = disk_space_bar.clone();

        // Run cleanup in background thread
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<String> {
                let client = WaypointHelperClient::new()?;
                let (success, message) = client.cleanup_snapshots(true)?; // Use schedule-based retention
                if !success {
                    return Err(anyhow::anyhow!(message));
                }
                Ok(message)
            })();
            let _ = tx.send(result);
        });

        // Handle result on main thread
        glib::spawn_future_local(async move {
            loop {
                match rx.try_recv() {
                    Ok(result) => {
                        match result {
                            Ok(message) => {
                                dialogs::show_toast(&window_clone, &message);
                                // Refresh snapshot list after cleanup
                                Self::refresh_list_static(
                                    &window_clone,
                                    &manager_clone,
                                    &user_prefs_clone,
                                    &backup_manager_clone,
                                    &list_clone,
                                    &compare_clone,
                                    &disk_clone,
                                    &disk_bar_clone,
                                );
                            }
                            Err(e) => {
                                error_helpers::show_error_with_context(
                                    &window_clone,
                                    error_helpers::ErrorContext::SnapshotDelete,
                                    &format!("Cleanup failed: {}", e),
                                );
                            }
                        }
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        glib::timeout_future(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        log::error!("Cleanup thread disconnected");
                        break;
                    }
                }
            }
        });
    }

    fn refresh_list_static(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        user_prefs_manager: &Rc<RefCell<UserPreferencesManager>>,
        backup_manager: &Rc<RefCell<BackupManager>>,
        list: &ListBox,
        compare_btn: &Button,
        disk_space_label: &Label,
        disk_space_bar: &gtk::LevelBar,
    ) {
        let window_clone = window.clone();
        let manager_clone = manager.clone();
        let user_prefs_clone = user_prefs_manager.clone();
        let backup_manager_clone = backup_manager.clone();
        let list_clone = list.clone();
        let compare_btn_clone = compare_btn.clone();
        let disk_space_clone = disk_space_label.clone();
        let disk_space_bar_clone = disk_space_bar.clone();

        snapshot_list::refresh_snapshot_list_internal(
            window,
            manager,
            user_prefs_manager,
            backup_manager,
            list,
            compare_btn,
            None, // No search filter
            None, // No date filter
            None, // No match label
            move |id, action| {
                Self::handle_snapshot_action(
                    &window_clone,
                    &manager_clone,
                    &user_prefs_clone,
                    &backup_manager_clone,
                    &list_clone,
                    &compare_btn_clone,
                    &disk_space_clone,
                    &disk_space_bar_clone,
                    id,
                    action,
                );
            },
            None, // No create button needed here
        );
    }

    fn show_error_dialog(window: &adw::ApplicationWindow, title: &str, message: &str) {
        dialogs::show_error(window, title, message);
    }

    fn handle_snapshot_action(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        user_prefs_manager: &Rc<RefCell<UserPreferencesManager>>,
        backup_manager: &Rc<RefCell<BackupManager>>,
        list: &ListBox,
        compare_btn: &Button,
        disk_space_label: &Label,
        disk_space_bar: &gtk::LevelBar,
        snapshot_id: &str,
        action: SnapshotAction,
    ) {
        match action {
            SnapshotAction::Browse => {
                Self::browse_snapshot(window, manager, snapshot_id);
            }
            SnapshotAction::Verify => {
                Self::verify_snapshot(window, manager, snapshot_id);
            }
            SnapshotAction::Restore => {
                Self::restore_snapshot(window, manager, list, snapshot_id);
            }
            SnapshotAction::Delete => {
                Self::delete_snapshot(
                    window,
                    manager,
                    user_prefs_manager,
                    backup_manager,
                    list,
                    compare_btn,
                    disk_space_label,
                    disk_space_bar,
                    snapshot_id,
                );
            }
            SnapshotAction::ToggleFavorite => {
                Self::toggle_favorite(
                    window,
                    user_prefs_manager,
                    manager,
                    backup_manager,
                    list,
                    compare_btn,
                    snapshot_id,
                );
            }
            SnapshotAction::EditNote => {
                Self::edit_note(
                    window,
                    user_prefs_manager,
                    manager,
                    backup_manager,
                    list,
                    compare_btn,
                    snapshot_id,
                );
            }
            SnapshotAction::Backup => {
                Self::backup_snapshot(window, manager, snapshot_id);
            }
        }
    }

    // Helper function to scan for backup destinations
    fn scan_backup_destinations() -> anyhow::Result<Vec<BackupDestination>> {
        let client = WaypointHelperClient::new()?;
        let (success, result) = client.scan_backup_destinations()?;

        if !success {
            return Err(anyhow::anyhow!(result));
        }

        // Parse JSON response
        let destinations: Vec<BackupDestination> = serde_json::from_str(&result)?;
        Ok(destinations)
    }

    // Helper function to verify backup exists and is valid
    fn verify_backup_exists(backup_path: &str) -> anyhow::Result<()> {
        use std::path::Path;

        let path = Path::new(backup_path);

        // Check if path exists
        if !path.exists() {
            return Err(anyhow::anyhow!("Backup path does not exist"));
        }

        // Check if it's a directory (btrfs subvolume should be a directory)
        if !path.is_dir() {
            return Err(anyhow::anyhow!("Backup path is not a directory"));
        }

        // Verify it's a btrfs subvolume using btrfs subvolume show
        let output = std::process::Command::new("btrfs")
            .arg("subvolume")
            .arg("show")
            .arg(backup_path)
            .output()
            .context("Failed to run btrfs subvolume show")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Not a valid btrfs subvolume"));
        }

        Ok(())
    }

    // Helper function to perform backup
    fn perform_backup(snapshot_name: &str, destination_mount: &str) -> anyhow::Result<String> {
        let client = WaypointHelperClient::new()?;

        // Get snapshot path from config
        let config = waypoint_common::WaypointConfig::default();
        let snapshot_path = format!("{}/{}", config.snapshot_dir.display(), snapshot_name);

        let (success, result, _size_bytes) = client.backup_snapshot(
            snapshot_path,
            destination_mount.to_string(),
            String::new(), // No parent snapshot for now (full backup)
        )?;

        if !success {
            return Err(anyhow::anyhow!(result));
        }

        Ok(result)
    }

    fn backup_snapshot(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        snapshot_id: &str,
    ) {
        // Get the snapshot to retrieve its actual name (directory name on disk)
        let snapshot = match manager.borrow().get_snapshot(snapshot_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                Self::show_error_dialog(window, "Not Found", "Snapshot not found");
                return;
            }
            Err(e) => {
                Self::show_error_dialog(
                    window,
                    "Error",
                    &format!("Failed to load snapshot: {}", e),
                );
                return;
            }
        };

        let snapshot_name = snapshot.name.clone();

        // Create backup dialog
        let dialog = adw::Window::new();
        dialog.set_title(Some("Backup Snapshot"));
        dialog.set_modal(true);
        dialog.set_transient_for(Some(window));
        dialog.set_default_size(600, 500);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Header
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(
            "Backup Snapshot",
            &snapshot_name,
        )));
        content.append(&header);

        // Main content area with clamp
        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_vexpand(true);

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(600);

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content_box.set_margin_top(24);
        content_box.set_margin_bottom(24);
        content_box.set_margin_start(12);
        content_box.set_margin_end(12);

        // Info group
        let info_group = adw::PreferencesGroup::new();
        info_group.set_title("Backup Destination");
        info_group.set_description(Some("Select an external drive to backup this snapshot"));

        // Scan button row
        let scan_row = adw::ActionRow::new();
        scan_row.set_title("Scan for External Drives");
        scan_row.set_subtitle("Click to detect available backup destinations");

        let scan_button = gtk::Button::with_label("Scan");
        scan_button.set_valign(gtk::Align::Center);
        scan_button.add_css_class("suggested-action");
        scan_row.add_suffix(&scan_button);

        info_group.add(&scan_row);
        content_box.append(&info_group);

        // Destinations list group
        let dest_group = adw::PreferencesGroup::new();
        dest_group.set_title("Available Destinations");
        dest_group.set_visible(false);
        content_box.append(&dest_group);

        // Progress group (hidden initially)
        let progress_group = adw::PreferencesGroup::new();
        progress_group.set_title("Backup Progress");
        progress_group.set_visible(false);

        let progress_row = adw::ActionRow::new();
        progress_row.set_title("Backing up snapshot...");

        let progress_bar = gtk::ProgressBar::new();
        progress_bar.set_hexpand(true);
        progress_bar.pulse();
        progress_row.add_suffix(&progress_bar);

        progress_group.add(&progress_row);
        content_box.append(&progress_group);

        clamp.set_child(Some(&content_box));
        scrolled.set_child(Some(&clamp));
        content.append(&scrolled);

        dialog.set_content(Some(&content));
        dialog.present();

        // Connect scan button
        let dialog_clone = dialog.clone();
        let window_clone = window.clone();
        let dest_group_clone = dest_group.clone();
        let progress_group_clone = progress_group.clone();
        let snapshot_name_clone = snapshot_name.clone();

        scan_button.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("Scanning...");

            let btn_clone = btn.clone();
            let dest_group = dest_group_clone.clone();
            let window_ref = window_clone.clone();
            let dialog_ref = dialog_clone.clone();
            let progress_group_ref = progress_group_clone.clone();
            let snapshot_name_ref = snapshot_name_clone.clone();

            // Use thread + channel pattern
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let result = Self::scan_backup_destinations();
                let _ = tx.send(result);
            });

            // Poll for result
            gtk::glib::spawn_future_local(async move {
                let result = loop {
                    match rx.try_recv() {
                        Ok(result) => break result,
                        Err(mpsc::TryRecvError::Empty) => {
                            glib::timeout_future(std::time::Duration::from_millis(50)).await;
                            continue;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            dialogs::show_error(
                                &window_ref,
                                "Scan Failed",
                                "Scan thread disconnected unexpectedly",
                            );
                            btn_clone.set_sensitive(true);
                            btn_clone.set_label("Scan");
                            return;
                        }
                    }
                };

                btn_clone.set_sensitive(true);
                btn_clone.set_label("Scan");

                // Clear existing destinations
                while let Some(child) = dest_group.first_child() {
                    dest_group.remove(&child);
                }

                match result {
                    Ok(destinations) => {
                        if destinations.is_empty() {
                            let empty_row = adw::ActionRow::new();
                            empty_row.set_title("No external drives found");
                            empty_row.set_subtitle("Connect an external btrfs drive and scan again");
                            dest_group.add(&empty_row);
                        } else {
                            for dest in &destinations {
                                let row = adw::ActionRow::new();

                                // Add drive type badge to title
                                let type_badge = match dest.drive_type {
                                    DriveType::Removable => " (USB)",
                                    DriveType::Network => " (Network)",
                                    DriveType::Internal => " (Internal)",
                                };
                                row.set_title(&format!("{}{}", dest.label, type_badge));
                                row.set_subtitle(&dest.mount_point);

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

                                // Backup button
                                let backup_btn = gtk::Button::with_label("Backup Here");
                                backup_btn.set_valign(gtk::Align::Center);
                                backup_btn.add_css_class("suggested-action");

                                let dest_mount = dest.mount_point.clone();
                                let dialog_ref2 = dialog_ref.clone();
                                let window_ref2 = window_ref.clone();
                                let progress_group_ref2 = progress_group_ref.clone();
                                let snapshot_name_ref2 = snapshot_name_ref.clone();

                                backup_btn.connect_clicked(move |_| {
                                    // Show progress
                                    progress_group_ref2.set_visible(true);

                                    let dialog_ref3 = dialog_ref2.clone();
                                    let window_ref3 = window_ref2.clone();
                                    let dest_mount_clone = dest_mount.clone();
                                    let snapshot_name_clone = snapshot_name_ref2.clone();
                                    let progress_group_ref3 = progress_group_ref2.clone();

                                    // Use thread + channel pattern
                                    let (tx, rx) = mpsc::channel();
                                    std::thread::spawn(move || {
                                        let result = Self::perform_backup(&snapshot_name_clone, &dest_mount_clone);
                                        let _ = tx.send(result);
                                    });

                                    // Poll for result
                                    gtk::glib::spawn_future_local(async move {
                                        let result = loop {
                                            match rx.try_recv() {
                                                Ok(result) => break result,
                                                Err(mpsc::TryRecvError::Empty) => {
                                                    glib::timeout_future(std::time::Duration::from_millis(50)).await;
                                                    continue;
                                                }
                                                Err(mpsc::TryRecvError::Disconnected) => {
                                                    progress_group_ref3.set_visible(false);
                                                    dialog_ref3.close();
                                                    Self::show_error_dialog(
                                                        &window_ref3,
                                                        "Backup Failed",
                                                        "Backup thread disconnected unexpectedly",
                                                    );
                                                    return;
                                                }
                                            }
                                        };

                                        // Hide progress
                                        progress_group_ref3.set_visible(false);

                                        match result {
                                            Ok(backup_path) => {
                                                // Verify the backup using thread + channel
                                                let (verify_tx, verify_rx) = mpsc::channel();
                                                let backup_path_clone = backup_path.clone();
                                                std::thread::spawn(move || {
                                                    let result = Self::verify_backup_exists(&backup_path_clone);
                                                    let _ = verify_tx.send(result);
                                                });

                                                // Poll for verification result
                                                let verify_result = loop {
                                                    match verify_rx.try_recv() {
                                                        Ok(result) => break result,
                                                        Err(mpsc::TryRecvError::Empty) => {
                                                            glib::timeout_future(std::time::Duration::from_millis(50)).await;
                                                            continue;
                                                        }
                                                        Err(mpsc::TryRecvError::Disconnected) => {
                                                            break Err(anyhow::anyhow!("Verification thread disconnected"));
                                                        }
                                                    }
                                                };

                                                dialog_ref3.close();

                                                let message = match verify_result {
                                                    Ok(()) => format!("✓ Backup completed and verified successfully\n\nLocation: {}", backup_path),
                                                    Err(e) => format!("⚠ Backup completed but verification failed:\n{}\n\nLocation: {}", e, backup_path),
                                                };

                                                let success_dialog = adw::MessageDialog::new(
                                                    Some(&window_ref3),
                                                    Some("Backup Complete"),
                                                    Some(&message),
                                                );
                                                success_dialog.add_response("ok", "OK");
                                                success_dialog.set_default_response(Some("ok"));
                                                success_dialog.present();
                                            }
                                            Err(e) => {
                                                dialog_ref3.close();
                                                Self::show_error_dialog(
                                                    &window_ref3,
                                                    "Backup Failed",
                                                    &format!("Failed to backup snapshot: {}", e),
                                                );
                                            }
                                        }
                                    });
                                });

                                row.add_suffix(&backup_btn);
                                dest_group.add(&row);
                            }
                        }

                        dest_group.set_visible(true);
                    }
                    Err(e) => {
                        dialogs::show_error(&window_ref, "Scan Failed", &format!("Failed to scan for destinations: {}", e));
                    }
                }
            });
        });
    }

    fn verify_snapshot(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        snapshot_id: &str,
    ) {
        // Get the snapshot to retrieve its actual name (directory name on disk)
        let snapshot = match manager.borrow().get_snapshot(snapshot_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                Self::show_error_dialog(window, "Not Found", "Snapshot not found");
                return;
            }
            Err(e) => {
                Self::show_error_dialog(
                    window,
                    "Error",
                    &format!("Failed to load snapshot: {}", e),
                );
                return;
            }
        };

        let window_clone = window.clone();
        let snapshot_name = snapshot.name.clone();

        // Run verification in background thread
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<crate::dbus_client::VerificationResult> {
                let client = WaypointHelperClient::new()?;
                client.verify_snapshot(snapshot_name)
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
                        let mut message =
                            "✗ Snapshot verification failed\n\nErrors found:\n".to_string();
                        for error in &verification.errors {
                            message.push_str(&format!("• {}\n", error));
                        }

                        if !verification.warnings.is_empty() {
                            message.push_str("\nWarnings:\n");
                            for warning in &verification.warnings {
                                message.push_str(&format!("• {}\n", warning));
                            }
                        }

                        Self::show_error_dialog(&window_clone, "Verification Failed", &message);
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

        // Open snapshot directory in file manager using GTK's FileLauncher
        use waypoint_common::WaypointConfig;

        let config = WaypointConfig::new();
        let snapshot_path = config.snapshot_dir.join(&snapshot.name);

        // Check if path exists before trying to open
        if !snapshot_path.exists() {
            dialogs::show_error(
                window,
                "Snapshot Not Found",
                &format!(
                    "The snapshot directory does not exist:\n\n{}\n\nThe snapshot may have been deleted outside of Waypoint.",
                    snapshot_path.display()
                ),
            );
            return;
        }

        // Use GTK's FileLauncher to open the directory
        let file = gtk::gio::File::for_path(&snapshot_path);
        let launcher = gtk::FileLauncher::new(Some(&file));

        let window_clone = window.clone();
        launcher.open_containing_folder(Some(window), gtk::gio::Cancellable::NONE, move |result| {
            if let Err(e) = result {
                dialogs::show_error(
                    &window_clone,
                    "Cannot Open File Manager",
                    &format!("Failed to open file manager: {}", e),
                );
            }
        });
    }

    fn toggle_favorite(
        _window: &adw::ApplicationWindow,
        user_prefs_manager: &Rc<RefCell<UserPreferencesManager>>,
        manager: &Rc<RefCell<SnapshotManager>>,
        backup_manager: &Rc<RefCell<BackupManager>>,
        list: &ListBox,
        compare_btn: &Button,
        snapshot_id: &str,
    ) {
        // Toggle favorite state in user preferences
        if let Err(e) = user_prefs_manager.borrow().toggle_favorite(snapshot_id) {
            log::error!("Failed to toggle snapshot favorite state: {}", e);
            return;
        }

        // Refresh the list to show updated star icon and potentially reorder
        let window_weak = list.root().and_downcast::<adw::ApplicationWindow>();
        if let Some(window) = window_weak {
            let window_clone = window.clone();
            let manager_clone = manager.clone();
            let user_prefs_clone = user_prefs_manager.clone();
            let backup_manager_clone = backup_manager.clone();
            let list_clone = list.clone();
            let compare_btn_clone = compare_btn.clone();

            snapshot_list::refresh_snapshot_list_internal(
                &window,
                manager,
                user_prefs_manager,
                backup_manager,
                list,
                compare_btn,
                None,
                None,
                None,
                move |id, action| {
                    // Re-create clones for the action handler
                    let window = window_clone.clone();
                    let manager = manager_clone.clone();
                    let user_prefs = user_prefs_clone.clone();
                    let backup_manager = backup_manager_clone.clone();
                    let list = list_clone.clone();
                    let compare_btn = compare_btn_clone.clone();

                    // We need disk_space_label and disk_space_bar, but we don't have them here
                    // For now, we'll use dummy labels that won't be updated
                    let disk_space_label = Label::new(None);
                    let disk_space_bar = gtk::LevelBar::new();

                    Self::handle_snapshot_action(
                        &window,
                        &manager,
                        &user_prefs,
                        &backup_manager,
                        &list,
                        &compare_btn,
                        &disk_space_label,
                        &disk_space_bar,
                        id,
                        action,
                    );
                },
                None,
            );
        }
    }

    fn edit_note(
        window: &adw::ApplicationWindow,
        user_prefs_manager: &Rc<RefCell<UserPreferencesManager>>,
        manager: &Rc<RefCell<SnapshotManager>>,
        backup_manager: &Rc<RefCell<BackupManager>>,
        list: &ListBox,
        compare_btn: &Button,
        snapshot_id: &str,
    ) {
        // Get snapshot info for context
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

        // Load current user preferences for this snapshot
        let current_prefs = user_prefs_manager
            .borrow()
            .get(snapshot_id)
            .unwrap_or_default();

        // Create note edit dialog using AdwWindow
        let dialog = adw::Window::new();
        dialog.set_transient_for(Some(window));
        dialog.set_modal(true);
        dialog.set_default_width(550);
        dialog.set_default_height(450);
        dialog.set_title(Some("Edit Note"));

        // Create toolbar view for better layout
        let toolbar_view = adw::ToolbarView::new();

        // Header bar
        let header = adw::HeaderBar::new();
        header.set_show_title(true);
        toolbar_view.add_top_bar(&header);

        // Content area with proper margins
        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 18);
        content_box.set_margin_top(24);
        content_box.set_margin_bottom(24);
        content_box.set_margin_start(24);
        content_box.set_margin_end(24);

        // Snapshot name context with icon
        let context_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let snapshot_icon = gtk::Image::from_icon_name("waypoint");
        snapshot_icon.set_pixel_size(24);
        context_box.append(&snapshot_icon);

        let snapshot_info_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let snapshot_label = gtk::Label::new(Some(&snapshot.name));
        snapshot_label.set_halign(gtk::Align::Start);
        snapshot_label.add_css_class("title-4");
        snapshot_info_box.append(&snapshot_label);

        let timestamp_label = gtk::Label::new(Some(&snapshot.format_timestamp()));
        timestamp_label.set_halign(gtk::Align::Start);
        timestamp_label.add_css_class("dim-label");
        timestamp_label.add_css_class("caption");
        snapshot_info_box.append(&timestamp_label);

        context_box.append(&snapshot_info_box);
        content_box.append(&context_box);

        // Section title
        let section_label = gtk::Label::new(Some("Note"));
        section_label.set_halign(gtk::Align::Start);
        section_label.add_css_class("heading");
        content_box.append(&section_label);

        // Text view with border
        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_vexpand(true);
        scrolled_window.set_min_content_height(200);
        scrolled_window.add_css_class("card");

        let text_view = gtk::TextView::new();
        text_view.set_wrap_mode(gtk::WrapMode::WordChar);
        text_view.set_accepts_tab(false);
        text_view.set_top_margin(16);
        text_view.set_bottom_margin(16);
        text_view.set_left_margin(16);
        text_view.set_right_margin(16);

        // Add placeholder text
        let buffer = text_view.buffer();
        if let Some(note) = &current_prefs.note {
            buffer.set_text(note);
        }

        // Placeholder hint when empty
        let placeholder_label = gtk::Label::new(Some(
            "Add a personal note about this restore point...\n\nFor example: \"Before upgrading system packages\" or \"Clean install after testing\"",
        ));
        placeholder_label.set_halign(gtk::Align::Start);
        placeholder_label.set_valign(gtk::Align::Start);
        placeholder_label.add_css_class("dim-label");
        placeholder_label.set_margin_top(16);
        placeholder_label.set_margin_start(16);
        placeholder_label.set_margin_end(16);
        placeholder_label.set_wrap(true);
        placeholder_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);

        // Overlay for placeholder
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&text_view));
        overlay.add_overlay(&placeholder_label);

        // Show/hide placeholder based on text
        let placeholder_clone = placeholder_label.clone();
        buffer.connect_changed(move |buf| {
            let has_text = buf
                .text(&buf.start_iter(), &buf.end_iter(), false)
                .trim()
                .len()
                > 0;
            placeholder_clone.set_visible(!has_text);
        });
        placeholder_label.set_visible(current_prefs.note.is_none());

        scrolled_window.set_child(Some(&overlay));
        content_box.append(&scrolled_window);

        // Character counter and helper text
        let footer_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        footer_box.set_halign(gtk::Align::Fill);

        let helper_label = gtk::Label::new(Some("Only you can read snapshot notes"));
        helper_label.set_halign(gtk::Align::Start);
        helper_label.set_hexpand(true);
        helper_label.add_css_class("dim-label");
        helper_label.add_css_class("caption");
        footer_box.append(&helper_label);

        let char_count_label = gtk::Label::new(Some("0 characters"));
        char_count_label.set_halign(gtk::Align::End);
        char_count_label.add_css_class("dim-label");
        char_count_label.add_css_class("caption");
        footer_box.append(&char_count_label);

        // Update character count
        let char_count_clone = char_count_label.clone();
        buffer.connect_changed(move |buf| {
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            let count = text.chars().count();
            char_count_clone.set_text(&format!(
                "{} character{}",
                count,
                if count == 1 { "" } else { "s" }
            ));
        });

        // Set initial count
        if let Some(note) = &current_prefs.note {
            let count = note.chars().count();
            char_count_label.set_text(&format!(
                "{} character{}",
                count,
                if count == 1 { "" } else { "s" }
            ));
        }

        content_box.append(&footer_box);

        // Bottom button area
        let button_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        button_box.set_halign(gtk::Align::End);
        button_box.set_margin_top(12);

        let cancel_btn = gtk::Button::with_label("Cancel");
        let save_btn = gtk::Button::with_label("Save");
        save_btn.add_css_class("suggested-action");

        button_box.append(&cancel_btn);
        button_box.append(&save_btn);
        content_box.append(&button_box);

        toolbar_view.set_content(Some(&content_box));
        dialog.set_content(Some(&toolbar_view));

        // Save function
        let save_note = {
            let dialog = dialog.clone();
            let user_prefs_clone = user_prefs_manager.clone();
            let manager_clone = manager.clone();
            let backup_manager_clone = backup_manager.clone();
            let list_clone = list.clone();
            let compare_btn_clone = compare_btn.clone();
            let snapshot_id = snapshot_id.to_string();
            let text_view_clone = text_view.clone();

            move || {
                // Get note text from buffer
                let buffer = text_view_clone.buffer();
                let note_text = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), false)
                    .to_string();

                // Update note (trim whitespace, use None if empty)
                let note = if note_text.trim().is_empty() {
                    None
                } else {
                    Some(note_text.trim().to_string())
                };

                // Save note to user preferences
                if let Err(e) = user_prefs_clone.borrow().update_note(&snapshot_id, note) {
                    log::error!("Failed to save snapshot note: {}", e);
                    return;
                }

                // Refresh list to show updated note in subtitle
                let window_weak = list_clone.root().and_downcast::<adw::ApplicationWindow>();
                if let Some(window) = window_weak {
                    let window_inner = window.clone();
                    let manager_inner = manager_clone.clone();
                    let user_prefs_inner = user_prefs_clone.clone();
                    let backup_manager_inner = backup_manager_clone.clone();
                    let list_inner = list_clone.clone();
                    let compare_btn_inner = compare_btn_clone.clone();

                    snapshot_list::refresh_snapshot_list_internal(
                        &window,
                        &manager_clone,
                        &user_prefs_clone,
                        &backup_manager_clone,
                        &list_clone,
                        &compare_btn_clone,
                        None,
                        None,
                        None,
                        move |id, action| {
                            let disk_space_label = Label::new(None);
                            let disk_space_bar = gtk::LevelBar::new();
                            Self::handle_snapshot_action(
                                &window_inner,
                                &manager_inner,
                                &user_prefs_inner,
                                &backup_manager_inner,
                                &list_inner,
                                &compare_btn_inner,
                                &disk_space_label,
                                &disk_space_bar,
                                id,
                                action,
                            );
                        },
                        None,
                    );
                }

                dialog.close();
            }
        };

        // Handle cancel button
        let dialog_clone = dialog.clone();
        cancel_btn.connect_clicked(move |_| {
            dialog_clone.close();
        });

        // Handle save button
        let save_note_clone = save_note.clone();
        save_btn.connect_clicked(move |_| {
            save_note_clone();
        });

        // Keyboard shortcuts
        let key_controller = gtk::EventControllerKey::new();
        let save_note_clone2 = save_note.clone();
        let dialog_clone2 = dialog.clone();
        key_controller.connect_key_pressed(move |_, key, _, modifiers| {
            // Ctrl+Enter to save
            if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                && (key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter)
            {
                save_note_clone2();
                return gtk::glib::Propagation::Stop;
            }
            // Escape to cancel
            if key == gtk::gdk::Key::Escape {
                dialog_clone2.close();
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        dialog.add_controller(key_controller);

        // Auto-focus text view
        text_view.grab_focus();

        // Show dialog
        dialog.present();
    }

    fn delete_snapshot(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
        user_prefs_manager: &Rc<RefCell<UserPreferencesManager>>,
        backup_manager: &Rc<RefCell<BackupManager>>,
        list: &ListBox,
        compare_btn: &Button,
        disk_space_label: &Label,
        disk_space_bar: &gtk::LevelBar,
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
        let snapshot_basename = snapshot
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&snapshot_name)
            .to_string();

        let window_clone = window.clone();
        let manager_clone = manager.clone();
        let user_prefs_clone = user_prefs_manager.clone();
        let backup_manager_clone = backup_manager.clone();
        let list_clone = list.clone();
        let compare_btn_clone = compare_btn.clone();
        let disk_space_clone = disk_space_label.clone();
        let disk_space_bar_clone = disk_space_bar.clone();

        // Check if snapshot has backups
        let has_backups = backup_manager.borrow().is_snapshot_backed_up(&snapshot.id);
        let message = if has_backups {
            format!(
                "Are you sure you want to delete '{}'?\n\nThis snapshot has backups on external drives. Deleting it here will NOT delete the backups.\n\nThis action cannot be undone.",
                snapshot_name
            )
        } else {
            format!(
                "Are you sure you want to delete '{}'?\n\nThis action cannot be undone.",
                snapshot_name
            )
        };

        dialogs::show_confirmation(
            window,
            "Delete Snapshot?",
            &message,
            "Delete",
            true,
            move || {
                let window = window_clone.clone();
                let manager = manager_clone.clone();
                let user_prefs = user_prefs_clone.clone();
                let backup_manager = backup_manager_clone.clone();
                let list = list_clone.clone();
                let compare_btn = compare_btn_clone.clone();
                let disk_space = disk_space_clone.clone();
                let disk_space_bar = disk_space_bar_clone.clone();
                let name = snapshot_basename.clone();
                let name_for_notification = snapshot_basename.clone();

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
                            let _ =
                                sender.send((None, Some(("Connection Error".to_string(), error))));
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

                                    // Send desktop notification
                                    if let Some(app) = window.application() {
                                        notifications::notify_snapshot_deleted(
                                            &app,
                                            &name_for_notification,
                                        );
                                    }

                                    // Refresh the list
                                    Self::refresh_list_static(
                                        &window,
                                        &manager,
                                        &user_prefs,
                                        &backup_manager,
                                        &list,
                                        &compare_btn,
                                        &disk_space,
                                        &disk_space_bar,
                                    );
                                    // Update disk space after deletion
                                    Self::update_disk_space_label(&disk_space, &disk_space_bar);
                                }
                                Ok((false, message)) => {
                                    error_helpers::show_error_with_context(
                                        &window,
                                        error_helpers::ErrorContext::SnapshotDelete,
                                        &message,
                                    );
                                }
                                Err(e) => {
                                    error_helpers::show_error_with_context(
                                        &window,
                                        error_helpers::ErrorContext::SnapshotDelete,
                                        &e.to_string(),
                                    );
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

        // Show restore choice dialog
        Self::show_restore_choice_dialog(window, &snapshot.name);
    }

    fn show_restore_choice_dialog(window: &adw::ApplicationWindow, snapshot_name: &str) {
        let dialog = adw::Window::new();
        dialog.set_transient_for(Some(window));
        dialog.set_modal(true);
        dialog.set_title(Some("Choose Restore Type"));
        dialog.set_default_size(500, 300);

        let main_box = gtk::Box::new(Orientation::Vertical, 0);

        // Header
        let header = adw::HeaderBar::new();
        main_box.append(&header);

        // Content
        let content_box = gtk::Box::new(Orientation::Vertical, 0);
        content_box.set_margin_top(24);
        content_box.set_margin_bottom(24);
        content_box.set_margin_start(24);
        content_box.set_margin_end(24);

        let group = adw::PreferencesGroup::new();
        group.set_title("How would you like to restore?");
        group.set_description(Some(
            "Choose whether to restore the entire system or individual files",
        ));

        // Full system restore option
        let full_restore_row = adw::ActionRow::new();
        full_restore_row.set_title("Restore Entire System");
        full_restore_row
            .set_subtitle("Roll back your entire system to this restore point (requires reboot)");
        full_restore_row.set_activatable(true);

        let full_icon = gtk::Image::from_icon_name("view-refresh-symbolic");
        full_icon.set_pixel_size(24);
        full_restore_row.add_prefix(&full_icon);

        let full_arrow = gtk::Image::from_icon_name("go-next-symbolic");
        full_restore_row.add_suffix(&full_arrow);

        group.add(&full_restore_row);

        // Individual files restore option
        let files_restore_row = adw::ActionRow::new();
        files_restore_row.set_title("Restore Individual Files");
        files_restore_row
            .set_subtitle("Select specific files or folders to restore from this restore point");
        files_restore_row.set_activatable(true);

        let files_icon = gtk::Image::from_icon_name("document-save-symbolic");
        files_icon.set_pixel_size(24);
        files_restore_row.add_prefix(&files_icon);

        let files_arrow = gtk::Image::from_icon_name("go-next-symbolic");
        files_restore_row.add_suffix(&files_arrow);

        group.add(&files_restore_row);

        content_box.append(&group);
        main_box.append(&content_box);

        dialog.set_content(Some(&main_box));

        // Connect actions
        let window_clone = window.clone();
        let snapshot_name_clone = snapshot_name.to_string();
        let dialog_clone = dialog.clone();
        full_restore_row.connect_activated(move |_| {
            dialog_clone.close();
            Self::perform_full_restore(&window_clone, &snapshot_name_clone);
        });

        let window_clone2 = window.clone();
        let snapshot_name_clone2 = snapshot_name.to_string();
        let dialog_clone2 = dialog.clone();
        files_restore_row.connect_activated(move |_| {
            dialog_clone2.close();
            file_restore_dialog::show_file_restore_dialog(&window_clone2, &snapshot_name_clone2);
        });

        dialog.present();
    }

    fn perform_full_restore(window: &adw::ApplicationWindow, snapshot_basename: &str) {
        let window_clone = window.clone();
        let snapshot_id_owned = snapshot_basename.to_string();
        let snapshot_id_for_idle = snapshot_basename.to_string();

        // Show loading toast while fetching preview
        dialogs::show_toast(window, "Loading restore preview...");

        // Create channel for background thread communication
        let (tx, rx) = mpsc::channel();

        // Fetch preview in background thread
        std::thread::spawn(move || {
            let client = match WaypointHelperClient::new() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(anyhow::anyhow!(
                        "Failed to connect to snapshot service: {}",
                        e
                    )));
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
                    Self::show_restore_preview_dialog(
                        &window_clone,
                        &snapshot_id_for_idle,
                        preview,
                    );
                    glib::ControlFlow::Break
                }
                Ok(Err(e)) => {
                    dialogs::show_error(
                        &window_clone,
                        "Preview Failed",
                        &format!("Failed to generate restore preview: {}", e),
                    );
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
            preview.snapshot_name, preview.snapshot_timestamp
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
        preview_parts.push(format!(
            "\n📦 Package Changes: {}",
            preview.total_package_changes
        ));

        if !preview.packages_to_add.is_empty() {
            preview_parts.push(format!("  ➕ {} to install", preview.packages_to_add.len()));
            // Show first few examples
            for pkg in preview.packages_to_add.iter().take(3) {
                let version = pkg.snapshot_version.as_deref().unwrap_or("unknown");
                preview_parts.push(format!("     • {} ({})", pkg.name, version));
            }
            if preview.packages_to_add.len() > 3 {
                preview_parts.push(format!(
                    "     • ... and {} more",
                    preview.packages_to_add.len() - 3
                ));
            }
        }
        if !preview.packages_to_remove.is_empty() {
            preview_parts.push(format!(
                "  ➖ {} to remove",
                preview.packages_to_remove.len()
            ));
            for pkg in preview.packages_to_remove.iter().take(3) {
                let version = pkg.current_version.as_deref().unwrap_or("unknown");
                preview_parts.push(format!("     • {} ({})", pkg.name, version));
            }
            if preview.packages_to_remove.len() > 3 {
                preview_parts.push(format!(
                    "     • ... and {} more",
                    preview.packages_to_remove.len() - 3
                ));
            }
        }
        if !preview.packages_to_upgrade.is_empty() {
            preview_parts.push(format!(
                "  ⬆️  {} to upgrade",
                preview.packages_to_upgrade.len()
            ));
            for pkg in preview.packages_to_upgrade.iter().take(3) {
                let curr = pkg.current_version.as_deref().unwrap_or("?");
                let snap = pkg.snapshot_version.as_deref().unwrap_or("?");
                preview_parts.push(format!("     • {} ({} → {})", pkg.name, curr, snap));
            }
            if preview.packages_to_upgrade.len() > 3 {
                preview_parts.push(format!(
                    "     • ... and {} more",
                    preview.packages_to_upgrade.len() - 3
                ));
            }
        }
        if !preview.packages_to_downgrade.is_empty() {
            preview_parts.push(format!(
                "  ⬇️  {} to downgrade",
                preview.packages_to_downgrade.len()
            ));
            for pkg in preview.packages_to_downgrade.iter().take(3) {
                let curr = pkg.current_version.as_deref().unwrap_or("?");
                let snap = pkg.snapshot_version.as_deref().unwrap_or("?");
                preview_parts.push(format!("     • {} ({} → {})", pkg.name, curr, snap));
            }
            if preview.packages_to_downgrade.len() > 3 {
                preview_parts.push(format!(
                    "     • ... and {} more",
                    preview.packages_to_downgrade.len() - 3
                ));
            }
        }

        // Affected subvolumes
        if !preview.affected_subvolumes.is_empty() {
            preview_parts.push(format!(
                "\n💾 Affected: {}",
                preview.affected_subvolumes.join(", ")
            ));
        }

        // Warning footer
        preview_parts.push(
            "\n⚠️  WARNING:\n\
            • All changes after this snapshot will be LOST\n\
            • System will require a REBOOT\n\
            • A backup snapshot will be created first"
                .to_string(),
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
                let name_for_notification = snapshot_name.clone();

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
                                    // Send desktop notification
                                    if let Some(app) = window.application() {
                                        notifications::notify_snapshot_restored(&app, &name_for_notification);
                                    }

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
                                    error_helpers::show_error_with_context(
                                        &window,
                                        error_helpers::ErrorContext::SnapshotRestore,
                                        &message
                                    );
                                }
                                Err(e) => {
                                    error_helpers::show_error_with_context(
                                        &window,
                                        error_helpers::ErrorContext::SnapshotRestore,
                                        &e.to_string()
                                    );
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
    fn show_compare_dialog(
        window: &adw::ApplicationWindow,
        manager: &Rc<RefCell<SnapshotManager>>,
    ) {
        comparison_dialog::show_compare_dialog(window, manager);
    }

    /// Show preferences dialog
    fn show_preferences_dialog(
        window: &adw::ApplicationWindow,
        backup_manager: &Rc<RefCell<BackupManager>>,
    ) {
        preferences_window::show_preferences_window(window, backup_manager.clone());
    }

    /// Show analytics dialog
    fn show_analytics_dialog(
        window: &adw::ApplicationWindow,
        snapshot_manager: &std::rc::Rc<std::cell::RefCell<SnapshotManager>>,
    ) {
        // Load snapshots
        let snapshots = match snapshot_manager.borrow().load_snapshots() {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to load snapshots for analytics: {}", e);
                Vec::new()
            }
        };
        analytics_dialog::show_analytics_dialog(window, &snapshots);
    }

    fn show_about_dialog(window: &adw::ApplicationWindow) {
        about_preferences::show_about_dialog(window);
    }

    #[allow(dead_code)]
    pub fn present(&self) {
        self.window.present();
    }
}
