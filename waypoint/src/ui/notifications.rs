use gio::prelude::*;
use gtk::Application;

/// Priority levels for notifications
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum NotificationPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl NotificationPriority {
    fn to_gio_priority(&self) -> gio::NotificationPriority {
        match self {
            NotificationPriority::Low => gio::NotificationPriority::Low,
            NotificationPriority::Normal => gio::NotificationPriority::Normal,
            NotificationPriority::High => gio::NotificationPriority::High,
            NotificationPriority::Urgent => gio::NotificationPriority::Urgent,
        }
    }
}

/// Send a desktop notification
///
/// # Arguments
/// * `app` - The GTK application instance
/// * `title` - Notification title
/// * `body` - Notification body text
/// * `priority` - Notification priority level
pub fn send_notification(
    app: &Application,
    title: &str,
    body: &str,
    priority: NotificationPriority,
) {
    let notification = gio::Notification::new(title);
    notification.set_body(Some(body));
    notification.set_priority(priority.to_gio_priority());

    // Use application icon
    let icon = gio::ThemedIcon::new("waypoint");
    notification.set_icon(&icon);

    app.send_notification(None, &notification);
}

/// Send a notification about successful snapshot creation
pub fn notify_snapshot_created(app: &Application, snapshot_name: &str) {
    send_notification(
        app,
        "Snapshot Created",
        &format!("Successfully created snapshot '{}'", snapshot_name),
        NotificationPriority::Normal,
    );
}

/// Send a notification about successful snapshot deletion
pub fn notify_snapshot_deleted(app: &Application, snapshot_name: &str) {
    send_notification(
        app,
        "Snapshot Deleted",
        &format!("Successfully deleted snapshot '{}'", snapshot_name),
        NotificationPriority::Normal,
    );
}

/// Send a notification about successful snapshot restoration
pub fn notify_snapshot_restored(app: &Application, snapshot_name: &str) {
    send_notification(
        app,
        "System Restored",
        &format!(
            "Snapshot '{}' restored successfully. Reboot to complete the rollback.",
            snapshot_name
        ),
        NotificationPriority::Urgent,
    );
}

/// Send a notification about retention policy cleanup
pub fn notify_retention_cleanup(app: &Application, count: usize) {
    send_notification(
        app,
        "Snapshots Cleaned Up",
        &format!(
            "Retention policy deleted {} old snapshot{}",
            count,
            if count == 1 { "" } else { "s" }
        ),
        NotificationPriority::Low,
    );
}

/// Send a notification about scheduled snapshot creation
pub fn notify_scheduled_snapshot(app: &Application, snapshot_name: &str) {
    send_notification(
        app,
        "Scheduled Snapshot Created",
        &format!("Automated snapshot '{}' created successfully", snapshot_name),
        NotificationPriority::Low,
    );
}
