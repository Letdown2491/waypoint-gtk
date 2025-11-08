use gtk::prelude::*;
use gtk::{Entry, Label, Orientation};
use libadwaita as adw;
use adw::prelude::*;

/// Validate snapshot name for security and filesystem compatibility
///
/// # Arguments
/// * `name` - The snapshot name to validate
///
/// # Returns
/// `true` if the name is valid and safe to use, `false` otherwise
///
/// # Validation Rules
/// - Name must not be empty and must be ≤ 255 characters
/// - Cannot contain `/`, null bytes, or `..`
/// - Cannot start with `-` or `.`
/// - Cannot be exactly `.` or `..`
///
/// # Note
/// This function is tested but not yet used in production code.
/// It's available for future validation requirements.
#[allow(dead_code)]
pub fn is_valid_snapshot_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }

    // Reject names with problematic characters
    if name.contains('/') || name.contains('\0') || name.contains("..") {
        return false;
    }

    // Reject names starting with - or .
    if name.starts_with('-') || name.starts_with('.') {
        return false;
    }

    // Reject special names
    if name == "." || name == ".." {
        return false;
    }

    true
}

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

    #[test]
    fn test_valid_snapshot_names() {
        assert!(is_valid_snapshot_name("snapshot-001"));
        assert!(is_valid_snapshot_name("backup_2024"));
        assert!(is_valid_snapshot_name("pre-upgrade"));
        assert!(is_valid_snapshot_name("my-snapshot"));
        assert!(is_valid_snapshot_name("waypoint-20241108-120000"));
    }

    #[test]
    fn test_invalid_snapshot_names() {
        // Empty or too long
        assert!(!is_valid_snapshot_name(""));
        assert!(!is_valid_snapshot_name(&"a".repeat(256)));

        // Path traversal
        assert!(!is_valid_snapshot_name("../etc"));
        assert!(!is_valid_snapshot_name("test/../root"));
        assert!(!is_valid_snapshot_name("."));
        assert!(!is_valid_snapshot_name(".."));

        // Dangerous characters
        assert!(!is_valid_snapshot_name("test/path"));
        assert!(!is_valid_snapshot_name("test\0null"));

        // Starting with special chars
        assert!(!is_valid_snapshot_name("-snapshot"));
        assert!(!is_valid_snapshot_name(".hidden"));
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
