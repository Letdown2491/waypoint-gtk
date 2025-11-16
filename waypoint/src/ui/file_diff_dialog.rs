//! File diff dialog for showing changed files between two snapshots

use adw::prelude::*;
use gtk::prelude::*;
use gtk::{Label, Orientation};
use libadwaita as adw;

use super::dialogs;

/// File change representation (matches waypoint-helper output)
#[derive(Debug, Clone, serde::Deserialize)]
struct FileChange {
    change_type: String, // "Added", "Modified", "Deleted"
    path: String,
}

/// Show dialog displaying file changes between two snapshots
pub fn show_file_diff_dialog(
    parent: &adw::ApplicationWindow,
    old_snapshot: &str,
    new_snapshot: &str,
) {
    // Create full window dialog
    let dialog = adw::Window::new();
    dialog.set_title(Some("File Changes"));
    dialog.set_default_size(800, 600);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(parent));

    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("File Changes", "")));
    content.append(&header);

    // Main content - show loading state initially
    let main_box = gtk::Box::new(Orientation::Vertical, 24);
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);

    // Title
    let title_box = gtk::Box::new(Orientation::Vertical, 6);
    let title = Label::new(Some(&format!("{} → {}", old_snapshot, new_snapshot)));
    title.add_css_class("title-2");
    title.set_halign(gtk::Align::Start);
    title_box.append(&title);

    let subtitle = Label::new(Some("Comparing file changes between snapshots..."));
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(gtk::Align::Start);
    title_box.append(&subtitle);

    let warning = Label::new(Some("Large snapshots may take several minutes to compare."));
    warning.add_css_class("caption");
    warning.add_css_class("dim-label");
    warning.set_halign(gtk::Align::Start);
    warning.set_margin_top(6);
    title_box.append(&warning);

    main_box.append(&title_box);

    // Loading spinner
    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_halign(gtk::Align::Center);
    spinner.set_margin_top(48);
    spinner.set_size_request(48, 48);
    main_box.append(&spinner);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&main_box));
    content.append(&scrolled);

    dialog.set_content(Some(&content));
    dialog.present();

    // Run comparison in background thread via D-Bus
    let (tx, rx) = std::sync::mpsc::channel();
    let old_snapshot_owned = old_snapshot.to_string();
    let new_snapshot_owned = new_snapshot.to_string();

    let (cancel_tx, cancel_rx) = std::sync::mpsc::channel::<()>();

    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<Vec<FileChange>> {
            use crate::dbus_client::WaypointHelperClient;

            let client = WaypointHelperClient::new()?;
            let json = client.compare_snapshots(old_snapshot_owned, new_snapshot_owned)?;
            let changes: Vec<FileChange> = serde_json::from_str(&json)?;
            Ok(changes)
        })();
        let _ = tx.send(result);
    });

    // Poll for result
    let dialog_clone = dialog.clone();
    let parent_clone = parent.clone();
    let old_snapshot_owned = old_snapshot.to_string();
    let new_snapshot_owned = new_snapshot.to_string();

    let _dialog_for_close = dialog.clone();
    dialog.connect_close_request(move |_| {
        let _ = cancel_tx.send(());
        gtk::glib::Propagation::Proceed
    });

    gtk::glib::spawn_future_local(async move {
        loop {
            if cancel_rx.try_recv().is_ok() {
                dialog_clone.close();
                break;
            }
            match rx.try_recv() {
                Ok(result) => {
                    // Remove loading content
                    dialog_clone.set_content(None::<&gtk::Box>);

                    match result {
                        Ok(changes) => {
                            display_changes(
                                &dialog_clone,
                                &old_snapshot_owned,
                                &new_snapshot_owned,
                                changes,
                            );
                        }
                        Err(e) => {
                            let error_msg = e.to_string();

                            // Provide a user-friendly error message for timeout issues
                            if error_msg.contains("timeout") || error_msg.contains("timed out") {
                                dialogs::show_error(
                                    &parent_clone,
                                    "Comparison Timeout",
                                    "The file comparison took too long (>25 seconds).\n\n\
                                     This happens with very large snapshots that have many file changes.\n\n\
                                     Try using \"Compare Packages\" instead, which works for snapshots of any size.",
                                );
                            } else {
                                dialogs::show_error(
                                    &parent_clone,
                                    "Comparison Failed",
                                    &format!("Failed to compare snapshots: {}", e),
                                );
                            }
                            dialog_clone.close();
                        }
                    }
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    gtk::glib::timeout_future(std::time::Duration::from_millis(100)).await;
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::error!("File comparison thread disconnected");
                    dialog_clone.close();
                    break;
                }
            }
        }
    });
}

/// Display the file changes in the dialog
fn display_changes(
    dialog: &adw::Window,
    old_snapshot: &str,
    new_snapshot: &str,
    changes: Vec<FileChange>,
) {
    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("File Changes", "")));
    content.append(&header);

    // Scrollable content
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);

    let main_box = gtk::Box::new(Orientation::Vertical, 24);
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);

    // Title
    let title_box = gtk::Box::new(Orientation::Vertical, 6);
    let title = Label::new(Some(&format!("{} → {}", old_snapshot, new_snapshot)));
    title.add_css_class("title-2");
    title.set_halign(gtk::Align::Start);
    title_box.append(&title);

    let subtitle = Label::new(Some(&format!("{} file(s) changed", changes.len())));
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(gtk::Align::Start);
    title_box.append(&subtitle);
    main_box.append(&title_box);

    if changes.is_empty() {
        // No changes
        let status_page = adw::StatusPage::new();
        status_page.set_icon_name(Some("emblem-ok-symbolic"));
        status_page.set_title("No Changes");
        status_page.set_description(Some("The snapshots are identical"));
        main_box.append(&status_page);
    } else {
        // Group changes by type
        let mut added: Vec<&FileChange> = Vec::new();
        let mut modified: Vec<&FileChange> = Vec::new();
        let mut deleted: Vec<&FileChange> = Vec::new();

        for change in &changes {
            match change.change_type.as_str() {
                "Added" => added.push(change),
                "Modified" => modified.push(change),
                "Deleted" => deleted.push(change),
                _ => {} // Unknown type, skip
            }
        }

        // Display each category
        if !added.is_empty() {
            let group = create_change_group("Added Files", &added, "list-add-symbolic", "success");
            main_box.append(&group);
        }

        if !modified.is_empty() {
            let group = create_change_group(
                "Modified Files",
                &modified,
                "document-edit-symbolic",
                "warning",
            );
            main_box.append(&group);
        }

        if !deleted.is_empty() {
            let group =
                create_change_group("Deleted Files", &deleted, "list-remove-symbolic", "error");
            main_box.append(&group);
        }
    }

    scrolled.set_child(Some(&main_box));
    content.append(&scrolled);

    dialog.set_content(Some(&content));
}

/// Create a group widget for a category of changes
fn create_change_group(
    title: &str,
    changes: &[&FileChange],
    icon_name: &str,
    css_class: &str,
) -> gtk::Box {
    let group_box = gtk::Box::new(Orientation::Vertical, 12);

    // Group header
    let header_box = gtk::Box::new(Orientation::Horizontal, 12);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class(css_class);
    header_box.append(&icon);

    let header_label = Label::new(Some(&format!("{} ({})", title, changes.len())));
    header_label.add_css_class("title-4");
    header_label.set_halign(gtk::Align::Start);
    header_box.append(&header_label);

    group_box.append(&header_box);

    // List of files
    let list_box = gtk::ListBox::new();
    list_box.add_css_class("boxed-list");
    list_box.set_selection_mode(gtk::SelectionMode::None);

    for change in changes {
        let row = adw::ActionRow::new();
        row.set_title(&change.path);

        // Add icon based on change type
        let change_icon = gtk::Image::from_icon_name(icon_name);
        change_icon.add_css_class(css_class);
        row.add_prefix(&change_icon);

        list_box.append(&row);
    }

    group_box.append(&list_box);

    group_box
}
