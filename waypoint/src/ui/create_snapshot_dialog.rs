use gtk::prelude::*;
use gtk::{Entry, Label, Orientation};
use libadwaita as adw;
use adw::prelude::*;

/// Sanitize description text to prevent issues
fn sanitize_description(desc: &str) -> String {
    // Trim whitespace and limit length
    let mut sanitized = desc.trim().to_string();

    // Limit to reasonable length (500 characters)
    if sanitized.len() > 500 {
        sanitized.truncate(500);
        sanitized.push_str("...");
    }

    // Remove null bytes
    sanitized = sanitized.replace('\0', "");

    sanitized
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
            let description = sanitize_description(&desc_entry.text());
            callback(Some((default_name_clone.clone(), description)));
        } else {
            callback(None);
        }
    });

    dialog.present();
}

#[cfg(test)]
mod tests {
    use super::*;
    use waypoint_common::validate_snapshot_name;

    #[test]
    fn test_valid_snapshot_names() {
        assert!(validate_snapshot_name("snapshot-001").is_ok());
        assert!(validate_snapshot_name("backup_2024").is_ok());
        assert!(validate_snapshot_name("pre-upgrade").is_ok());
        assert!(validate_snapshot_name("my-snapshot").is_ok());
        assert!(validate_snapshot_name("waypoint-20241108-120000").is_ok());
    }

    #[test]
    fn test_invalid_snapshot_names() {
        // Empty or too long
        assert!(validate_snapshot_name("").is_err());
        assert!(validate_snapshot_name(&"a".repeat(256)).is_err());

        // Path traversal
        assert!(validate_snapshot_name("../etc").is_err());
        assert!(validate_snapshot_name("test/../root").is_err());
        assert!(validate_snapshot_name(".").is_err());
        assert!(validate_snapshot_name("..").is_err());

        // Dangerous characters
        assert!(validate_snapshot_name("test/path").is_err());
        assert!(validate_snapshot_name("test\0null").is_err());

        // Starting with special chars
        assert!(validate_snapshot_name("-snapshot").is_err());
        assert!(validate_snapshot_name(".hidden").is_err());
    }

    #[test]
    fn test_sanitize_description() {
        // Trim whitespace
        assert_eq!(sanitize_description("  test  "), "test");

        // Remove null bytes
        assert_eq!(sanitize_description("test\0null"), "testnull");

        // Limit length
        let long_desc = "a".repeat(600);
        let sanitized = sanitize_description(&long_desc);
        assert!(sanitized.len() <= 503); // 500 + "..."
        assert!(sanitized.ends_with("..."));

        // Normal description
        assert_eq!(sanitize_description("Before upgrade"), "Before upgrade");
    }
}
