#!/bin/bash
# Clean up phantom snapshots from metadata

METADATA_FILE="/var/lib/waypoint/snapshots.json"

if [ ! -f "$METADATA_FILE" ]; then
    echo "No metadata file found at $METADATA_FILE"
    exit 0
fi

echo "Checking for phantom snapshots..."

# Create backup
BACKUP="${METADATA_FILE}.backup-$(date +%Y%m%d-%H%M%S)"
cp "$METADATA_FILE" "$BACKUP"
echo "Created backup: $BACKUP"

# Build a valid snapshots array
echo "[" > "${METADATA_FILE}.tmp"
FIRST=true
REMOVED=0

# Read each snapshot and check if path exists
jq -c '.[]' "$METADATA_FILE" | while read -r snapshot; do
    SNAPSHOT_PATH=$(echo "$snapshot" | jq -r '.path')

    if [ -d "$SNAPSHOT_PATH" ]; then
        # Snapshot exists - keep it
        if [ "$FIRST" = true ]; then
            FIRST=false
        else
            echo "," >> "${METADATA_FILE}.tmp"
        fi
        echo "$snapshot" >> "${METADATA_FILE}.tmp"
    else
        # Phantom snapshot - remove it
        SNAPSHOT_NAME=$(echo "$snapshot" | jq -r '.name')
        echo "  ✗ Removing phantom: $SNAPSHOT_NAME (path not found: $SNAPSHOT_PATH)"
        REMOVED=$((REMOVED + 1))
    fi
done

echo "]" >> "${METADATA_FILE}.tmp"

# Count snapshots before and after
BEFORE=$(jq '. | length' "$METADATA_FILE" 2>/dev/null || echo 0)
AFTER=$(jq '. | length' "${METADATA_FILE}.tmp" 2>/dev/null || echo 0)

if [ "$AFTER" -lt "$BEFORE" ]; then
    # Pretty print the JSON and save
    jq '.' "${METADATA_FILE}.tmp" > "$METADATA_FILE"
    rm "${METADATA_FILE}.tmp"
    echo ""
    echo "✓ Removed $((BEFORE - AFTER)) phantom snapshot(s)"
    echo "✓ Cleaned metadata saved to $METADATA_FILE"
else
    rm "${METADATA_FILE}.tmp"
    echo ""
    echo "✓ No phantom snapshots found - metadata is clean"
fi
