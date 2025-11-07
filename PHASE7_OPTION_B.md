# Phase 7 - Option B: Search & Filter Snapshots

**Status:** 🚧 In Progress
**Estimated Time:** 3 hours
**Started:** 2025-11-07

---

## Implementation Plan

### 1. Add Search Entry to UI
- Add `gtk::SearchEntry` above snapshot list
- Proper spacing and margins
- Clear button to reset search
- Real-time filtering as user types

### 2. Implement Search Filter Logic
- Filter by snapshot name
- Filter by description
- Case-insensitive search
- Show only matching snapshots

### 3. Add Date Range Filters
- Quick filters: "All", "Last 7 days", "Last 30 days", "Last 90 days"
- Use button group for filter selection
- Combine with text search

### 4. Show Match Count
- Display "Showing X of Y snapshots"
- Update count as filters change
- Show in status area below filters

### 5. Polish & Testing
- Ensure smooth performance with many snapshots
- Test all filter combinations
- Verify UI responsiveness

---

## UI Design

### Layout:
```
┌────────────────────────────────────────────┐
│ 🔍 Search snapshots...              [×]    │
├────────────────────────────────────────────┤
│ [All] [Last 7 days] [Last 30 days] [90d]  │
├────────────────────────────────────────────┤
│ Showing 3 of 15 snapshots                  │
├────────────────────────────────────────────┤
│ 📦 waypoint-20251107-143000                │
│    Before Docker installation              │
│    2025-11-07 14:30:00  •  2.45 GiB        │
├────────────────────────────────────────────┤
│ 📦 waypoint-20251105-120000                │
│    Pre-kernel upgrade                      │
│    2025-11-05 12:00:00  •  2.89 GiB        │
└────────────────────────────────────────────┘
```

---

## Technical Approach

### Data Structure
Add to `MainWindow`:
```rust
pub struct MainWindow {
    window: adw::ApplicationWindow,
    snapshot_manager: Rc<RefCell<SnapshotManager>>,
    snapshot_list: ListBox,
    compare_btn: Button,
    search_entry: SearchEntry,       // NEW
    date_filter: Rc<RefCell<DateFilter>>, // NEW
}

#[derive(Clone, Copy, PartialEq)]
enum DateFilter {
    All,
    Last7Days,
    Last30Days,
    Last90Days,
}
```

### Filtering Function
```rust
fn filter_snapshots(
    snapshots: &[Snapshot],
    search_text: &str,
    date_filter: DateFilter,
) -> Vec<&Snapshot> {
    let search_lower = search_text.to_lowercase();
    let now = Utc::now();

    snapshots.iter().filter(|snapshot| {
        // Text filter
        let text_match = search_text.is_empty() ||
            snapshot.name.to_lowercase().contains(&search_lower) ||
            snapshot.description.as_ref()
                .map(|d| d.to_lowercase().contains(&search_lower))
                .unwrap_or(false);

        // Date filter
        let date_match = match date_filter {
            DateFilter::All => true,
            DateFilter::Last7Days => {
                let age = now.signed_duration_since(snapshot.timestamp).num_days();
                age <= 7
            },
            DateFilter::Last30Days => {
                let age = now.signed_duration_since(snapshot.timestamp).num_days();
                age <= 30
            },
            DateFilter::Last90Days => {
                let age = now.signed_duration_since(snapshot.timestamp).num_days();
                age <= 90
            },
        };

        text_match && date_match
    }).collect()
}
```

### Search Entry Handler
```rust
search_entry.connect_search_changed(move |entry| {
    let search_text = entry.text().to_string();
    Self::refresh_with_filter(&window, &manager, &list, &search_text, date_filter);
});
```

### Date Filter Buttons
```rust
let filter_box = gtk::Box::new(Orientation::Horizontal, 6);
filter_box.add_css_class("linked");

let all_btn = ToggleButton::with_label("All");
let week_btn = ToggleButton::with_label("Last 7 days");
let month_btn = ToggleButton::with_label("Last 30 days");
let quarter_btn = ToggleButton::with_label("Last 90 days");

// Connect buttons to update filter
all_btn.connect_toggled(move |btn| {
    if btn.is_active() {
        date_filter.replace(DateFilter::All);
        refresh_with_filter(...);
    }
});
```

---

## Files to Modify

1. ✅ `waypoint/src/ui/mod.rs` - Add search UI, filtering logic
2. ✅ Tests - May need to add filter tests

---

## Success Criteria

- ✅ Search entry filters snapshots in real-time
- ✅ Date range filters work correctly
- ✅ Filters can be combined (text + date)
- ✅ Match count updates correctly
- ✅ Clear button resets all filters
- ✅ Performance is good with 50+ snapshots
- ✅ Build succeeds with no warnings
- ✅ All tests pass

---

Let's begin implementation!
