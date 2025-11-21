# Waypoint User Guide

A complete guide to using Waypoint for system snapshots and backups on Void Linux.

## Table of Contents

- [Getting Started](#getting-started)
- [Creating Your First Snapshot](#creating-your-first-snapshot)
- [Managing Snapshots](#managing-snapshots)
- [Restoring from a Snapshot](#restoring-from-a-snapshot)
- [Setting Up Automatic Snapshots](#setting-up-automatic-snapshots)
- [Configuring Backups](#configuring-backups)
- [Retention Policies](#retention-policies)
- [Quota Management](#quota-management)
- [Advanced Features](#advanced-features)

## Getting Started

### What is Waypoint?

Waypoint is a snapshot management tool for Btrfs filesystems that allows you to:
- Create point-in-time snapshots of your system
- Restore your system to a previous state
- Schedule automatic snapshots
- Backup snapshots to external drives
- Track package changes between snapshots

### First Launch

After installation, launch Waypoint from your application menu or run:

```sh
waypoint
```

On first launch, you'll see:
- The main snapshot list
- Backup indicator at the bottom
- A "Create Restore Point" button in the header

### Understanding the Interface

**Header Bar:**
- **Create Restore Point** button (left) - Creates a new snapshot
- **Compare** button - Compare two snapshots (enabled when 2 snapshots selected)
- **Search** button (🔍) - Search and filter snapshots
- **Hamburger menu** (☰) - Access analytics, preferences, keyboard shortcuts, and about

**Snapshot List:**
- Each snapshot shows: name, creation time, description, and disk usage
- Click a snapshot row to expand details and actions

**Footer:**
- **Backup status** - Shows current backup status (click to open backup preferences)
  - "All backups current • Last: X ago" = Healthy
  - "X backups pending" = Waiting for drive connection
  - "X backups failed" = Action needed
  - "All destinations disconnected" = Connect backup drives
  - "Backing up..." = Backup in progress

## Creating Your First Snapshot

### Manual Snapshot Creation

1. Click the **"Create Restore Point"** button in the header
2. A dialog will appear with:
   - **Description** field - Enter a meaningful description (e.g., "Before system upgrade")
   - **Subvolumes** - Select which parts of your system to snapshot
     - **/** (root) - System files, installed programs
     - **/home** - User files and settings
     - **/var** - Logs, databases, caches
3. Click **"Create"**
4. Wait for creation (usually 1-10 seconds depending on system size)
5. A notification will confirm success

**Tip:** Always create a snapshot before:
- System upgrades
- Installing new software
- Making system configuration changes
- Major application updates

### Understanding Subvolumes

**What to snapshot:**
- **Root (/)** - Always snapshot this for system rollback
- **/home** - Include if you want to restore user data
- **/var** - Usually not needed unless you need database/log recovery

**Note:** Snapshots are instant and take minimal space initially (copy-on-write).

## Managing Snapshots

### Viewing Snapshot Details

Click any snapshot row to expand it and see:
- **Full name** and **creation timestamp**
- **Description** (if provided)
- **Disk usage** (cached, updates every 5 minutes)
- **Package count** (if package tracking is enabled)
- **Subvolumes** included in the snapshot

### Snapshot Actions

Click the **three-dot menu (⋮)** on any snapshot to:
- **View Details** - See full snapshot information
- **Restore Files** - Restore individual files without full rollback
- **Add/Edit Note** - Add personal notes to the snapshot
- **Compare with Another** - View differences between snapshots
- **Pin/Unpin** - Keep important snapshots at the top
- **Delete Restore Point** - Remove the snapshot (requires confirmation)

### Pinning Snapshots

Pin important snapshots to:
- Keep them at the top of the list
- Protect them from automatic cleanup
- Mark them as "favorites"

To pin: Click ⋮ → Toggle the star icon

### Adding Notes

Add context to snapshots:
1. Click ⋮ → **"Edit Note"**
2. Type your note (e.g., "Clean install before GPU driver update")
3. Press **Ctrl+Enter** to save or **Escape** to cancel

Notes appear truncated in the list, click to see full text.

### Searching and Filtering

Click the **🔍 Search** button to:
- **Text search** - Find snapshots by name or description
- **Date filter** - Show snapshots from:
  - Last 7 days
  - Last 30 days
  - Last 90 days
  - All snapshots

**Keyboard shortcut:** Press **Ctrl+F** to open search

## Restoring from a Snapshot

### Full System Restore (Rollback)

**⚠️ Important:** Full restore reboots your system. Save all work first.

1. Find the snapshot you want to restore to
2. Click the snapshot row to expand it
3. Click **"Restore"** button
4. Choose **"Restore Full System"**
5. Review the rollback preview showing:
   - **Package changes** (added, removed, upgraded, downgraded)
   - **Kernel version** comparison
   - **Affected subvolumes**
6. Click **"Restore and Reboot"** to proceed
7. System will reboot into the restored state

**Safety feature:** Waypoint automatically creates a safety backup before rollback, allowing you to undo if needed.

**Safety validations:** During multi-subvolume restores, Waypoint validates /etc/fstab to ensure all mount points are correct. If validation fails, the restore is cancelled before any changes are made. Temporary writable copies are automatically cleaned up after restore.

### Restoring Individual Files

Restore specific files without full system rollback:

1. Click the snapshot row
2. Click **"Restore"** button
3. Choose **"Restore Individual Files"**
4. A file browser opens showing snapshot contents
5. Navigate to the files you want
6. Select files/folders and click **"Restore Selected"**
7. Choose restore destination
8. Files are copied back

**Use cases:**
- Recover accidentally deleted files
- Restore old configuration files
- Get previous versions of documents

## Setting Up Automatic Snapshots

### Accessing Scheduler Settings

1. Open hamburger menu (☰) → **"Preferences"**
2. Click **"Scheduled Snapshots"** tab
3. You'll see the scheduler service status and schedule cards

### Understanding the Service Status

**Green circle** + "Running" = Scheduler is active
**Red circle** + "Stopped" = Scheduler is not running
**Gray circle** + "Disabled" = Scheduler service not installed

If stopped, click the info bar to restart.

### Configuring Schedules

Waypoint supports four schedule types:

#### Hourly Snapshots
- Creates snapshots every hour
- Good for: Active development, frequent changes
- Example: hourly-20251118-1400

#### Daily Snapshots
- Creates one snapshot per day at specified time
- Good for: General system protection
- Example: daily-20251118-0200

#### Weekly Snapshots
- Creates one snapshot per week on specified day
- Good for: Long-term checkpoints
- Example: weekly-20251118

#### Monthly Snapshots
- Creates one snapshot per month
- Good for: Archival purposes
- Example: monthly-202511

### Enabling a Schedule

1. Find the schedule card (e.g., "Hourly Snapshots")
2. Toggle the **switch** at the top to enable
3. Click **"Edit"** to configure:
   - **Prefix** - Snapshot name prefix (default: hourly, daily, etc.)
   - **Description** - Optional description for snapshots
   - **Subvolumes** - Which parts to snapshot (/, /home, /var)
     - **Note:** Root filesystem (/) is always included and cannot be disabled
   - **Retention** - How many to keep (see [Retention Policies](#retention-policies))

### Quick Setup Example

For basic protection:

1. **Enable Daily snapshots**
   - Set time: 2:00 AM (when you're likely not using the system)
   - Subvolumes: / and /home
   - Retention: Keep 7 daily, 4 weekly, 3 monthly

2. **Optional: Enable Weekly snapshots**
   - Day: Sunday at 3:00 AM
   - Subvolumes: / and /home
   - Retention: Keep 4 weekly snapshots

### Viewing Schedule Status

Each schedule card shows:
- **Next run** - When the next snapshot will be created
- **Last success** - When the last snapshot was created
- **Sparkline** - Visual history of recent snapshots (green dots = created snapshots)

## Configuring Backups

### Why Backup Snapshots?

Snapshots protect against software issues, but not:
- Hardware failure (dead disk)
- Filesystem corruption
- Accidental deletion of snapshots

**Solution:** Backup snapshots to external drives.

### Setting Up Backup Destinations

1. Connect an external drive (USB drive, external SSD, etc.)
2. Open hamburger menu → **"Preferences"** → **"Backups"** tab
3. Wait for drive to appear in **"Backup Destinations"** (auto-scans every 5 seconds)
4. Click on the drive to configure it
5. Configure backup settings:
   - **Enable backups** - Toggle switch to activate
   - **Backup filter** - Choose which snapshots to backup:
     - **All** - Backup every snapshot
     - **Favorites** - Only backup pinned snapshots
     - **Last 7 days** - Only recent snapshots
     - **Last 30 days** - Last month of snapshots
     - **Critical** - System snapshots only (excludes user data)
   - **Backup triggers**:
     - **Backup on snapshot creation** - Automatically backup when new snapshot is created
     - **Backup on drive mount** - Backup pending snapshots when drive is connected
   - **Retention** - Automatically delete backups older than X days
6. Click **"Save"**

**Automatic backup workflow:**
1. You create a snapshot (manual or scheduled)
2. If "Backup on snapshot creation" is enabled and drive is connected: Backup starts immediately
3. If drive is disconnected: Backup added to pending queue
4. When drive reconnects: Pending backups process automatically

**Note:** Scheduled snapshots (hourly, daily, weekly, monthly) automatically trigger backups when created, making automated backup workflows seamless.

### Backup Types

**Btrfs drives:**
- Uses incremental backups (btrfs send/receive)
- First backup: Full copy (slow)
- Subsequent backups: Only changes (fast)
- Most efficient for Btrfs-to-Btrfs

**Non-Btrfs drives (NTFS, exFAT, network shares):**
- Uses rsync for full backups
- Every backup is complete copy
- Slower but works with any filesystem
- Good for universal compatibility

### Monitoring Backup Progress

**Pending Backups section** shows:
- **Queue Status** - Summary of pending/in-progress/failed backups
- **Individual backups** - Each backup with progress bar and status

**Progress tracking:**
- Real-time transfer speed
- Bytes transferred / total bytes
- Current stage (preparing, transferring, verifying, complete)

### Drive Health

Each drive shows:
- **Total space** and **available space**
- **Number of backups** stored
- **Last backup** timestamp
- **Filesystem type** (Btrfs, NTFS, exFAT, etc.)

### Managing Pending Backups

**Viewing pending backups:**
1. Open hamburger menu → **"Preferences"** → **"Backups"** tab
2. Scroll to **"Pending Backups"** section
3. See list of backups waiting for drive connection

**Pending backups are processed automatically when:**
- The destination drive is connected
- "Backup on drive mount" is enabled
- Waypoint is running

**Manual processing:**
- Connect the drive and wait 5-10 seconds
- Backups process in chronological order (oldest first)
- Watch progress in real-time

### Handling Failed Backups

**If backups fail:**
1. Check the **"Failed Backups"** section in Backups preferences
2. Review the error message for each failed backup
3. Fix the issue (e.g., free up drive space, check permissions)
4. Click **"Retry Failed"** button to retry all failed backups

**Common failure causes:**
- Drive full (no space left)
- Drive disconnected during backup
- Permission issues
- Drive filesystem errors

### Deleting Backups

**To free up space on backup drives:**
1. Go to Preferences → Backups
2. Click on the destination drive
3. View list of backups
4. Select backups to delete
5. Click **"Delete Selected"**
6. Confirm deletion

**Or use retention policies:**
- Set "Delete backups older than X days" when configuring destination
- Backups are automatically deleted after the specified age

## Retention Policies

### Why Use Retention?

Without retention, snapshots accumulate and consume disk space. Retention policies automatically delete old snapshots while keeping recent ones.

### Timeline-Based Retention (Recommended)

Timeline retention keeps snapshots distributed across time periods:

**Example configuration:**
- **Hourly bucket:** Keep last 6 snapshots (6 hours of coverage)
- **Daily bucket:** Keep last 7 snapshots (1 week of coverage)
- **Weekly bucket:** Keep last 4 snapshots (1 month of coverage)
- **Monthly bucket:** Keep last 3 snapshots (3 months of coverage)
- **Yearly bucket:** Keep last 2 snapshots (2 years of coverage)

**How it works:**
- Most recent hourly snapshots are kept
- Older snapshots are "promoted" to daily/weekly/monthly buckets
- Provides good coverage without excessive disk usage

### Configuring Timeline Retention

1. Open hamburger menu → **"Preferences"** → **"Scheduled Snapshots"**
2. Click **"Edit"** on any schedule card
3. Scroll to **"Timeline Retention"** section
4. Configure each bucket:
   - Set count to 0 to disable that bucket
   - Recommended: At least daily and weekly buckets
5. Click **"Save"**

### Per-Schedule vs Global Retention

**Per-schedule retention** (recommended):
- Each schedule (hourly, daily, weekly, monthly) has its own policy
- More flexible control
- Example: Keep 24 hourly, 7 daily, 4 weekly, 3 monthly

**Global retention:**
- Apply one policy to all snapshots
- Simpler but less flexible

### Protected Snapshots

Snapshots are **never** deleted by retention if:
- **Pinned** (marked as favorite)
- **Manual snapshots** (created via "Create Restore Point" button)
- **Less than minimum count** (safety setting)

## Quota Management

### What are Btrfs Quotas?

Quotas limit how much disk space snapshots can use, preventing snapshots from filling your entire disk.

### Enabling Quotas

1. Open hamburger menu → **"Preferences"** → **"Quotas"** tab
2. Toggle **"Enable Btrfs Quotas"**
3. Choose quota type:
   - **Simple quotas** (recommended) - Easier, better performance
   - **Traditional qgroups** - More features, slower
4. Set a **quota limit** (e.g., 50GB, 100GB)
5. Optionally enable **"Automatically delete old snapshots when quota reached"**

### Monitoring Quota Usage

The Quotas tab shows:
- **Current usage** - How much space snapshots are using
- **Limit** - Maximum allowed space
- **Percentage used** - Visual progress bar

Color coding:
- **Green** - Usage is healthy
- **Yellow** - Approaching limit
- **Red** - At or near limit

### Quota-Based Cleanup

When enabled, Waypoint automatically:
1. Detects when quota limit is reached
2. Deletes oldest snapshots (respecting pinned snapshots)
3. Continues until usage drops below limit

## Advanced Features

### Exclusion Patterns

Reduce snapshot sizes by excluding unnecessary files and directories.

**Accessing exclusions:**
1. Open hamburger menu → **"Preferences"** → **"Exclusions"** tab
2. View system defaults and add custom patterns

**System default exclusions:**
- `/var/cache` - Package manager caches
- `/tmp` - Temporary files
- Browser caches and other temporary data

**Adding custom exclusions:**
1. Click **"Add Pattern"** button
2. Enter the pattern (e.g., `/var/log`)
3. Choose pattern type:
   - **Prefix** - Matches paths starting with pattern (e.g., `/var/cache`)
   - **Suffix** - Matches paths ending with pattern (e.g., `.log`)
   - **Glob** - Wildcard matching (e.g., `/home/*/.cache/*`)
   - **Exact** - Exact path match only
4. Add a description (optional but helpful)
5. Click **"Add"**

**Important notes:**
- Exclusions only apply to **new snapshots**, not existing ones
- Excluded paths are deleted from snapshots after creation
- Be careful not to exclude important system files
- Test with a manual snapshot before enabling for scheduled snapshots

**Common patterns to exclude:**
- `/var/cache` - Package caches (can be regenerated)
- `/tmp` - Temporary files
- `/var/tmp` - More temporary files
- `.cache` - User application caches (use Contains pattern)
- `.thumbnails` - Image thumbnails
- `node_modules` - JavaScript dependencies (for developers)

### Package Tracking

Waypoint automatically tracks installed packages (XBPS) when creating snapshots.

**Viewing package changes:**
1. Click **"Compare"** button in header
2. Select two snapshots
3. View **"Package Diff"** tab showing:
   - Added packages (green)
   - Removed packages (red)
   - Upgraded packages (blue, with version change)
   - Downgraded packages (orange, with version change)

### Snapshot Comparison

Compare two snapshots to see what changed:

1. Click **"Compare"** button in main window
2. Select **base snapshot** and **compare to** snapshot from dropdowns
3. View **Summary** section showing:
   - **Package Changes** - Count of added/removed/changed packages
   - **File Changes** - Total number of modified files
4. Click **"View Packages"** to see detailed package differences (added, removed, upgraded, downgraded)
5. Click **"View Files"** to see file-level changes organized by directory:
   - Changes grouped by top-level directory (e.g., /etc, /usr/lib, /home/user)
   - Sorted by number of changes (largest first)
   - Shows up to 5 files per directory with expandable groups
   - Color-coded by change type (Added, Modified, Deleted)

**Export comparison:**
Click **"Export"** button in package or file view to save comparison report as text file.

### Analytics Dashboard

View snapshot statistics and insights:

1. Open hamburger menu → **"Analytics"**
2. See:
   - **Total snapshots** and **total size**
   - **Space usage trends** over time
   - **Largest snapshots** (identify space hogs)
   - **Actionable insights** (recommendations)

### Keyboard Shortcuts

Press **Ctrl+?** or hamburger menu → **"Keyboard Shortcuts"** to see all available shortcuts:

**General:**
- **Ctrl+F** - Open search
- **Ctrl+N** - Create new restore point
- **Ctrl+R** or **F5** - Refresh snapshot list
- **Ctrl+,** - Open preferences
- **Escape** - Close search bar

**Note Editing:**
- **Ctrl+Enter** - Save note changes
- **Escape** - Cancel editing

### Command Line Interface

Waypoint includes a CLI for scripting and automation:

```sh
# List all snapshots
waypoint-cli list

# Create snapshot
waypoint-cli create "my-snapshot" "Description"

# Delete snapshot
waypoint-cli delete "my-snapshot"

# Show detailed info
waypoint-cli show "my-snapshot"

# Compare snapshots
waypoint-cli diff "snapshot1" "snapshot2"
```

See `waypoint-cli --help` for all commands.

### Browse Snapshots in File Manager

Open any snapshot in your file manager:

1. Click snapshot row to expand
2. Click **"Browse in File Manager"**
3. Snapshot opens in your default file manager (Thunar, Nautilus, etc.)
4. Navigate and view files without restoring

**Read-only:** Snapshot contents cannot be modified.

## Best Practices

### Snapshot Frequency

**Recommended setup:**
- **Hourly** - If you do frequent system changes
- **Daily** - Minimum for general protection (at 2 AM)
- **Weekly** - For long-term checkpoints (Sunday)
- **Before major changes** - Always create manual snapshot

### Backup Strategy

**The 3-2-1 rule:**
- **3** copies of data (original + 2 backups)
- **2** different media types (internal disk + external drive)
- **1** off-site backup (optional: network share, cloud)

**For Waypoint:**
1. System with snapshots (copy 1)
2. External drive backups (copy 2)
3. Optional: Network share backups (copy 3)

**Backup filter recommendations:**
- **Home user:** Use "All" or "Favorites" filter
  - Pin important snapshots before upgrades
  - Regular backups capture everything
- **Developer:** Use "Last 7 days" or "Favorites"
  - Recent work is backed up
  - Save space by not backing up old dev snapshots
- **Server:** Use "Critical" filter
  - Only backup system snapshots (/)
  - Exclude user data if backed up separately

**Backup automation tips:**
- Enable "Backup on snapshot creation" for critical systems
- Enable "Backup on drive mount" for portable drives
- Use backup retention to manage drive space (e.g., 30-90 days)
- Keep at least one backup drive offsite for disaster recovery

### Retention Guidelines

**Conservative (lots of history):**
- Hourly: 24
- Daily: 7
- Weekly: 4
- Monthly: 6
- Yearly: 2

**Balanced (moderate history):**
- Hourly: 6
- Daily: 7
- Weekly: 4
- Monthly: 3
- Yearly: 0

**Aggressive (minimal disk usage):**
- Hourly: 0
- Daily: 3
- Weekly: 2
- Monthly: 1
- Yearly: 0

### Quota Recommendations

Set quota limit to **20-30% of total disk space** for good balance.

**Examples:**
- 500GB disk → 100-150GB quota
- 1TB disk → 200-300GB quota
- 2TB disk → 400-600GB quota

### Exclusion Best Practices

**Always exclude:**
- `/tmp` and `/var/tmp` - Temporary files
- `/var/cache` - Package manager caches
- Browser caches (`~/.cache/mozilla`, `~/.cache/chromium`)

**Consider excluding (depending on needs):**
- Log files (`/var/log`) - If you don't need historical logs
- Download directories - Large files you can re-download
- Build artifacts (`node_modules`, `target/`, `build/`) - For developers

**Never exclude:**
- System configuration (`/etc`)
- Boot files (`/boot`)
- User documents and important data
- Application binaries (`/usr`, `/bin`, `/sbin`)

**Test your exclusions:**
1. Add exclusion patterns
2. Create a test snapshot
3. Browse the snapshot to verify important files are still included
4. Adjust patterns as needed

## Next Steps

- **Set up automatic snapshots** - Start with daily snapshots
- **Configure backups** - Connect an external drive
- **Enable retention** - Prevent disk from filling up
- **Create before upgrades** - Make it a habit

For troubleshooting, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
For API integration, see [API.md](API.md).
For advanced configuration, see [ARCHITECTURE.md](ARCHITECTURE.md).
