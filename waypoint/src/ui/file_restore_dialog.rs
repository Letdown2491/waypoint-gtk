//! File restoration dialog for browsing and restoring files from snapshots

#![allow(deprecated)] // Still using FileChooserDialog for folder selection

use crate::dbus_client::WaypointHelperClient;
use adw::prelude::*;
use gtk::prelude::*;
use gtk::{FileChooserAction, FileChooserDialog, Orientation, ResponseType};
use libadwaita as adw;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use waypoint_common::WaypointConfig;

use super::dialogs;
use super::error_helpers;

/// Show custom file browser dialog for restoring files from a snapshot
pub fn show_file_restore_dialog(parent: &adw::ApplicationWindow, snapshot_name: &str) {
    let config = WaypointConfig::new();
    let snapshot_path = config.snapshot_dir.join(snapshot_name).join("root");

    // Verify snapshot exists
    if !snapshot_path.exists() {
        error_helpers::show_error_with_context(
            parent,
            error_helpers::ErrorContext::SnapshotRestore,
            &format!("Snapshot directory not found: {}", snapshot_path.display()),
        );
        return;
    }

    // Create custom file browser window
    let dialog = adw::Window::new();
    dialog.set_title(Some(&format!("Restore Files - {snapshot_name}")));
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));
    dialog.set_default_size(900, 600);

    let content_box = gtk::Box::new(Orientation::Vertical, 0);

    // Header bar
    let header = adw::HeaderBar::new();

    let cancel_button = gtk::Button::with_label("Cancel");
    header.pack_start(&cancel_button);

    let restore_button = gtk::Button::with_label("Restore Selected");
    restore_button.add_css_class("suggested-action");
    restore_button.set_sensitive(false); // Disabled until files are selected
    header.pack_end(&restore_button);

    content_box.append(&header);

    // Main content with sidebar and file list
    let paned = gtk::Paned::new(Orientation::Horizontal);
    paned.set_vexpand(true);
    paned.set_position(250);

    // Sidebar with quick access
    let sidebar = create_sidebar(&snapshot_path);
    paned.set_start_child(Some(&sidebar));

    // Track selected files
    let selected_files: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));

    // File list area
    let file_area = create_file_list_area(&snapshot_path, selected_files.clone(), restore_button.clone());
    paned.set_end_child(Some(&file_area));

    content_box.append(&paned);
    dialog.set_content(Some(&content_box));

    // Wire up cancel button
    let dialog_clone = dialog.clone();
    cancel_button.connect_clicked(move |_| {
        dialog_clone.close();
    });

    // Wire up restore button
    let dialog_clone = dialog.clone();
    let parent_clone = parent.clone();
    let snapshot_name_owned = snapshot_name.to_string();
    let snapshot_root = snapshot_path.clone();

    restore_button.connect_clicked(move |_| {
        let files = selected_files.borrow().clone();
        if files.is_empty() {
            dialogs::show_toast(&parent_clone, "No files selected");
            return;
        }

        dialog_clone.close();
        show_restore_confirmation_dialog(&parent_clone, &snapshot_name_owned, files, &snapshot_root);
    });

    dialog.present();
}

/// Create sidebar with common directories
fn create_sidebar(_snapshot_root: &Path) -> gtk::Box {
    let sidebar = gtk::Box::new(Orientation::Vertical, 0);
    sidebar.add_css_class("sidebar");

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);

    let list_box = gtk::ListBox::new();
    list_box.add_css_class("navigation-sidebar");

    // Common directories
    let common_dirs = vec![
        ("Home Directory", "user-home-symbolic", "home"),
        ("Documents", "folder-documents-symbolic", "home/*/Documents"),
        ("Downloads", "folder-download-symbolic", "home/*/Downloads"),
        ("Pictures", "folder-pictures-symbolic", "home/*/Pictures"),
        ("System Configuration", "preferences-system-symbolic", "etc"),
        ("Applications", "applications-system-symbolic", "usr/share/applications"),
        ("System Binaries", "utilities-terminal-symbolic", "usr/bin"),
    ];

    for (label, icon, _path) in common_dirs {
        let row = gtk::ListBoxRow::new();
        let row_box = gtk::Box::new(Orientation::Horizontal, 12);
        row_box.set_margin_start(12);
        row_box.set_margin_end(12);
        row_box.set_margin_top(8);
        row_box.set_margin_bottom(8);

        let icon_widget = gtk::Image::from_icon_name(icon);
        row_box.append(&icon_widget);

        let label_widget = gtk::Label::new(Some(label));
        label_widget.set_xalign(0.0);
        row_box.append(&label_widget);

        row.set_child(Some(&row_box));
        list_box.append(&row);
    }

    scrolled.set_child(Some(&list_box));
    sidebar.append(&scrolled);

    sidebar
}

/// Create file list area with search and tree view
fn create_file_list_area(
    snapshot_root: &Path,
    selected_files: Rc<RefCell<Vec<PathBuf>>>,
    restore_button: gtk::Button,
) -> gtk::Box {
    let container = gtk::Box::new(Orientation::Vertical, 0);

    // Search bar
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search files..."));
    search_entry.set_margin_start(12);
    search_entry.set_margin_end(12);
    search_entry.set_margin_top(12);
    search_entry.set_margin_bottom(12);
    container.append(&search_entry);

    // File tree view
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);

    let list_box = gtk::ListBox::new();

    // Show files from snapshot root
    if let Ok(entries) = std::fs::read_dir(snapshot_root) {
        for entry in entries.flatten().take(50) {
            let path = entry.path();
            let row = gtk::ListBoxRow::new();
            let row_box = gtk::Box::new(Orientation::Horizontal, 12);
            row_box.set_margin_start(12);
            row_box.set_margin_end(12);
            row_box.set_margin_top(8);
            row_box.set_margin_bottom(8);

            let is_dir = path.is_dir();
            let icon_name = if is_dir { "folder-symbolic" } else { "text-x-generic-symbolic" };
            let icon = gtk::Image::from_icon_name(icon_name);
            row_box.append(&icon);

            let name = entry.file_name();
            let label = gtk::Label::new(Some(&name.to_string_lossy()));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            row_box.append(&label);

            // Checkbutton for selection
            let check = gtk::CheckButton::new();

            // Wire up checkbox to track selection
            let selected_files_clone = selected_files.clone();
            let restore_button_clone = restore_button.clone();
            let file_path = path.clone();

            check.connect_toggled(move |check_btn| {
                let mut files = selected_files_clone.borrow_mut();

                if check_btn.is_active() {
                    if !files.contains(&file_path) {
                        files.push(file_path.clone());
                    }
                } else {
                    files.retain(|p| p != &file_path);
                }

                // Enable/disable restore button based on selection
                restore_button_clone.set_sensitive(!files.is_empty());
            });

            row_box.append(&check);

            row.set_child(Some(&row_box));
            list_box.append(&row);
        }
    }

    scrolled.set_child(Some(&list_box));
    container.append(&scrolled);

    container
}

/// Show confirmation dialog before restoring files
fn show_restore_confirmation_dialog(
    parent: &adw::ApplicationWindow,
    snapshot_name: &str,
    selected_files: Vec<PathBuf>,
    snapshot_root: &PathBuf,
) {
    // Build file list message
    let file_list: Vec<String> = selected_files
        .iter()
        .map(|p| {
            // Get path relative to snapshot root
            p.strip_prefix(snapshot_root)
                .ok()
                .and_then(|rel| rel.to_str())
                .map(|s| format!("/{s}"))
                .unwrap_or_else(|| p.display().to_string())
        })
        .collect();

    let heading = format!(
        "Restore {} file(s) from snapshot '{}'?",
        file_list.len(),
        snapshot_name
    );
    let body = file_list.join("\n");

    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Restore Files from Snapshot"),
        Some(&heading),
    );

    dialog.set_body(&body);
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("restore", "Restore to Original Location");
    dialog.add_response("restore_custom", "Restore to Custom Location");
    dialog.set_response_appearance("restore", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("cancel"));

    let snapshot_name_owned = snapshot_name.to_string();
    let parent_clone = parent.clone();

    dialog.connect_response(None, move |_, response| {
        match response {
            "restore" => {
                // Restore to original locations
                perform_file_restore(
                    &parent_clone,
                    &snapshot_name_owned,
                    file_list.clone(),
                    "",
                    true,
                );
            }
            "restore_custom" => {
                // Show directory chooser for custom location
                show_custom_location_chooser(
                    &parent_clone,
                    &snapshot_name_owned,
                    file_list.clone(),
                );
            }
            _ => {} // Cancel - do nothing
        }
    });

    dialog.present();
}

/// Show directory chooser for custom restore location
fn show_custom_location_chooser(
    parent: &adw::ApplicationWindow,
    snapshot_name: &str,
    file_paths: Vec<String>,
) {
    let dialog = FileChooserDialog::new(
        Some("Choose Restore Location"),
        Some(parent),
        FileChooserAction::SelectFolder,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Restore Here", ResponseType::Accept),
        ],
    );

    dialog.set_modal(true);

    let snapshot_name_owned = snapshot_name.to_string();
    let parent_clone = parent.clone();

    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            if let Some(folder) = dialog.file().and_then(|f| f.path()) {
                let target_dir = folder.to_string_lossy().to_string();
                perform_file_restore(
                    &parent_clone,
                    &snapshot_name_owned,
                    file_paths.clone(),
                    &target_dir,
                    true,
                );
            }
        }
        dialog.close();
    });

    dialog.present();
}

/// Perform the actual file restoration via D-Bus
fn perform_file_restore(
    parent: &adw::ApplicationWindow,
    snapshot_name: &str,
    file_paths: Vec<String>,
    target_directory: &str,
    overwrite: bool,
) {
    let parent_clone = parent.clone();
    let snapshot_name_owned = snapshot_name.to_string();
    let target_directory_owned = target_directory.to_string();
    let (cancel_tx, cancel_rx) = std::sync::mpsc::channel::<()>();

    // Run restoration in background thread
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<String> {
            let client = WaypointHelperClient::new()?;
            let (success, message) = client.restore_files(
                snapshot_name_owned,
                file_paths,
                target_directory_owned,
                overwrite,
            )?;

            if !success {
                return Err(anyhow::anyhow!(message));
            }

            Ok(message)
        })();

        let _ = tx.send(result);
    });

    // Handle result on main thread
    let parent_for_close = parent.clone();
    parent_for_close.connect_close_request(move |_| {
        let _ = cancel_tx.send(());
        gtk::glib::Propagation::Proceed
    });

    gtk::glib::spawn_future_local(async move {
        loop {
            if cancel_rx.try_recv().is_ok() {
                break;
            }
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(message) => {
                            dialogs::show_toast(&parent_clone, &message);
                        }
                        Err(e) => {
                            error_helpers::show_error_with_context(
                                &parent_clone,
                                error_helpers::ErrorContext::SnapshotRestore,
                                &format!("File restoration failed: {e}"),
                            );
                        }
                    }
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    gtk::glib::timeout_future(std::time::Duration::from_millis(50)).await;
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::error!("File restoration thread disconnected");
                    break;
                }
            }
        }
    });
}
