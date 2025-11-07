// Package management for waypoint-helper

use anyhow::{Context, Result};
use std::process::Command;
use waypoint_common::Package;

/// Get list of all installed packages using xbps-query
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
                packages.push(Package {
                    name: name.to_string(),
                    version: version.to_string(),
                });
            }
        }
    }

    Ok(packages)
}

/// Split a package string like "firefox-120.0_1" into ("firefox", "120.0_1")
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
