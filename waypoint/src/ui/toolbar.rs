//! Toolbar creation for the main window
//!
//! This module provides the toolbar UI component with all action buttons.

use gtk::prelude::*;
use gtk::{Button, Label, Orientation};

/// Create the main toolbar with action buttons
///
/// Creates a horizontal toolbar containing primary action buttons:
/// - Create Restore Point (suggested action, pill-styled)
/// - Compare Snapshots
///
/// # Returns
/// A tuple containing:
/// - `gtk::Box` - The toolbar container
/// - `Button` - Create restore point button
/// - `Button` - Compare snapshots button
///
/// # Example
/// ```no_run
/// let (toolbar, create_btn, compare_btn) = toolbar::create_toolbar();
/// // Connect button handlers...
/// container.append(&toolbar);
/// ```
pub fn create_toolbar() -> (gtk::Box, Button, Button) {
    // Use Clamp for toolbar as well (GNOME HIG)
    let toolbar = gtk::Box::new(Orientation::Horizontal, 12);
    toolbar.set_margin_top(18);
    toolbar.set_margin_bottom(12);
    toolbar.set_margin_start(12);
    toolbar.set_margin_end(12);

    // Create button with icon
    let create_btn_content = gtk::Box::new(Orientation::Horizontal, 6);
    let create_icon = gtk::Image::from_icon_name("document-save-symbolic");
    let create_label = Label::new(Some("Create Restore Point"));
    create_btn_content.append(&create_icon);
    create_btn_content.append(&create_label);

    let create_btn = Button::new();
    create_btn.set_child(Some(&create_btn_content));
    create_btn.add_css_class("suggested-action");
    create_btn.add_css_class("pill");

    toolbar.append(&create_btn);

    // Spacer
    let spacer = gtk::Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    toolbar.append(&spacer);

    // Compare button
    let compare_btn = Button::builder()
        .label("Compare Snapshots")
        .build();
    compare_btn.add_css_class("flat");

    toolbar.append(&compare_btn);

    (toolbar, create_btn, compare_btn)
}
