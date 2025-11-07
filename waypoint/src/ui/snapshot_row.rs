use crate::snapshot::{Snapshot, format_bytes};
use gtk::prelude::*;
use gtk::{Box, Button, Orientation};
use libadwaita as adw;
use adw::prelude::*;

pub struct SnapshotRow {
    row: adw::ActionRow,
}

pub enum SnapshotAction {
    Browse,
    Restore,
    Delete,
}

impl SnapshotRow {
    pub fn new<F>(snapshot: &Snapshot, on_action: F) -> adw::ActionRow
    where
        F: Fn(String, SnapshotAction) + 'static,
    {
        let row = adw::ActionRow::new();
        row.set_title(&snapshot.name);

        // Build subtitle with metadata - cleaner format
        let mut subtitle_parts = vec![snapshot.format_timestamp()];

        // Add size if available
        if let Some(size) = snapshot.size_bytes {
            subtitle_parts.push(format_bytes(size));
        }

        if let Some(count) = snapshot.package_count {
            subtitle_parts.push(format!("{} packages", count));
        }

        if let Some(kernel) = &snapshot.kernel_version {
            // Only show first part of kernel version (e.g., "6.6.54" instead of full version string)
            if let Some(short_version) = kernel.split_whitespace().next() {
                subtitle_parts.push(format!("Kernel {}", short_version));
            }
        }

        row.set_subtitle(&subtitle_parts.join("  •  "));

        // Add action buttons with better spacing
        let button_box = Box::new(Orientation::Horizontal, 0);
        button_box.add_css_class("linked");

        let browse_btn = Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text("Browse Files")
            .valign(gtk::Align::Center)
            .build();
        browse_btn.add_css_class("flat");

        let restore_btn = Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Restore System to This Point")
            .valign(gtk::Align::Center)
            .build();
        restore_btn.add_css_class("flat");

        let delete_btn = Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Delete Restore Point")
            .valign(gtk::Align::Center)
            .build();
        delete_btn.add_css_class("flat");
        delete_btn.add_css_class("destructive-action");

        // Connect button signals
        let snapshot_id = snapshot.id.clone();
        let callback = std::rc::Rc::new(on_action);

        let id_clone = snapshot_id.clone();
        let cb_clone = callback.clone();
        browse_btn.connect_clicked(move |_| {
            cb_clone(id_clone.clone(), SnapshotAction::Browse);
        });

        let id_clone = snapshot_id.clone();
        let cb_clone = callback.clone();
        restore_btn.connect_clicked(move |_| {
            cb_clone(id_clone.clone(), SnapshotAction::Restore);
        });

        let id_clone = snapshot_id.clone();
        let cb_clone = callback.clone();
        delete_btn.connect_clicked(move |_| {
            cb_clone(id_clone.clone(), SnapshotAction::Delete);
        });

        button_box.append(&browse_btn);
        button_box.append(&restore_btn);
        button_box.append(&delete_btn);

        row.add_suffix(&button_box);
        row.set_activatable(false);

        row
    }
}

impl std::ops::Deref for SnapshotRow {
    type Target = adw::ActionRow;

    fn deref(&self) -> &Self::Target {
        &self.row
    }
}
