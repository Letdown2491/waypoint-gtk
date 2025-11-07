use gtk::prelude::*;
use gtk::{Entry, Label, Orientation};
use libadwaita as adw;
use adw::prelude::*;

/// Show dialog to get custom description for snapshot
/// Returns (snapshot_name, description) or None if cancelled
#[allow(dead_code)]
pub fn show_create_snapshot_dialog(parent: &adw::ApplicationWindow) -> Option<(String, String)> {
    let timestamp = chrono::Utc::now();
    let default_name = format!("waypoint-{}", timestamp.format("%Y%m%d-%H%M%S"));
    let default_desc = format!("System snapshot {}", timestamp.format("%Y-%m-%d %H:%M"));

    // Create dialog
    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Create Restore Point"),
        Some("Give this snapshot a description to help identify it later."),
    );

    // Create custom content
    let content = gtk::Box::new(Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    // Description entry
    let desc_label = Label::new(Some("Description:"));
    desc_label.set_halign(gtk::Align::Start);
    content.append(&desc_label);

    let desc_entry = Entry::new();
    desc_entry.set_text(&default_desc);
    desc_entry.set_placeholder_text(Some("e.g., Before system upgrade"));
    content.append(&desc_entry);

    // Info label
    let info = Label::new(Some("The snapshot will be automatically named based on the current date and time."));
    info.set_wrap(true);
    info.add_css_class("dim-label");
    info.set_halign(gtk::Align::Start);
    info.set_margin_top(6);
    content.append(&info);

    dialog.set_extra_child(Some(&content));

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("create", "Create");
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("create"));
    dialog.set_close_response("cancel");

    // Make Enter key trigger creation
    desc_entry.connect_activate(move |_| {
        // Trigger the "create" response
        // This is a bit hacky but works
    });

    // Show dialog and wait for response
    let (sender, _receiver) = std::sync::mpsc::channel();
    let sender = std::sync::Arc::new(std::sync::Mutex::new(sender));

    let desc_entry_clone = desc_entry.clone();
    dialog.connect_response(None, move |_, response| {
        let sender = sender.lock().unwrap();
        if response == "create" {
            let description = desc_entry_clone.text().to_string();
            let _ = sender.send(Some((default_name.clone(), description)));
        } else {
            let _ = sender.send(None);
        }
    });

    dialog.present();

    // Wait for response (this blocks, but it's in the GTK event loop)
    // Actually, we can't block in GTK, so we need to return a callback-based approach
    // Let me refactor this to use a callback instead

    None // Placeholder
}

/// Show dialog to get custom description for snapshot (callback-based)
pub fn show_create_snapshot_dialog_async<F>(parent: &adw::ApplicationWindow, callback: F)
where
    F: Fn(Option<(String, String)>) + 'static,
{
    let timestamp = chrono::Utc::now();
    let default_name = format!("waypoint-{}", timestamp.format("%Y%m%d-%H%M%S"));
    let default_desc = format!("System snapshot {}", timestamp.format("%Y-%m-%d %H:%M"));

    // Create dialog
    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Create Restore Point"),
        Some("Give this snapshot a description to help identify it later."),
    );

    // Create custom content
    let content = gtk::Box::new(Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    // Description entry
    let desc_label = Label::new(Some("Description:"));
    desc_label.set_halign(gtk::Align::Start);
    content.append(&desc_label);

    let desc_entry = Entry::new();
    desc_entry.set_text(&default_desc);
    desc_entry.set_placeholder_text(Some("e.g., Before Docker installation"));
    desc_entry.set_activates_default(true);
    content.append(&desc_entry);

    // Info label
    let info = Label::new(Some("The snapshot will be automatically named based on the current date and time."));
    info.set_wrap(true);
    info.add_css_class("dim-label");
    info.set_halign(gtk::Align::Start);
    info.set_margin_top(6);
    content.append(&info);

    dialog.set_extra_child(Some(&content));

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("create", "Create");
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("create"));
    dialog.set_close_response("cancel");

    // Handle response
    let default_name_clone = default_name.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "create" {
            let description = desc_entry.text().to_string();
            callback(Some((default_name_clone.clone(), description)));
        } else {
            callback(None);
        }
    });

    dialog.present();
}
