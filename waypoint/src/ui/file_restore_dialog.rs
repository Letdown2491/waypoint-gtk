//! File restoration dialog for browsing and restoring files from snapshots
//!
//! Note: Uses GTK4's FileChooserDialog which is deprecated since 4.10,
//! but remains the most practical solution for file selection until a better
//! alternative is available in GTK/Libadwaita.

#![allow(deprecated)]

use crate::dbus_client::WaypointHelperClient;
use gtk::prelude::*;
use gtk::{FileChooserAction, FileChooserDialog, ResponseType};
use libadwaita as adw;
use adw::prelude::*;
use std::path::PathBuf;
use waypoint_common::WaypointConfig;

use super::dialogs;
use super::error_helpers;

/// Show file browser dialog for restoring files from a snapshot
pub fn show_file_restore_dialog(parent: &adw::ApplicationWindow, snapshot_name: &str) {
    let config = WaypointConfig::new();
    let snapshot_path = config
        .snapshot_dir
        .join(snapshot_name)
        .join("root");

    // Verify snapshot exists
    if !snapshot_path.exists() {
        error_helpers::show_error_with_context(
            parent,
            error_helpers::ErrorContext::SnapshotRestore,
            &format!("Snapshot directory not found: {}", snapshot_path.display()),
        );
        return;
    }

    // Create file chooser dialog
    let dialog = FileChooserDialog::new(
        Some(&format!("Browse Snapshot: {}", snapshot_name)),
        Some(parent),
        FileChooserAction::Open,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Restore Selected Files", ResponseType::Accept),
        ],
    );

    dialog.set_select_multiple(true);
    dialog.set_modal(true);

    // Set initial folder to snapshot root
    let _ = dialog.set_current_folder(Some(&gtk::gio::File::for_path(&snapshot_path)));

    // Connect response handler
    let snapshot_name_owned = snapshot_name.to_string();
    let parent_clone = parent.clone();

    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            // Get selected files
            let files: Vec<PathBuf> = dialog
                .files()
                .iter::<gtk::gio::File>()
                .filter_map(|f| f.ok())
                .filter_map(|f| f.path())
                .collect();

            if files.is_empty() {
                dialogs::show_toast(&parent_clone, "No files selected");
                dialog.close();
                return;
            }

            // Show restore confirmation dialog
            show_restore_confirmation_dialog(
                &parent_clone,
                &snapshot_name_owned,
                files,
                &snapshot_path,
            );
        }

        dialog.close();
    });

    dialog.present();
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
                .map(|s| format!("/{}", s))
                .unwrap_or_else(|| p.display().to_string())
        })
        .collect();

    let heading = format!("Restore {} file(s) from snapshot '{}'?", file_list.len(), snapshot_name);
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
                show_custom_location_chooser(&parent_clone, &snapshot_name_owned, file_list.clone());
            }
            _ => {} // Cancel - do nothing
        }
    });

    dialog.present();
}

/// Show directory chooser for custom restore location
fn show_custom_location_chooser(parent: &adw::ApplicationWindow, snapshot_name: &str, file_paths: Vec<String>) {
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
    gtk::glib::spawn_future_local(async move {
        loop {
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
                                &format!("File restoration failed: {}", e),
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
