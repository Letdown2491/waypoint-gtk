use crate::snapshot::{Snapshot, format_bytes};
use crate::user_preferences::SnapshotPreferences;
use gtk::prelude::*;
use gtk::{Box, Button, Orientation};
use libadwaita as adw;
use adw::prelude::*;

pub struct SnapshotRow {
    row: adw::ActionRow,
}

pub enum SnapshotAction {
    Browse,
    Verify,
    Restore,
    Delete,
    ToggleFavorite,
    EditNote,
    Backup,
}

/// Backup status for a snapshot
#[derive(Debug, Clone, PartialEq)]
pub enum BackupStatus {
    /// Not backed up to any destination
    NotBackedUp,
    /// Backed up to all enabled destinations
    FullyBackedUp,
    /// Backed up to some but not all enabled destinations
    PartiallyBackedUp(usize, usize), // (backed_up_count, total_count)
    /// Has pending backups
    Pending,
    /// Has failed backups
    Failed,
}

impl SnapshotRow {
    #[allow(dead_code)]
    pub fn new<F>(snapshot: &Snapshot, on_action: F) -> adw::ActionRow
    where
        F: Fn(String, SnapshotAction) + 'static,
    {
        Self::new_with_context(
            snapshot,
            &SnapshotPreferences::default(),
            on_action,
            None,
            &BackupStatus::NotBackedUp,
        )
    }

    pub fn new_with_context<F>(
        snapshot: &Snapshot,
        preferences: &SnapshotPreferences,
        on_action: F,
        max_size: Option<u64>,
        backup_status: &BackupStatus,
    ) -> adw::ActionRow
    where
        F: Fn(String, SnapshotAction) + 'static,
    {
        let row = adw::ActionRow::new();
        row.set_title(&snapshot.name);

        // Create prefix box for waypoint icon + backup status
        let prefix_box = Box::new(Orientation::Horizontal, 4);

        // Add waypoint icon as prefix
        let icon = gtk::Image::from_icon_name("waypoint");
        icon.set_pixel_size(16);
        prefix_box.append(&icon);

        // Add backup status indicator
        match backup_status {
            BackupStatus::FullyBackedUp => {
                let backup_icon = gtk::Image::from_icon_name("emblem-ok-symbolic");
                backup_icon.set_pixel_size(12);
                backup_icon.set_tooltip_text(Some("Backed up to all destinations"));
                backup_icon.add_css_class("success");
                prefix_box.append(&backup_icon);
            }
            BackupStatus::PartiallyBackedUp(count, total) => {
                let backup_icon = gtk::Image::from_icon_name("emblem-important-symbolic");
                backup_icon.set_pixel_size(12);
                backup_icon.set_tooltip_text(Some(&format!("Backed up to {} of {} destinations", count, total)));
                backup_icon.add_css_class("warning");
                prefix_box.append(&backup_icon);
            }
            BackupStatus::Pending => {
                let backup_icon = gtk::Image::from_icon_name("document-save-symbolic");
                backup_icon.set_pixel_size(12);
                backup_icon.set_tooltip_text(Some("Backup pending"));
                backup_icon.add_css_class("dim-label");
                prefix_box.append(&backup_icon);
            }
            BackupStatus::Failed => {
                let backup_icon = gtk::Image::from_icon_name("dialog-error-symbolic");
                backup_icon.set_pixel_size(12);
                backup_icon.set_tooltip_text(Some("Backup failed"));
                backup_icon.add_css_class("error");
                prefix_box.append(&backup_icon);
            }
            BackupStatus::NotBackedUp => {
                // No icon for not backed up state
            }
        }

        row.add_prefix(&prefix_box);

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

        // Build subtitle text with optional note
        let subtitle = if let Some(note) = &preferences.note {
            // Truncate note if too long (show first 60 chars + ellipsis)
            let note_preview = if note.len() > 60 {
                format!("{}…", &note.chars().take(60).collect::<String>().trim())
            } else {
                note.to_string()
            };
            format!("{}\nNote: {}", subtitle_parts.join("  •  "), note_preview)
        } else {
            subtitle_parts.join("  •  ")
        };

        row.set_subtitle(&subtitle);

        // Add size indicator if size is available and max_size is provided
        if let (Some(size), Some(max)) = (snapshot.size_bytes, max_size) {
            if max > 0 {
                let size_box = Box::new(Orientation::Vertical, 4);
                size_box.set_valign(gtk::Align::Center);
                size_box.set_margin_end(12);

                // Size label
                let size_label = gtk::Label::new(Some(&format_bytes(size)));
                size_label.add_css_class("caption");
                size_label.add_css_class("dim-label");
                size_label.set_halign(gtk::Align::End);
                size_box.append(&size_label);

                // Level bar showing relative size
                let level_bar = gtk::LevelBar::new();
                level_bar.set_min_value(0.0);
                level_bar.set_max_value(1.0);
                level_bar.set_value((size as f64) / (max as f64));
                level_bar.set_width_request(80);
                level_bar.set_valign(gtk::Align::Center);
                size_box.append(&level_bar);

                row.add_suffix(&size_box);
            }
        }

        // Add action buttons - primary action + menu
        let button_box = Box::new(Orientation::Horizontal, 6);

        // Star/favorite button
        let star_btn = Button::builder()
            .icon_name(if preferences.is_favorite {
                "starred-symbolic"
            } else {
                "non-starred-symbolic"
            })
            .tooltip_text(if preferences.is_favorite {
                "Unpin Restore Point"
            } else {
                "Pin Restore Point"
            })
            .valign(gtk::Align::Center)
            .build();
        star_btn.add_css_class("flat");

        // Primary action: Restore button
        let restore_btn = Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Restore System to This Point")
            .valign(gtk::Align::Center)
            .build();
        restore_btn.add_css_class("flat");

        // Menu button for secondary actions
        let menu_btn = gtk::MenuButton::new();
        menu_btn.set_icon_name("view-more-symbolic");
        menu_btn.set_tooltip_text(Some("More Actions"));
        menu_btn.set_valign(gtk::Align::Center);
        menu_btn.add_css_class("flat");

        // Create popover menu
        let menu = gtk::gio::Menu::new();

        // Browse action
        let browse_action_name = format!("snapshot.browse-{}", snapshot.id.replace('/', "-"));
        menu.append(Some("Browse Files"), Some(&browse_action_name));

        // Verify action
        let verify_action_name = format!("snapshot.verify-{}", snapshot.id.replace('/', "-"));
        menu.append(Some("Verify Integrity"), Some(&verify_action_name));

        // Backup action
        let backup_action_name = format!("snapshot.backup-{}", snapshot.id.replace('/', "-"));
        menu.append(Some("Backup to External Drive"), Some(&backup_action_name));

        // Edit Note action
        let edit_note_action_name = format!("snapshot.edit-note-{}", snapshot.id.replace('/', "-"));
        menu.append(Some("Edit Note"), Some(&edit_note_action_name));

        // Delete action in a separate section (creates visual separator)
        let delete_section = gtk::gio::Menu::new();
        let delete_action_name = format!("snapshot.delete-{}", snapshot.id.replace('/', "-"));
        delete_section.append(Some("Delete Restore Point"), Some(&delete_action_name));
        menu.append_section(None, &delete_section);

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        menu_btn.set_popover(Some(&popover));

        // Connect buttons
        let snapshot_id = snapshot.id.clone();
        let callback = std::rc::Rc::new(on_action);

        // Connect star button
        let id_clone = snapshot_id.clone();
        let cb_clone = callback.clone();
        star_btn.connect_clicked(move |_| {
            cb_clone(id_clone.clone(), SnapshotAction::ToggleFavorite);
        });

        // Connect restore button
        let id_clone = snapshot_id.clone();
        let cb_clone = callback.clone();
        restore_btn.connect_clicked(move |_| {
            cb_clone(id_clone.clone(), SnapshotAction::Restore);
        });

        // Create action group for this row's menu actions
        let action_group = gtk::gio::SimpleActionGroup::new();

        // Browse action
        let browse_action = gtk::gio::SimpleAction::new(&format!("browse-{}", snapshot.id.replace('/', "-")), None);
        let browse_id = snapshot.id.clone();
        let browse_cb = callback.clone();
        browse_action.connect_activate(move |_, _| {
            browse_cb(browse_id.clone(), SnapshotAction::Browse);
        });
        action_group.add_action(&browse_action);

        // Verify action
        let verify_action = gtk::gio::SimpleAction::new(&format!("verify-{}", snapshot.id.replace('/', "-")), None);
        let verify_id = snapshot.id.clone();
        let verify_cb = callback.clone();
        verify_action.connect_activate(move |_, _| {
            verify_cb(verify_id.clone(), SnapshotAction::Verify);
        });
        action_group.add_action(&verify_action);

        // Backup action
        let backup_action = gtk::gio::SimpleAction::new(&format!("backup-{}", snapshot.id.replace('/', "-")), None);
        let backup_id = snapshot.id.clone();
        let backup_cb = callback.clone();
        backup_action.connect_activate(move |_, _| {
            backup_cb(backup_id.clone(), SnapshotAction::Backup);
        });
        action_group.add_action(&backup_action);

        // Edit Note action
        let edit_note_action = gtk::gio::SimpleAction::new(&format!("edit-note-{}", snapshot.id.replace('/', "-")), None);
        let edit_note_id = snapshot.id.clone();
        let edit_note_cb = callback.clone();
        edit_note_action.connect_activate(move |_, _| {
            edit_note_cb(edit_note_id.clone(), SnapshotAction::EditNote);
        });
        action_group.add_action(&edit_note_action);

        // Delete action
        let delete_action = gtk::gio::SimpleAction::new(&format!("delete-{}", snapshot.id.replace('/', "-")), None);
        let delete_id = snapshot.id.clone();
        let delete_cb = callback.clone();
        delete_action.connect_activate(move |_, _| {
            delete_cb(delete_id.clone(), SnapshotAction::Delete);
        });
        action_group.add_action(&delete_action);

        // Insert the action group into the row
        row.insert_action_group("snapshot", Some(&action_group));

        button_box.append(&star_btn);
        button_box.append(&restore_btn);
        button_box.append(&menu_btn);

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
