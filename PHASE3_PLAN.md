# Phase 3 Implementation Plan

## Overview

Phase 3 transforms Waypoint from a snapshot manager into a complete system recovery tool with automatic rollback capabilities.

## Core Features

### 1. Automatic Snapshot Rollback ⭐ Priority #1

**Objective**: Allow users to restore their system to a previous snapshot with one click.

#### Approach: Btrfs Subvolume Default

**How it works:**
1. User clicks "Restore" on a snapshot
2. Show detailed confirmation dialog with warnings
3. Use `btrfs subvolume set-default <snapshot-id> /` to change default subvolume
4. Prompt user to reboot
5. On reboot, system boots into the snapshot

**Implementation Steps:**
- [ ] Get subvolume ID for a snapshot path
- [ ] Implement `btrfs subvolume set-default` wrapper
- [ ] Create detailed rollback confirmation dialog
- [ ] Add reboot prompt after successful rollback
- [ ] Handle errors gracefully

**Safety Considerations:**
- ⚠️ CRITICAL: This changes boot behavior
- Must verify subvolume exists before setting as default
- Must check if snapshot is read-only (need to create RW snapshot for boot)
- Should create a "pre-rollback" snapshot as backup
- Clear warnings about data loss for changes after snapshot

**Alternative Approach (Future):**
- GRUB integration for boot-time selection
- Allows trying snapshot without commitment
- More complex, Phase 4 feature

---

### 2. Package State Tracking

**Objective**: Record and compare installed packages across snapshots.

#### Data Collection

**At snapshot creation:**
```bash
xbps-query -l > snapshot-packages.txt
```

**Store in metadata:**
```json
{
  "packages": [
    {"name": "firefox", "version": "120.0_1"},
    {"name": "vim", "version": "9.0.2103_1"}
  ]
}
```

**Implementation:**
- [ ] Add `packages` field to `Snapshot` struct
- [ ] Query `xbps-query -l` during snapshot creation
- [ ] Parse output and store in metadata
- [ ] Add package count to UI display

---

### 3. Package Diff View

**Objective**: Show what changed between snapshots.

#### UI Design

**Comparison Dialog:**
```
Comparing: Snapshot A (2025-01-05) → Snapshot B (2025-01-07)

📦 Packages Added (5):
  • firefox-120.0_1
  • rust-1.75.0_1
  ...

📦 Packages Removed (2):
  • chromium-119.0_1
  ...

📦 Packages Updated (12):
  • linux (6.1.69_1 → 6.1.70_1)
  • gtk4 (4.12.0_1 → 4.12.1_1)
  ...

Total: 19 changes
```

**Implementation:**
- [ ] Create diff algorithm for package lists
- [ ] Build diff dialog UI with scrollable lists
- [ ] Add "Compare" button or menu item
- [ ] Color-code changes (green=added, red=removed, blue=updated)

---

### 4. Polkit Integration 🔐

**Objective**: Remove need for `sudo` - seamless privilege escalation.

#### Architecture

**Current (Phase 2):**
```
User runs: sudo waypoint
  ↓
Everything runs as root
```

**Target (Phase 3):**
```
User runs: waypoint (no sudo)
  ↓
UI runs as user
  ↓
Privileged operations → polkit auth → helper binary (as root)
```

#### Components

**A. Helper Binary** (`waypoint-helper`)
- Separate binary that performs privileged operations
- Actions: `create-snapshot`, `delete-snapshot`, `rollback-snapshot`
- Invoked via pkexec with polkit policy
- Returns JSON results to main app

**B. Polkit Policy** (already exists)
- `com.voidlinux.waypoint.policy`
- Already defined actions
- Just needs helper binary implementation

**C. IPC Layer**
- Main app spawns helper with pkexec
- Pass parameters via command-line args
- Read results from stdout (JSON)

**Implementation:**
- [ ] Create `src/bin/waypoint-helper.rs`
- [ ] Implement privileged operations in helper
- [ ] Create IPC wrapper in main app
- [ ] Replace direct btrfs calls with polkit-aware calls
- [ ] Test authentication dialogs

---

### 5. Pre-Upgrade Hook Script

**Objective**: Auto-create snapshots before system upgrades.

#### XBPS Hook

**File**: `/etc/xbps.d/waypoint-pre-upgrade.sh`

```bash
#!/bin/bash
# XBPS hook to create waypoint snapshot before upgrade

case "${XBPS_TARGET_PHASE}" in
  pre)
    if command -v waypoint-helper >/dev/null 2>&1; then
      echo "Creating snapshot before upgrade..."
      pkexec waypoint-helper create-snapshot "Pre-upgrade $(date +%Y-%m-%d)"
    fi
    ;;
esac
```

**Implementation:**
- [ ] Create hook script template
- [ ] Install hook during `make install`
- [ ] Make it configurable (enable/disable)
- [ ] Add UI setting to toggle auto-snapshots

---

## Implementation Order

### Week 1: Core Rollback
1. ✅ Plan architecture (this document)
2. Implement rollback backend (btrfs operations)
3. Create rollback confirmation dialog
4. Test rollback on VM/test system
5. Add safety checks and validation

### Week 2: Package Tracking & Diff
1. Add package querying to snapshot creation
2. Store package data in metadata
3. Implement diff algorithm
4. Create diff view UI
5. Test with real snapshot comparisons

### Week 3: Polkit Integration
1. Create helper binary structure
2. Implement privileged operations in helper
3. Add IPC layer to main app
4. Test authentication flows
5. Update documentation

### Week 4: Polish & Hooks
1. Add pre-upgrade hook
2. Comprehensive testing
3. Documentation updates
4. Performance optimization
5. Bug fixes

---

## Safety & Testing

### Rollback Testing Protocol

**⚠️ CRITICAL: Test on disposable VM first!**

1. **Setup Test Environment:**
   - Spin up Void Linux VM with Btrfs
   - Install waypoint
   - Create test snapshots

2. **Test Rollback Flow:**
   - Create snapshot A
   - Make system changes (install packages)
   - Create snapshot B
   - Roll back to snapshot A
   - Verify system state matches A
   - Check boot process works

3. **Test Failure Scenarios:**
   - Invalid snapshot ID
   - Corrupted subvolume
   - Insufficient permissions
   - Verify error handling

4. **Edge Cases:**
   - Multiple rollbacks
   - Rollback to oldest snapshot
   - Rollback with active applications

---

## Risk Mitigation

### Rollback Risks

| Risk | Mitigation |
|------|------------|
| System won't boot | Create RW snapshot first, test before setting default |
| Data loss | Clear warnings, create pre-rollback snapshot |
| GRUB issues | Document manual recovery, keep kernel accessible |
| Permission errors | Extensive polkit testing |
| Corrupted snapshot | Validate snapshot before rollback |

### Testing Requirements

- [ ] VM testing with multiple rollback scenarios
- [ ] Test on multiple Void Linux configurations
- [ ] Test with different desktop environments
- [ ] Document recovery procedures
- [ ] Create troubleshooting guide

---

## Success Criteria

Phase 3 is complete when:

- [ ] Users can roll back with one click (+ reboot)
- [ ] Rollback works reliably on test systems
- [ ] Package changes are tracked automatically
- [ ] Diff view shows package changes clearly
- [ ] Polkit authentication works seamlessly
- [ ] No sudo required to run app
- [ ] Pre-upgrade hooks work (optional)
- [ ] Comprehensive documentation exists
- [ ] Recovery procedures documented

---

## Future Considerations (Phase 4)

- GRUB integration for boot-time snapshot selection
- Non-Btrfs fallback (rsync-based)
- File-level diff view
- Snapshot scheduling
- Remote backup/export
- Multi-subvolume support (/home snapshots)

---

**Current Status**: Phase 3 Planning Complete - Ready to implement rollback!
