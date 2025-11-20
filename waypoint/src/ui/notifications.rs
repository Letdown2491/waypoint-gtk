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
    fn to_gio_priority(self) -> gio::NotificationPriority {
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
        &format!("Successfully created snapshot '{snapshot_name}'"),
        NotificationPriority::Normal,
    );
}

/// Send a notification about successful snapshot deletion
pub fn notify_snapshot_deleted(app: &Application, snapshot_name: &str) {
    send_notification(
        app,
        "Snapshot Deleted",
        &format!("Successfully deleted snapshot '{snapshot_name}'"),
        NotificationPriority::Normal,
    );
}

/// Send a notification about successful snapshot restoration
pub fn notify_snapshot_restored(app: &Application, snapshot_name: &str) {
    send_notification(
        app,
        "System Restored",
        &format!(
            "Snapshot '{snapshot_name}' restored successfully. Reboot to complete the rollback."
        ),
        NotificationPriority::Urgent,
    );
}

/// Send a notification about retention policy cleanup
#[allow(dead_code)]
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
        &format!(
            "Automated snapshot '{snapshot_name}' created successfully"
        ),
        NotificationPriority::Low,
    );
}

/// Send a notification about backup starting
pub fn notify_backup_started(
    app: &Application,
    destination_label: &str,
    pending_count: usize,
) {
    let message = if pending_count == 1 {
        format!("Starting backup of 1 snapshot to {destination_label}")
    } else {
        format!("Starting backup of {pending_count} snapshots to {destination_label}")
    };
    send_notification(
        app,
        "Backup Started",
        &message,
        NotificationPriority::Low,
    );
}

/// Send a notification about successful backup completion
pub fn notify_backup_completed(
    app: &Application,
    destination_label: &str,
    success_count: usize,
    failed_count: usize,
) {
    if failed_count == 0 {
        let message = if success_count == 1 {
            format!("Backed up 1 snapshot to {destination_label}")
        } else {
            format!(
                "Backed up {success_count} snapshots to {destination_label}"
            )
        };
        send_notification(
            app,
            "Backup Completed",
            &message,
            NotificationPriority::Normal,
        );
    } else if success_count > 0 {
        send_notification(
            app,
            "Backup Partially Completed",
            &format!(
                "{success_count} succeeded, {failed_count} failed backing up to {destination_label}"
            ),
            NotificationPriority::Normal,
        );
    } else {
        send_notification(
            app,
            "Backup Failed",
            &format!(
                "Failed to backup {} snapshot{} to {}",
                failed_count,
                if failed_count == 1 { "" } else { "s" },
                destination_label
            ),
            NotificationPriority::High,
        );
    }
}
