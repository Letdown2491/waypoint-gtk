# Phase 7: Advanced Features - Proposal

**Current Status:** Phase 6 Complete ✅
**Waypoint Version:** 0.4.0
**Date:** 2025-11-07

---

## Phase 7 Options

Waypoint is now production-ready with core functionality complete. Phase 7 focuses on advanced features that enhance usability and power-user workflows.

---

## 🎯 HIGH PRIORITY OPTIONS

### Option A: **Snapshot Size Calculation & Display**
**Effort:** ~2 hours | **Impact:** High | **Priority:** 🔴 HIGH

**Problem:**
- Snapshot sizes currently show as "None" in metadata
- Statistics dialog shows total size as 0
- Users can't see which snapshots consume the most space

**Solution:**
- Calculate actual disk usage when creating snapshots
- Add background size calculation for existing snapshots
- Display size in snapshot list rows
- Sort by size in statistics

**Implementation:**
1. Call `btrfs::get_snapshot_size()` after snapshot creation
2. Update snapshot metadata with size
3. Add "Size" column/badge to snapshot rows
4. Add "Calculate Sizes" button to statistics dialog for existing snapshots
5. Show largest snapshots in statistics

**Benefits:**
- Users can identify space-hogging snapshots
- Better informed decisions about which to delete
- More accurate disk usage statistics

---

### Option B: **Search & Filter Snapshots**
**Effort:** ~3 hours | **Impact:** High | **Priority:** 🟠 HIGH

**Problem:**
- With many snapshots, finding specific ones is hard
- No way to filter by description, date, or tags
- List becomes cluttered over time

**Solution:**
- Add search bar above snapshot list
- Filter by description, date range, or subvolumes
- Highlight matching text
- Show match count

**Implementation:**
1. Add `gtk::SearchEntry` to UI
2. Filter snapshots in `refresh_snapshot_list()`
3. Add date range picker (last 7 days, 30 days, custom)
4. Optional: Add tag filtering (if tags are implemented)

**UI Mockup:**
```
┌────────────────────────────────────┐
│ 🔍 Search snapshots...        [×]  │
├────────────────────────────────────┤
│ Filters: [All] [Last 7d] [Last 30d]│
├────────────────────────────────────┤
│ 📦 Before Docker installation      │
│ 📦 Pre-kernel upgrade              │
│ (Showing 2 of 15 snapshots)        │
└────────────────────────────────────┘
```

**Benefits:**
- Find snapshots quickly
- Focus on relevant snapshots
- Better UX with many snapshots

---

### Option C: **Retention Policy GUI Editor**
**Effort:** ~3 hours | **Impact:** Medium-High | **Priority:** 🟡 MEDIUM

**Problem:**
- Users must manually edit JSON config
- No validation of settings
- Unclear what values are valid

**Solution:**
- Add "Edit Policy" button to statistics dialog
- Visual editor with spinners and toggles
- Live preview of which snapshots would be deleted
- Validation and helpful hints

**Implementation:**
1. Create `retention_editor_dialog.rs`
2. Use `adw::PreferencesPage` with spinners for numbers
3. Add "Preview" section showing snapshots to be deleted
4. Save to `~/.config/waypoint/retention.json`
5. Add validation (e.g., min_snapshots ≤ max_snapshots)

**UI Mockup:**
```
┌──────────────────────────────────────┐
│ Retention Policy Editor        [×]   │
├──────────────────────────────────────┤
│ Max Snapshots:  [10      ] ⬆⬇        │
│ Max Age (days): [30      ] ⬆⬇        │
│ Min Snapshots:  [3       ] ⬆⬇        │
│                                      │
│ Keep Patterns:                       │
│ ┌──────────────────────────────────┐ │
│ │ pre-upgrade                [×]   │ │
│ │ stable                     [×]   │ │
│ └──────────────────────────────────┘ │
│ [+ Add Pattern]                      │
│                                      │
│ Preview: 2 snapshots will be deleted │
│                                      │
│           [Cancel]  [Save]           │
└──────────────────────────────────────┘
```

**Benefits:**
- User-friendly configuration
- Prevents invalid settings
- Visual feedback before applying
- No manual JSON editing

---

## 🎨 MEDIUM PRIORITY OPTIONS

### Option D: **Scheduled/Automatic Snapshots**
**Effort:** ~4 hours | **Impact:** High | **Priority:** 🟠 HIGH

**Problem:**
- Users must manually create snapshots
- Easy to forget to snapshot before changes
- No automatic daily/weekly snapshots

**Solution:**
- Add systemd timer for automatic snapshots
- Configure schedule in preferences
- Optional: Pre/post hooks for package upgrades (already have XBPS hook)

**Implementation:**
1. Create systemd user timer unit
2. Add schedule configuration to preferences
3. Options: Daily, Weekly, Before shutdown, Custom cron
4. Install timer via D-Bus helper (needs root)
5. Show last auto-snapshot time in UI

**Configuration:**
```json
{
  "schedule": {
    "enabled": true,
    "frequency": "daily",  // "daily", "weekly", "monthly"
    "time": "02:00",
    "description_prefix": "Auto"
  }
}
```

**Benefits:**
- Automatic protection without user action
- Regular backup cadence
- Peace of mind
- Works with retention policy

---

### Option E: **Snapshot Tagging System**
**Effort:** ~3 hours | **Impact:** Medium | **Priority:** 🟡 MEDIUM

**Problem:**
- Only description field for context
- Can't categorize snapshots
- Hard to find snapshots by purpose

**Solution:**
- Add tags to snapshots (e.g., "stable", "experimental", "pre-upgrade")
- Filter by tags
- Color-coded tag badges
- Quick tag templates

**Implementation:**
1. Add `tags: Vec<String>` to `Snapshot` struct
2. Create tag selector dialog
3. Display tags as badges in snapshot rows
4. Add tag filter to search bar
5. Predefined tag templates: "Stable", "Experimental", "Pre-Upgrade", "Backup"

**UI Display:**
```
┌────────────────────────────────────────────┐
│ 📦 Before Docker installation              │
│    2025-11-07 14:30 | 2.3 GiB              │
│    [Stable] [Backup]                       │
├────────────────────────────────────────────┤
│ 📦 Experimental kernel 6.12                │
│    2025-11-06 09:15 | 2.1 GiB              │
│    [Experimental] [Pre-Upgrade]            │
└────────────────────────────────────────────┘
```

**Benefits:**
- Better organization
- Visual categorization
- Enhanced filtering
- Clearer snapshot purposes

---

### Option F: **Snapshot Comparison Enhancements**
**Effort:** ~3 hours | **Impact:** Medium | **Priority:** 🟡 MEDIUM

**Problem:**
- Package comparison is good but could be better
- No file-level diff
- Can't see configuration changes

**Solution:**
- Add file diff view (show changed files in /etc)
- Compare disk usage between snapshots
- Show kernel version differences
- Export comparison as report

**Implementation:**
1. Extend comparison dialog with tabs:
   - **Packages** (existing)
   - **Files** (new) - show changed files in /etc
   - **Summary** (new) - sizes, kernel, date
2. Use `diff` or `git diff` for file comparison
3. Add "Export Report" button (save as text/HTML)

**Benefits:**
- More detailed snapshot comparison
- Understand what changed
- Better rollback decisions
- Documentation of changes

---

## 🔧 LOWER PRIORITY OPTIONS

### Option G: **Configuration Import/Export**
**Effort:** ~2 hours | **Impact:** Low-Medium | **Priority:** 🟢 LOW

**Features:**
- Export all settings (retention policy, subvolumes, schedule)
- Import settings from another machine
- Share configurations easily

### Option H: **Desktop Notifications**
**Effort:** ~2 hours | **Impact:** Low-Medium | **Priority:** 🟢 LOW

**Features:**
- Notify when snapshot created
- Notify when cleanup happens
- Notify on errors
- Optional: Reminder to create snapshot

### Option I: **CLI Interface**
**Effort:** ~4 hours | **Impact:** Medium | **Priority:** 🟡 MEDIUM

**Features:**
- `waypoint create --description "Before upgrade"`
- `waypoint list`
- `waypoint restore <snapshot-id>`
- `waypoint cleanup`
- Useful for scripts and automation

### Option J: **GRUB Integration**
**Effort:** ~6 hours | **Impact:** High | **Priority:** 🔴 HIGH (but complex)

**Features:**
- Boot into snapshots from GRUB menu
- Add Btrfs snapshot boot entries
- Similar to Snapper's grub integration
- **Note:** This is complex and requires careful testing

---

## 📊 Recommendation Matrix

| Option | Effort | Impact | User Value | Technical Debt | Score |
|--------|--------|--------|------------|----------------|-------|
| **A. Snapshot Sizes** | 2h | High | ⭐⭐⭐⭐⭐ | Low | 🏆 **Best Quick Win** |
| **B. Search/Filter** | 3h | High | ⭐⭐⭐⭐⭐ | Low | 🏆 **High Value** |
| **C. Policy Editor** | 3h | Med-High | ⭐⭐⭐⭐ | Low | ✅ **Good Choice** |
| **D. Auto Snapshots** | 4h | High | ⭐⭐⭐⭐⭐ | Medium | ✅ **Power Feature** |
| **E. Tagging** | 3h | Medium | ⭐⭐⭐ | Low | ⚠️ **Nice to Have** |
| **F. Better Comparison** | 3h | Medium | ⭐⭐⭐ | Medium | ⚠️ **Enhancement** |
| **G. Import/Export** | 2h | Low-Med | ⭐⭐ | Low | ⚠️ **Optional** |
| **H. Notifications** | 2h | Low-Med | ⭐⭐ | Low | ⚠️ **Polish** |
| **I. CLI** | 4h | Medium | ⭐⭐⭐⭐ | Medium | ✅ **Automation** |
| **J. GRUB** | 6h | High | ⭐⭐⭐⭐⭐ | High | ⚠️ **Complex** |

---

## 🎯 Recommended Phase 7 Focus

### **Recommended Combo: A + B (Snapshot Sizes + Search)**
**Total Effort:** ~5 hours | **Impact:** Maximum

**Why This Combo:**
1. **Snapshot Sizes (2h)** - Completes the statistics feature from Phase 6
2. **Search/Filter (3h)** - Essential as snapshot count grows
3. Both are high-value, low-technical-debt features
4. Natural progression from Phase 6
5. Makes Waypoint feel complete and polished

**Alternative Combos:**
- **A + C** (Sizes + Policy Editor) - Polish existing features [5h]
- **A + D** (Sizes + Auto Snapshots) - Maximum automation [6h]
- **B + E** (Search + Tags) - Organization focus [6h]

---

## 🚀 My Recommendation

**Start with Option A (Snapshot Sizes)** - It's the quickest win that completes Phase 6's statistics feature. Then add **Option B (Search/Filter)** for a complete Phase 7.

**Or**, if you prefer maximum automation, go with **Option D (Scheduled Snapshots)** for a "set it and forget it" experience.

**What would you like to focus on for Phase 7?**
