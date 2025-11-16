use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

/// Show a confirmation dialog
pub fn show_confirmation<F>(
    window: &adw::ApplicationWindow,
    title: &str,
    message: &str,
    confirm_label: &str,
    destructive: bool,
    on_confirm: F,
) where
    F: Fn() + 'static,
{
    let dialog = adw::MessageDialog::new(Some(window), Some(title), Some(message));

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("confirm", confirm_label);

    if destructive {
        dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
    } else {
        dialog.set_response_appearance("confirm", adw::ResponseAppearance::Suggested);
    }

    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    dialog.connect_response(None, move |_, response| {
        if response == "confirm" {
            on_confirm();
        }
    });

    dialog.present();
}

/// Show an error dialog
pub fn show_error(window: &adw::ApplicationWindow, title: &str, message: &str) {
    let dialog = adw::MessageDialog::new(Some(window), Some(title), Some(message));
    dialog.add_response("ok", "OK");
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present();
}

/// Show an info dialog
#[allow(dead_code)]
pub fn show_info(window: &adw::ApplicationWindow, title: &str, message: &str) {
    let dialog = adw::MessageDialog::new(Some(window), Some(title), Some(message));
    dialog.add_response("ok", "OK");
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present();
}

/// Show a toast notification
pub fn show_toast(window: &adw::ApplicationWindow, message: &str) {
    show_toast_with_timeout(window, message, 3);
}

/// Show a toast notification with custom timeout
pub fn show_toast_with_timeout(window: &adw::ApplicationWindow, message: &str, timeout_seconds: u32) {
    // Get the ToastOverlay from the window content
    if let Some(content) = window.content() {
        if let Ok(toast_overlay) = content.downcast::<adw::ToastOverlay>() {
            let toast = adw::Toast::new(message);
            toast.set_timeout(timeout_seconds);
            toast_overlay.add_toast(toast);
            return;
        }
    }

    // Fallback: print to stdout
    println!("✓ {}", message);
}

/// Show a detailed error list dialog
pub fn show_error_list(window: &adw::ApplicationWindow, title: &str, errors: &[String]) {
    use gtk::Orientation;

    let dialog = adw::Window::new();
    dialog.set_title(Some(title));
    dialog.set_modal(true);
    dialog.set_transient_for(Some(window));
    dialog.set_default_size(600, 400);

    let content = gtk::Box::new(Orientation::Vertical, 0);

    // Header
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(title, "")));
    content.append(&header);

    // Scrollable error list
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);

    let list_box = gtk::ListBox::new();
    list_box.add_css_class("boxed-list");
    list_box.set_margin_top(12);
    list_box.set_margin_bottom(12);
    list_box.set_margin_start(12);
    list_box.set_margin_end(12);

    for (i, error) in errors.iter().enumerate() {
        let row = adw::ActionRow::new();
        row.set_title(&format!("Error {}", i + 1));
        row.set_subtitle(error);

        let icon = gtk::Image::from_icon_name("dialog-error-symbolic");
        icon.add_css_class("error");
        row.add_prefix(&icon);

        list_box.append(&row);
    }

    scrolled.set_child(Some(&list_box));
    content.append(&scrolled);

    dialog.set_content(Some(&content));
    dialog.present();
}
