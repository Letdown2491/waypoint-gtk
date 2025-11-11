use gtk::prelude::*;
use gtk::{Box, Button, Label, Orientation, SpinButton};
use libadwaita as adw;
use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::retention::RetentionPolicy;
use crate::snapshot::SnapshotManager;
use super::dialogs;

/// Create and show the retention policy editor dialog
pub fn show_retention_editor(
    parent: &adw::ApplicationWindow,
    manager: &Rc<RefCell<SnapshotManager>>,
) {
    // Load current policy
    let current_policy = RetentionPolicy::load().unwrap_or_default();

    // Create dialog
    let dialog = adw::Window::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_title(Some("Retention Policy"));
    dialog.set_default_size(500, 600);

    // Main container
    let main_box = Box::new(Orientation::Vertical, 0);

    // Header bar
    let header = adw::HeaderBar::new();
    main_box.append(&header);

    // Scrolled window for content
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);
    main_box.append(&scrolled);

    // Content box
    let content_box = Box::new(Orientation::Vertical, 24);
    content_box.set_margin_top(24);
    content_box.set_margin_bottom(24);
    content_box.set_margin_start(24);
    content_box.set_margin_end(24);
    scrolled.set_child(Some(&content_box));

    // Settings group
    let settings_group = adw::PreferencesGroup::new();
    settings_group.set_title("Policy Rules");
    content_box.append(&settings_group);

    // Max snapshots setting
    let max_snapshots_row = adw::ActionRow::new();
    max_snapshots_row.set_title("Maximum Snapshots");
    max_snapshots_row.set_subtitle("Keep at most this many snapshots (0 = unlimited)");
    let max_snapshots_spin = SpinButton::with_range(0.0, 100.0, 1.0);
    max_snapshots_spin.set_value(current_policy.max_snapshots as f64);
    max_snapshots_spin.set_valign(gtk::Align::Center);
    max_snapshots_row.add_suffix(&max_snapshots_spin);
    settings_group.add(&max_snapshots_row);

    // Max age setting
    let max_age_row = adw::ActionRow::new();
    max_age_row.set_title("Maximum Age");
    max_age_row.set_subtitle("Delete snapshots older than this many days (0 = unlimited)");
    let max_age_spin = SpinButton::with_range(0.0, 365.0, 1.0);
    max_age_spin.set_value(current_policy.max_age_days as f64);
    max_age_spin.set_valign(gtk::Align::Center);
    max_age_row.add_suffix(&max_age_spin);
    settings_group.add(&max_age_row);

    // Min snapshots setting
    let min_snapshots_row = adw::ActionRow::new();
    min_snapshots_row.set_title("Minimum Snapshots");
    min_snapshots_row.set_subtitle("Always keep at least this many snapshots, regardless of age");
    let min_snapshots_spin = SpinButton::with_range(0.0, 50.0, 1.0);
    min_snapshots_spin.set_value(current_policy.min_snapshots as f64);
    min_snapshots_spin.set_valign(gtk::Align::Center);
    min_snapshots_row.add_suffix(&min_snapshots_spin);
    settings_group.add(&min_snapshots_row);

    // Keep patterns group
    let patterns_group = adw::PreferencesGroup::new();
    patterns_group.set_title("Keep Patterns");
    patterns_group.set_description(Some(
        "Snapshots with names containing these patterns will never be automatically deleted"
    ));
    content_box.append(&patterns_group);

    // Store patterns in a shared vector
    let patterns = Rc::new(RefCell::new(current_policy.keep_patterns.clone()));

    // Pattern list box
    let pattern_list = gtk::ListBox::new();
    pattern_list.set_selection_mode(gtk::SelectionMode::None);
    pattern_list.add_css_class("boxed-list");

    // Function to refresh pattern list
    let refresh_patterns = {
        let pattern_list = pattern_list.clone();
        let patterns = patterns.clone();
        Rc::new(move || {
            // Clear existing rows
            while let Some(child) = pattern_list.first_child() {
                pattern_list.remove(&child);
            }

            // Add rows for each pattern
            let patterns_borrowed = patterns.borrow();
            if patterns_borrowed.is_empty() {
                let placeholder_row = adw::ActionRow::new();
                placeholder_row.set_title("No patterns configured");
                placeholder_row.set_sensitive(false);
                pattern_list.append(&placeholder_row);
            } else {
                for pattern in patterns_borrowed.iter() {
                    let row = adw::ActionRow::new();
                    row.set_title(pattern);

                    let remove_btn = Button::new();
                    remove_btn.set_icon_name("user-trash-symbolic");
                    remove_btn.set_valign(gtk::Align::Center);
                    remove_btn.add_css_class("flat");

                    let patterns_clone = patterns.clone();
                    let pattern_list_clone = pattern_list.clone();
                    let pattern_to_remove = pattern.clone();
                    remove_btn.connect_clicked(move |_| {
                        // Remove by value, not by index
                        patterns_clone.borrow_mut().retain(|p| p != &pattern_to_remove);

                        // Refresh the list manually
                        while let Some(child) = pattern_list_clone.first_child() {
                            pattern_list_clone.remove(&child);
                        }
                        let pats = patterns_clone.borrow();
                        if pats.is_empty() {
                            let placeholder = adw::ActionRow::new();
                            placeholder.set_title("No patterns configured");
                            placeholder.set_sensitive(false);
                            pattern_list_clone.append(&placeholder);
                        } else {
                            for pat in pats.iter() {
                                let r = adw::ActionRow::new();
                                r.set_title(pat);
                                pattern_list_clone.append(&r);
                            }
                        }
                    });

                    row.add_suffix(&remove_btn);
                    pattern_list.append(&row);
                }
            }
        })
    };

    refresh_patterns();
    patterns_group.add(&pattern_list);

    // Add pattern entry
    let add_pattern_box = Box::new(Orientation::Horizontal, 12);
    add_pattern_box.set_margin_top(12);
    let pattern_entry = gtk::Entry::new();
    pattern_entry.set_placeholder_text(Some("Enter pattern (e.g., pre-upgrade)"));
    pattern_entry.set_hexpand(true);
    add_pattern_box.append(&pattern_entry);

    let add_pattern_btn = Button::with_label("Add Pattern");
    add_pattern_btn.add_css_class("suggested-action");
    add_pattern_box.append(&add_pattern_btn);

    patterns_group.add(&add_pattern_box);

    // Add pattern button handler
    let patterns_for_add = patterns.clone();
    let pattern_entry_for_add = pattern_entry.clone();
    let refresh_patterns_for_add = refresh_patterns.clone();
    add_pattern_btn.connect_clicked(move |_| {
        let text = pattern_entry_for_add.text().to_string();
        if !text.is_empty() {
            patterns_for_add.borrow_mut().push(text);
            pattern_entry_for_add.set_text("");
            refresh_patterns_for_add();
        }
    });

    // Current cleanup status (based on saved policy)
    let cleanup_group = adw::PreferencesGroup::new();
    cleanup_group.set_title("Snapshots to Clean Up");
    cleanup_group.set_description(Some("Based on currently saved retention policy"));
    content_box.append(&cleanup_group);

    if let Ok(to_cleanup) = manager.borrow().get_snapshots_to_cleanup() {
        let cleanup_row = adw::ActionRow::new();
        if to_cleanup.is_empty() {
            cleanup_row.set_title("No snapshots will be automatically deleted");
            let ok_icon = gtk::Image::from_icon_name("emblem-ok-symbolic");
            cleanup_row.add_prefix(&ok_icon);
        } else {
            cleanup_row.set_title(&format!("{} snapshot{} will be deleted on next cleanup",
                to_cleanup.len(),
                if to_cleanup.len() == 1 { "" } else { "s" }
            ));
            let trash_icon = gtk::Image::from_icon_name("user-trash-symbolic");
            cleanup_row.add_prefix(&trash_icon);
        }
        cleanup_group.add(&cleanup_row);
    }

    // Preview group
    let preview_group = adw::PreferencesGroup::new();
    preview_group.set_title("Preview");
    preview_group.set_description(Some("Snapshots that would be deleted with current settings"));
    content_box.append(&preview_group);

    let preview_label = Label::new(Some("Calculating..."));
    preview_label.set_wrap(true);
    preview_label.set_halign(gtk::Align::Start);
    preview_label.add_css_class("dim-label");
    preview_group.add(&preview_label);

    // Update preview when values change
    let preview_label_for_update = preview_label.clone();
    let manager_for_update = manager.clone();
    let patterns_for_update = patterns.clone();
    let max_snapshots_spin_for_update = max_snapshots_spin.clone();
    let max_age_spin_for_update = max_age_spin.clone();
    let min_snapshots_spin_for_update = min_snapshots_spin.clone();

    let update_preview = Rc::new(move || {
        let max_snap = max_snapshots_spin_for_update.value() as usize;
        let max_age = max_age_spin_for_update.value() as u32;
        let min_snap = min_snapshots_spin_for_update.value() as usize;
        let pats = patterns_for_update.borrow().clone();

        let policy = RetentionPolicy {
            max_snapshots: max_snap,
            max_age_days: max_age,
            min_snapshots: min_snap,
            keep_patterns: pats,
        };

        if let Ok(snapshots) = manager_for_update.borrow().load_snapshots() {
            let to_delete = policy.apply(&snapshots);
            let text = if to_delete.is_empty() {
                "✓ No snapshots will be deleted with these settings".to_string()
            } else {
                let more_text = if to_delete.len() > 5 {
                    format!("\n  ... and {} more", to_delete.len() - 5)
                } else {
                    String::new()
                };
                format!("⚠ {} snapshot{} will be deleted:\n{}{}",
                    to_delete.len(),
                    if to_delete.len() == 1 { "" } else { "s" },
                    to_delete.iter()
                        .take(5)
                        .map(|s| format!("  • {}", s))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    more_text
                )
            };
            preview_label_for_update.set_text(&text);
        }
    });

    max_snapshots_spin.connect_value_changed({
        let update = update_preview.clone();
        move |_| update()
    });
    max_age_spin.connect_value_changed({
        let update = update_preview.clone();
        move |_| update()
    });
    min_snapshots_spin.connect_value_changed({
        let update = update_preview.clone();
        move |_| update()
    });

    // Initial preview
    update_preview();

    // Button box at bottom
    let button_box = Box::new(Orientation::Horizontal, 12);
    button_box.set_margin_top(12);
    button_box.set_margin_bottom(12);
    button_box.set_margin_start(12);
    button_box.set_margin_end(12);
    button_box.set_halign(gtk::Align::End);
    main_box.append(&button_box);

    let cancel_btn = Button::with_label("Cancel");
    button_box.append(&cancel_btn);

    let save_btn = Button::with_label("Save & Apply");
    save_btn.add_css_class("suggested-action");
    button_box.append(&save_btn);

    // Cancel button handler
    let dialog_for_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });

    // Save button handler
    let dialog_for_save = dialog.clone();
    let parent_for_save = parent.clone();
    save_btn.connect_clicked(move |_| {
        let policy = RetentionPolicy {
            max_snapshots: max_snapshots_spin.value() as usize,
            max_age_days: max_age_spin.value() as u32,
            min_snapshots: min_snapshots_spin.value() as usize,
            keep_patterns: patterns.borrow().clone(),
        };

        match policy.save() {
            Ok(_) => {
                // Show success toast
                if let Some(window) = parent_for_save.downcast_ref::<adw::ApplicationWindow>() {
                    dialogs::show_toast(window, "Retention policy saved successfully");
                }

                dialog_for_save.close();
            }
            Err(e) => {
                log::error!("Failed to save retention policy: {}", e);
                let error_dialog = adw::MessageDialog::new(Some(&dialog_for_save), Some("Save Failed"), Some(&format!("Failed to save retention policy: {}", e)));
                error_dialog.add_response("ok", "OK");
                error_dialog.set_default_response(Some("ok"));
                error_dialog.present();
            }
        }
    });

    dialog.set_content(Some(&main_box));
    dialog.present();
}
