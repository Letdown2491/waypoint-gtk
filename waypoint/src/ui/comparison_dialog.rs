use crate::snapshot::SnapshotManager;
use adw::prelude::*;
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

use super::comparison_view::ComparisonView;
use super::dialogs;

/// Show dialog to compare two snapshots
pub fn show_compare_dialog(
    window: &adw::ApplicationWindow,
    manager: &Rc<RefCell<SnapshotManager>>,
) {
    let snapshots = match manager.borrow().load_snapshots() {
        Ok(s) => s,
        Err(e) => {
            dialogs::show_error(window, "Error", &format!("Failed to load snapshots: {e}"));
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

    // Create comparison dialog with navigation view
    let dialog = adw::Window::new();
    dialog.set_title(Some("Compare Snapshots"));
    dialog.set_default_size(850, 700);
    dialog.set_modal(true);
    dialog.set_transient_for(Some(window));

    // Create the comparison view with snapshots
    let comparison_view = ComparisonView::new(snapshots);

    // Set the comparison view as dialog content
    dialog.set_content(Some(comparison_view.widget()));

    // Present the dialog
    dialog.present();
}
