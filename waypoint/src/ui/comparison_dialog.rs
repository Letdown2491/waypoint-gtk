use crate::snapshot::SnapshotManager;
use gtk::{Label, Orientation};
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::dialogs;
use super::package_diff_dialog;

/// Show dialog to compare two snapshots
pub fn show_compare_dialog(window: &adw::ApplicationWindow, manager: &Rc<RefCell<SnapshotManager>>) {
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
