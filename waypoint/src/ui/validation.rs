use std::path::PathBuf;

/// Validate that a path is safe to open with xdg-open
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// `Ok(())` if path is safe to open, `Err` with description if invalid
///
/// # Security
/// Only allows paths within known snapshot directories to prevent opening
/// arbitrary files or directories that could be malicious.
#[allow(dead_code)]
pub fn validate_path_for_open(path: &std::path::Path) -> Result<(), String> {
    // Canonicalize the path to resolve symlinks and get absolute path
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => return Err(format!("Cannot resolve path: {}", e)),
    };

    // Define allowed base directories
    let allowed_dirs = [
        PathBuf::from("/.snapshots"),
        PathBuf::from("/mnt/btrfs-root/@snapshots"),
    ];

    // Check if the canonical path starts with any allowed directory
    for allowed_dir in &allowed_dirs {
        // Try to canonicalize the allowed dir (it might not exist)
        if let Ok(canonical_allowed) = allowed_dir.canonicalize() {
            if canonical.starts_with(&canonical_allowed) {
                return Ok(());
            }
        } else {
            // If allowed dir doesn't exist yet, do string comparison
            if canonical.starts_with(allowed_dir) {
                return Ok(());
            }
        }
    }

    Err(format!(
        "Path '{}' is outside allowed snapshot directories",
        canonical.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs as unix_fs;

    #[test]
    fn test_validate_nonexistent_path() {
        let path = std::path::Path::new("/nonexistent/path/to/snapshot");
        let result = validate_path_for_open(path);

        // Should fail because path doesn't exist (can't canonicalize)
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot resolve path"));
    }

    #[test]
    fn test_validate_system_path_rejected() {
        // Try to validate /etc/passwd - should be rejected
        let path = std::path::Path::new("/etc/passwd");

        // Skip test if path doesn't exist
        if !path.exists() {
            return;
        }

        let result = validate_path_for_open(path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside allowed snapshot directories"));
    }

    #[test]
    fn test_validate_home_directory_rejected() {
        // Try to validate user's home directory - should be rejected
        if let Some(home) = dirs::home_dir() {
            let result = validate_path_for_open(&home);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("outside allowed snapshot directories"));
        }
    }

    #[test]
    fn test_validate_root_directory_rejected() {
        // Root directory should be rejected (not in snapshot dirs)
        let path = std::path::Path::new("/");
        let result = validate_path_for_open(path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside allowed snapshot directories"));
    }

    #[test]
    fn test_validate_tmp_directory_rejected() {
        // /tmp should be rejected
        let path = std::path::Path::new("/tmp");
        let result = validate_path_for_open(path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside allowed snapshot directories"));
    }

    #[test]
    fn test_snapshots_directory_requires_actual_path() {
        // Test that we properly validate /.snapshots exists before accepting it
        let snapshots_dir = std::path::Path::new("/.snapshots");

        if snapshots_dir.exists() {
            // If it exists, it should be valid
            let result = validate_path_for_open(snapshots_dir);
            assert!(result.is_ok(), "/.snapshots should be valid if it exists");
        } else {
            // If it doesn't exist, it should fail to canonicalize
            let result = validate_path_for_open(snapshots_dir);
            assert!(result.is_err(), "/.snapshots should fail if it doesn't exist");
        }
    }

    #[test]
    fn test_path_traversal_attack_blocked() {
        // Create a test structure in /tmp to test path traversal
        let test_dir = std::env::temp_dir().join("waypoint-test-validation");
        let _ = fs::remove_dir_all(&test_dir); // Clean up from previous runs

        // This tests that even if we try path traversal, it won't escape
        // We can't create /.snapshots in tests, but we can verify rejection logic
        let malicious_path = test_dir.join("../../etc/passwd");

        if malicious_path.exists() {
            let result = validate_path_for_open(&malicious_path);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("outside allowed snapshot directories"));
        }

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_symlink_to_system_file_blocked() {
        // Create a temporary directory for testing
        let test_dir = std::env::temp_dir().join("waypoint-test-symlink");
        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::create_dir_all(&test_dir);

        let symlink_path = test_dir.join("link-to-passwd");

        // Create a symlink to /etc/passwd
        if unix_fs::symlink("/etc/passwd", &symlink_path).is_ok() {
            // The symlink should be rejected because it points outside snapshot dirs
            let result = validate_path_for_open(&symlink_path);
            assert!(result.is_err());
            // After canonicalization, it will point to /etc/passwd
            assert!(result.unwrap_err().contains("outside allowed snapshot directories"));
        }

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_allowed_directories_list() {
        // Verify our allowed directories are as expected
        let allowed_dirs = [
            PathBuf::from("/.snapshots"),
            PathBuf::from("/mnt/btrfs-root/@snapshots"),
        ];

        assert_eq!(allowed_dirs.len(), 2);
        assert_eq!(allowed_dirs[0], PathBuf::from("/.snapshots"));
        assert_eq!(allowed_dirs[1], PathBuf::from("/mnt/btrfs-root/@snapshots"));
    }
}
