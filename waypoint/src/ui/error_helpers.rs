//! User-friendly error messages with recovery suggestions
//!
//! This module transforms technical error messages into helpful, actionable
//! messages that guide users toward solutions.

use libadwaita as adw;
use adw::prelude::*;

/// Error context for providing better user guidance
#[derive(Debug, Clone, Copy)]
pub enum ErrorContext {
    SnapshotCreate,
    SnapshotDelete,
    SnapshotRestore,
    #[allow(dead_code)]
    SnapshotVerify,
    #[allow(dead_code)]
    SnapshotList,
    DiskSpace,
    FilesystemCheck,
    #[allow(dead_code)]
    DBusConnection,
    #[allow(dead_code)]
    Authorization,
    #[allow(dead_code)]
    Configuration,
}

/// Show an improved error dialog with context and recovery suggestions
pub fn show_error_with_context(
    window: &adw::ApplicationWindow,
    context: ErrorContext,
    error: &str,
) {
    let (title, message, details) = format_error_message(context, error);

    let dialog = adw::MessageDialog::new(Some(window), Some(&title), Some(&message));

    // Add details as body if available
    if let Some(detail_text) = details {
        dialog.set_body(&detail_text);
    }

    dialog.add_response("ok", "OK");
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present();
}

/// Format error message with helpful context and recovery suggestions
fn format_error_message(context: ErrorContext, error: &str) -> (String, String, Option<String>) {
    match context {
        ErrorContext::SnapshotCreate => format_snapshot_create_error(error),
        ErrorContext::SnapshotDelete => format_snapshot_delete_error(error),
        ErrorContext::SnapshotRestore => format_snapshot_restore_error(error),
        ErrorContext::SnapshotVerify => format_snapshot_verify_error(error),
        ErrorContext::SnapshotList => format_snapshot_list_error(error),
        ErrorContext::DiskSpace => format_disk_space_error(error),
        ErrorContext::FilesystemCheck => format_filesystem_error(error),
        ErrorContext::DBusConnection => format_dbus_error(error),
        ErrorContext::Authorization => format_authorization_error(error),
        ErrorContext::Configuration => format_configuration_error(error),
    }
}

fn format_snapshot_create_error(error: &str) -> (String, String, Option<String>) {
    let title = "Failed to Create Snapshot".to_string();

    let (message, recovery) = if error.contains("not enough space") || error.contains("No space left") {
        (
            "Not enough disk space to create snapshot.".to_string(),
            Some("Try deleting old snapshots or freeing up disk space before creating a new snapshot.".to_string())
        )
    } else if error.contains("Authorization failed") || error.contains("not authorized") {
        (
            "Permission denied.".to_string(),
            Some("You need administrator privileges to create snapshots. Make sure you enter the correct password when prompted.".to_string())
        )
    } else if error.contains("not a btrfs") || error.contains("wrong fs type") {
        (
            "Your root filesystem is not Btrfs.".to_string(),
            Some("Waypoint requires a Btrfs filesystem to create snapshots. This system appears to be using a different filesystem type.".to_string())
        )
    } else if error.contains("already exists") {
        (
            "A snapshot with this name already exists.".to_string(),
            Some("Choose a different name for your snapshot.".to_string())
        )
    } else {
        (
            "An error occurred while creating the snapshot.".to_string(),
            Some(format!("Technical details: {}", error))
        )
    };

    (title, message, recovery)
}

fn format_snapshot_delete_error(error: &str) -> (String, String, Option<String>) {
    let title = "Failed to Delete Snapshot".to_string();

    let (message, recovery) = if error.contains("Authorization failed") {
        (
            "Permission denied.".to_string(),
            Some("You need administrator privileges to delete snapshots.".to_string())
        )
    } else if error.contains("not found") || error.contains("does not exist") {
        (
            "Snapshot not found.".to_string(),
            Some("The snapshot may have already been deleted. Try refreshing the list.".to_string())
        )
    } else if error.contains("busy") || error.contains("in use") {
        (
            "Snapshot is currently in use.".to_string(),
            Some("Close any programs that might be accessing the snapshot and try again.".to_string())
        )
    } else {
        (
            "An error occurred while deleting the snapshot.".to_string(),
            Some(format!("Technical details: {}", error))
        )
    };

    (title, message, recovery)
}

fn format_snapshot_restore_error(error: &str) -> (String, String, Option<String>) {
    let title = "Failed to Restore Snapshot".to_string();

    let (message, recovery) = if error.contains("Authorization failed") {
        (
            "Permission denied.".to_string(),
            Some("You need administrator privileges to restore snapshots.".to_string())
        )
    } else if error.contains("not found") {
        (
            "Snapshot not found.".to_string(),
            Some("The snapshot may have been deleted. Check the snapshot list.".to_string())
        )
    } else if error.contains("fstab") {
        (
            "Failed to update boot configuration.".to_string(),
            Some("The system configuration file (/etc/fstab) could not be updated. Your system may require manual configuration.".to_string())
        )
    } else {
        (
            "An error occurred during snapshot restore.".to_string(),
            Some(format!("Technical details: {}\n\nNote: You must reboot for restore changes to take effect.", error))
        )
    };

    (title, message, recovery)
}

fn format_snapshot_verify_error(error: &str) -> (String, String, Option<String>) {
    let title = "Verification Failed".to_string();

    let (message, recovery) = if error.contains("not found") {
        (
            "Snapshot not found on disk.".to_string(),
            Some("The snapshot directory may have been manually deleted. You can safely remove this entry from the list.".to_string())
        )
    } else if error.contains("corrupt") {
        (
            "Snapshot appears to be corrupted.".to_string(),
            Some("This snapshot should not be used for restore. Consider deleting it and creating a new one.".to_string())
        )
    } else {
        (
            "Unable to verify snapshot integrity.".to_string(),
            Some(format!("Technical details: {}", error))
        )
    };

    (title, message, recovery)
}

fn format_snapshot_list_error(error: &str) -> (String, String, Option<String>) {
    (
        "Failed to Load Snapshots".to_string(),
        "Unable to retrieve the snapshot list.".to_string(),
        Some(format!("This could be a temporary issue. Try refreshing the list.\n\nTechnical details: {}", error))
    )
}

fn format_disk_space_error(error: &str) -> (String, String, Option<String>) {
    (
        "Insufficient Disk Space".to_string(),
        "Not enough free space to create a snapshot.".to_string(),
        Some(format!("Delete old snapshots or free up disk space before proceeding.\n\nTechnical details: {}", error))
    )
}

fn format_filesystem_error(error: &str) -> (String, String, Option<String>) {
    let (message, recovery) = if error.contains("not a btrfs") || error.contains("btrfs") {
        (
            "This system is not using Btrfs.".to_string(),
            Some("Waypoint requires a Btrfs filesystem to function. Your root filesystem appears to be using a different type.".to_string())
        )
    } else {
        (
            "Filesystem check failed.".to_string(),
            Some(format!("Unable to verify filesystem type.\n\nTechnical details: {}", error))
        )
    };

    (
        "Filesystem Error".to_string(),
        message,
        recovery
    )
}

fn format_dbus_error(error: &str) -> (String, String, Option<String>) {
    (
        "Service Connection Error".to_string(),
        "Unable to connect to the Waypoint system service.".to_string(),
        Some(format!("The waypoint-helper service may not be running. Try restarting it or your system.\n\nTechnical details: {}", error))
    )
}

fn format_authorization_error(error: &str) -> (String, String, Option<String>) {
    (
        "Authorization Required".to_string(),
        "This operation requires administrator privileges.".to_string(),
        Some(format!("Enter your password when prompted to authorize this action.\n\nTechnical details: {}", error))
    )
}

fn format_configuration_error(error: &str) -> (String, String, Option<String>) {
    let (message, recovery) = if error.contains("parse") || error.contains("JSON") {
        (
            "Configuration file is invalid.".to_string(),
            Some("The configuration file contains invalid data. It may need to be reset to defaults.".to_string())
        )
    } else if error.contains("permission") || error.contains("denied") {
        (
            "Cannot save configuration.".to_string(),
            Some("Permission denied when writing configuration file. Check file permissions.".to_string())
        )
    } else {
        (
            "Configuration error occurred.".to_string(),
            Some(format!("Technical details: {}", error))
        )
    };

    (
        "Configuration Error".to_string(),
        message,
        recovery
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_space_error_formatting() {
        let (title, message, details) = format_error_message(
            ErrorContext::SnapshotCreate,
            "not enough space on device"
        );

        assert_eq!(title, "Failed to Create Snapshot");
        assert!(message.contains("Not enough disk space"));
        assert!(details.is_some());
        assert!(details.unwrap().contains("Try deleting old snapshots"));
    }

    #[test]
    fn test_authorization_error_formatting() {
        let (title, message, details) = format_error_message(
            ErrorContext::SnapshotCreate,
            "Authorization failed: not authorized"
        );

        assert_eq!(title, "Failed to Create Snapshot");
        assert!(message.contains("Permission denied"));
        assert!(details.unwrap().contains("administrator privileges"));
    }

    #[test]
    fn test_btrfs_error_formatting() {
        let (title, message, details) = format_error_message(
            ErrorContext::FilesystemCheck,
            "not a btrfs filesystem"
        );

        assert_eq!(title, "Filesystem Error");
        assert!(message.contains("not using Btrfs"));
        assert!(details.unwrap().contains("requires a Btrfs filesystem"));
    }

    #[test]
    fn test_snapshot_exists_error() {
        let (title, message, details) = format_error_message(
            ErrorContext::SnapshotCreate,
            "snapshot already exists"
        );

        assert_eq!(title, "Failed to Create Snapshot");
        assert!(message.contains("already exists"));
        assert!(details.unwrap().contains("Choose a different name"));
    }

    #[test]
    fn test_generic_error_includes_details() {
        let (_, _, details) = format_error_message(
            ErrorContext::SnapshotList,
            "some unknown error occurred"
        );

        assert!(details.is_some());
        assert!(details.unwrap().contains("some unknown error occurred"));
    }
}
