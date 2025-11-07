# Phase 7 - Option A: Snapshot Size Calculation

**Status:** 🚧 In Progress
**Estimated Time:** 2 hours
**Started:** 2025-11-07

---

## Implementation Plan

### 1. Calculate Size After Snapshot Creation
- After D-Bus `create_snapshot()` succeeds, calculate size
- Use existing `btrfs::get_snapshot_size()` function
- Store size in snapshot metadata

### 2. Update Snapshot Metadata
- Modify snapshot metadata to include size
- Save updated metadata to disk
- Handle existing snapshots without sizes (None)

### 3. Display Size in Snapshot Rows
- Add size badge/label to each snapshot row
- Format using `format_bytes()` helper
- Show "Calculating..." for snapshots without size

### 4. Add "Calculate Sizes" Button
- Add button to statistics dialog
- Calculate sizes for all snapshots missing size data
- Show progress/spinner during calculation
- Update UI after calculation

### 5. Enhance Statistics Dialog
- Show largest snapshots
- Show average snapshot size
- More accurate total size calculation

---

## Technical Approach

### Step 1: Get Snapshot Path from D-Bus
The D-Bus helper needs to return the snapshot path so we can calculate its size.

**Check current return value:**
- Currently returns: `(bool, String)` - (success, message)
- Need: snapshot path or name to construct path

### Step 2: Calculate Size After Creation
After successful snapshot creation in `create_snapshot_with_description()`:

```rust
// After snapshot creation succeeds:
let snapshot_path = PathBuf::from(format!("/@snapshots/{}", snapshot_name));
let size_bytes = btrfs::get_snapshot_size(&snapshot_path).ok();

// Create/update metadata
let mut snapshot = Snapshot::new(snapshot_name.clone(), snapshot_path.clone());
snapshot.description = Some(description);
snapshot.size_bytes = size_bytes;
snapshot.subvolumes = subvolumes.iter().map(PathBuf::from).collect();

// Save metadata
manager.borrow().add_snapshot(snapshot)?;
```

### Step 3: Update Snapshot Row Display
Add size display to `snapshot_row.rs`:

```rust
// In create_snapshot_row():
if let Some(size) = snapshot.size_bytes {
    let size_label = Label::new(Some(&format_bytes(size)));
    size_label.add_css_class("dim-label");
    row.add_suffix(&size_label);
}
```

### Step 4: Background Size Calculator
Add function to calculate sizes for existing snapshots:

```rust
async fn calculate_all_sizes(
    window: &adw::ApplicationWindow,
    manager: &Rc<RefCell<SnapshotManager>>,
) {
    dialogs::show_toast(window, "Calculating snapshot sizes...");

    let mut snapshots = manager.borrow().load_snapshots()?;
    let mut updated = false;

    for snapshot in &mut snapshots {
        if snapshot.size_bytes.is_none() {
            if let Ok(size) = btrfs::get_snapshot_size(&snapshot.path) {
                snapshot.size_bytes = Some(size);
                updated = true;
            }
        }
    }

    if updated {
        manager.borrow().save_snapshots(&snapshots)?;
        dialogs::show_toast(window, "Snapshot sizes calculated");
    }
}
```

### Step 5: Add Button to Statistics Dialog
In `statistics_dialog.rs`:

```rust
// Add button at bottom of dialog
let calc_btn = Button::with_label("Calculate Missing Sizes");
calc_btn.connect_clicked(move |_| {
    glib::spawn_future_local(async move {
        calculate_all_sizes(&window, &manager).await;
    });
});
```

---

## Files to Modify

1. ✅ `waypoint/src/ui/mod.rs` - Calculate size after snapshot creation
2. ✅ `waypoint/src/ui/snapshot_row.rs` - Display size in row
3. ✅ `waypoint/src/ui/statistics_dialog.rs` - Add calculate button, show largest snapshots
4. ✅ `waypoint/src/btrfs.rs` - Already has `get_snapshot_size()`, remove `#[allow(dead_code)]`

---

## Testing Plan

1. Create new snapshot - verify size is calculated and stored
2. View snapshot list - verify size is displayed
3. Click "Calculate Sizes" - verify existing snapshots get sizes
4. View statistics - verify total size is accurate
5. Delete snapshot - verify sizes still work

---

## Success Criteria

- ✅ New snapshots automatically get size calculated
- ✅ Size displayed in snapshot list
- ✅ Button to calculate sizes for existing snapshots
- ✅ Statistics show accurate total size
- ✅ Build succeeds with no warnings
- ✅ All tests pass

---

Let's begin implementation!
