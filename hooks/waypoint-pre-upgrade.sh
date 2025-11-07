#!/bin/bash
# Waypoint pre-upgrade hook for XBPS
# Automatically creates a snapshot before system upgrades

# This hook is triggered by XBPS before package upgrades
# It creates a Waypoint snapshot so you can roll back if something breaks

case "${XBPS_TARGET_PHASE}" in
    pre)
        # Only run before the upgrade transaction

        # Check if waypoint is available
        if ! command -v waypoint >/dev/null 2>&1; then
            echo "waypoint: Command not found, skipping automatic snapshot"
            exit 0
        fi

        # Check if we're on Btrfs
        if ! stat -f -c %T / 2>/dev/null | grep -q "btrfs"; then
            # Not on Btrfs, skip silently
            exit 0
        fi

        # Check if running as root
        if [ "$(id -u)" -ne 0 ]; then
            echo "waypoint: Not running as root, skipping automatic snapshot"
            exit 0
        fi

        echo "==============================================="
        echo "Waypoint: Creating pre-upgrade snapshot..."
        echo "==============================================="

        # Create snapshot with timestamp
        SNAPSHOT_NAME="@snapshots/waypoint-pre-upgrade-$(date +%Y%m%d-%H%M%S)"

        if btrfs subvolume snapshot -r / "/${SNAPSHOT_NAME}" >/dev/null 2>&1; then
            echo "✓ Snapshot created: ${SNAPSHOT_NAME}"
            echo "  You can roll back if the upgrade causes issues."
        else
            echo "⚠ Failed to create snapshot (non-fatal, continuing upgrade)"
        fi

        echo "==============================================="
        ;;
esac

# Always exit 0 so we don't block upgrades even if snapshot fails
exit 0
