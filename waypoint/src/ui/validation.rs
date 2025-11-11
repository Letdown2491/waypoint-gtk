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
