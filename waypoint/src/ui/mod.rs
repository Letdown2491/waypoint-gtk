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
                std::process::exit(1);
            }
        };

        // Create header bar
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new("Waypoint", "")));

        // Status banner - also returns whether Btrfs is available
        let (banner, is_btrfs) = Self::create_status_banner();

        // Toolbar with buttons
        let (toolbar, create_btn, compare_btn, statistics_btn, scheduler_btn, preferences_btn) = toolbar::create_toolbar();

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
        let win_clone3 = window.clone();
        let sm_clone3 = snapshot_manager.clone();

        statistics_btn.connect_clicked(move |_| {
            Self::show_statistics_dialog(&win_clone3, &sm_clone3);
        });

        // Connect scheduler button
        let win_clone4 = window.clone();

        scheduler_btn.connect_clicked(move |_| {
            scheduler_dialog::show_scheduler_dialog(&win_clone4);
        });

        // Connect preferences button
        let win_clone5 = window.clone();

        preferences_btn.connect_clicked(move |_| {
            Self::show_preferences_dialog(&win_clone5);
        });

        window
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

        // Check available disk space (can check without root)
        const MIN_SPACE_GB: u64 = 1; // Minimum 1 GB free space
        const MIN_SPACE_BYTES: u64 = MIN_SPACE_GB * 1024 * 1024 * 1024;

        match btrfs::get_available_space(&std::path::PathBuf::from("/")) {
            Ok(available) => {
                if available < MIN_SPACE_BYTES {
                    let available_gb = available as f64 / (1024.0 * 1024.0 * 1024.0);
                    Self::show_error_dialog(
                        window,
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
        let window_clone = window.clone();
        let list_clone = list.clone();
        let manager_clone = manager.clone();
        let compare_btn_clone = compare_btn.clone();

        create_snapshot_dialog::show_create_snapshot_dialog_async(window, move |result| {
            if let Some((snapshot_name, description)) = result {
                // User confirmed, create the snapshot
                Self::create_snapshot_with_description(
                    &window_clone,
                    manager_clone.clone(),
                    list_clone.clone(),
                    compare_btn_clone.clone(),
                    snapshot_name,
                    description,
                );
            }
            // If None, user cancelled - do nothing
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
        let snapshot_path = PathBuf::from(format!("/@snapshots/{}", snapshot_name));

        // Calculate snapshot size (this may take a moment)
        let size_bytes = match btrfs::get_snapshot_size(&snapshot_path) {
            Ok(size) => {
                eprintln!("Calculated snapshot size: {} bytes", size);
                Some(size)
            }
            Err(e) => {
                eprintln!("Warning: Failed to calculate snapshot size: {}", e);
                None
            }
        };

        // Create snapshot metadata
        let snapshot = Snapshot {
            id: snapshot_name.to_string(),
            name: snapshot_name.to_string(),
            timestamp: chrono::Utc::now(),
            path: snapshot_path,
            description: Some(description.to_string()),
            kernel_version: None, // Could add this later
            package_count: None,  // Could add this later
            size_bytes,
            packages: Vec::new(),
            subvolumes: subvolume_paths.to_vec(),
        };

        // Save metadata
        if let Err(e) = manager.borrow().add_snapshot(snapshot) {
            eprintln!("Warning: Failed to save snapshot metadata: {}", e);
        }
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
            SnapshotAction::Restore => {
                Self::restore_snapshot(window, manager, list, snapshot_id);
            }
            SnapshotAction::Delete => {
                Self::delete_snapshot(window, manager, list, compare_btn, snapshot_id);
            }
        }
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

        // Build detailed warning message
        let snapshot_date = snapshot.format_timestamp();
        let pkg_info = if let Some(count) = snapshot.package_count {
            format!("{} packages", count)
        } else {
            "unknown package count".to_string()
        };

        let kernel_info = snapshot.kernel_version.as_deref().unwrap_or("unknown");

        let warning_message = format!(
            "⚠️ CRITICAL WARNING ⚠️\n\n\
            You are about to restore your system to:\n\
            • Snapshot: {}\n\
            • Created: {}\n\
            • Kernel: {}\n\
            • Packages: {}\n\n\
            This will:\n\
            ✓ Change your system to match this snapshot\n\
            ✓ Require a reboot to take effect\n\
            ✗ LOSE ALL CHANGES made after this snapshot\n\
            ✗ This CANNOT be undone automatically\n\n\
            Before proceeding:\n\
            1. Save all your work\n\
            2. Close all applications\n\
            3. Make sure you have a backup\n\n\
            A backup snapshot will be created first.\n\n\
            Do you want to continue?",
            snapshot.name,
            snapshot_date,
            kernel_info,
            pkg_info
        );

        // Extract snapshot basename
        let snapshot_basename = snapshot.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&snapshot.name)
            .to_string();

        let window_clone = window.clone();

        dialogs::show_confirmation(
            window,
            "Restore System Snapshot?",
            &warning_message,
            "Restore and Reboot",
            true, // destructive
            move || {
                let window = window_clone.clone();
                let name = snapshot_basename.clone();

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
            },
        );
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

    #[allow(dead_code)]
    pub fn present(&self) {
        self.window.present();
    }
}
