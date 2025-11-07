use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Represents an installed package
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Package {
    pub name: String,
    pub version: String,
}

impl Package {
    #[allow(dead_code)]
    pub fn new(name: String, version: String) -> Self {
        Self { name, version }
    }
}

/// Get list of all installed packages using xbps-query
#[allow(dead_code)]
pub fn get_installed_packages() -> Result<Vec<Package>> {
    let output = Command::new("xbps-query")
        .arg("-l")
        .output()
        .context("Failed to execute xbps-query. Is XBPS installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("xbps-query failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut packages = Vec::new();

    for line in stdout.lines() {
        // Format: "ii package-name-1.2.3_1 Description here"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let pkg_full = parts[1];
            // Split "package-name-1.2.3_1" into name and version
            if let Some((name, version)) = split_package_name_version(pkg_full) {
                packages.push(Package::new(name.to_string(), version.to_string()));
            }
        }
    }

    Ok(packages)
}

/// Split a package string like "firefox-120.0_1" into ("firefox", "120.0_1")
#[allow(dead_code)]
fn split_package_name_version(pkg: &str) -> Option<(&str, &str)> {
    // Find the last dash that's followed by a digit (version start)
    let mut split_pos = None;

    for (i, c) in pkg.char_indices() {
        if c == '-' {
            // Check if next character is a digit
            if let Some(next_char) = pkg.chars().nth(i + 1) {
                if next_char.is_ascii_digit() {
                    split_pos = Some(i);
                }
            }
        }
    }

    split_pos.map(|pos| {
        let (name, version_with_dash) = pkg.split_at(pos);
        (name, &version_with_dash[1..]) // Skip the dash
    })
}

/// Package diff result
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PackageDiff {
    pub added: Vec<Package>,
    pub removed: Vec<Package>,
    pub updated: Vec<PackageUpdate>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PackageUpdate {
    pub name: String,
    pub old_version: String,
    pub new_version: String,
}

impl PackageDiff {
    #[allow(dead_code)]
    pub fn total_changes(&self) -> usize {
        self.added.len() + self.removed.len() + self.updated.len()
    }
}

/// Compare two package lists and return the differences
#[allow(dead_code)]
pub fn diff_packages(old_packages: &[Package], new_packages: &[Package]) -> PackageDiff {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut updated = Vec::new();

    // Find added and updated packages
    for new_pkg in new_packages {
        match old_packages.iter().find(|p| p.name == new_pkg.name) {
            Some(old_pkg) => {
                // Package exists in both, check if version changed
                if old_pkg.version != new_pkg.version {
                    updated.push(PackageUpdate {
                        name: new_pkg.name.clone(),
                        old_version: old_pkg.version.clone(),
                        new_version: new_pkg.version.clone(),
                    });
                }
            }
            None => {
                // Package only in new list = added
                added.push(new_pkg.clone());
            }
        }
    }

    // Find removed packages
    for old_pkg in old_packages {
        if !new_packages.iter().any(|p| p.name == old_pkg.name) {
            removed.push(old_pkg.clone());
        }
    }

    // Sort for consistent output
    added.sort_by(|a, b| a.name.cmp(&b.name));
    removed.sort_by(|a, b| a.name.cmp(&b.name));
    updated.sort_by(|a, b| a.name.cmp(&b.name));

    PackageDiff {
        added,
        removed,
        updated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_package_name_version() {
        assert_eq!(
            split_package_name_version("firefox-120.0_1"),
            Some(("firefox", "120.0_1"))
        );
        assert_eq!(
            split_package_name_version("lib64-glibc-2.38_1"),
            Some(("lib64-glibc", "2.38_1"))
        );
        assert_eq!(
            split_package_name_version("rust-1.75.0_1"),
            Some(("rust", "1.75.0_1"))
        );
    }

    #[test]
    fn test_package_diff() {
        let old = vec![
            Package::new("firefox".to_string(), "119.0_1".to_string()),
            Package::new("vim".to_string(), "9.0_1".to_string()),
            Package::new("removed-pkg".to_string(), "1.0_1".to_string()),
        ];

        let new = vec![
            Package::new("firefox".to_string(), "120.0_1".to_string()),
            Package::new("vim".to_string(), "9.0_1".to_string()),
            Package::new("new-pkg".to_string(), "1.0_1".to_string()),
        ];

        let diff = diff_packages(&old, &new);

        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].name, "new-pkg");

        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].name, "removed-pkg");

        assert_eq!(diff.updated.len(), 1);
        assert_eq!(diff.updated[0].name, "firefox");
        assert_eq!(diff.updated[0].old_version, "119.0_1");
        assert_eq!(diff.updated[0].new_version, "120.0_1");
    }
}
