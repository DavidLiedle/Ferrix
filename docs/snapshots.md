# Snapshots and Recovery

Ferrix provides revolutionary session persistence through its snapshot system, ensuring you never lose work due to crashes, reboots, or accidents.

## Overview

Snapshots capture the complete state of a session including:
- All windows and their names
- Pane layouts and dimensions
- Working directories
- Running processes and commands
- Scrollback buffer contents
- Environment variables
- Cursor positions

## Manual Snapshots

### Creating Snapshots

```bash
# Quick snapshot of current session
ferrix save-snapshot session-name

# Snapshot with custom name
ferrix save-snapshot session-name --name "before-upgrade"

# Snapshot with description
ferrix save-snapshot session-name \
  --name "stable-state" \
  --description "Working configuration before experimenting"

# From within session (with keybinding)
# Press: Ctrl-b S
```

### Listing Snapshots

```bash
# List all snapshots
ferrix list-snapshots

# Output format:
# Created              Name                    Size      Path
# 2024-01-20 14:30:00  before-upgrade         1.2MB     ~/.ferrix/snapshots/...
# 2024-01-20 13:00:00  auto_recovery_main     0.8MB     ~/.ferrix/snapshots/auto/...
```

### Loading Snapshots

```bash
# Load specific snapshot
ferrix load-snapshot ~/.ferrix/snapshots/session_20240120_143000.snapshot

# Load and attach immediately
ferrix load-snapshot /path/to/snapshot.ferrix && ferrix attach

# From within session
# Press: Ctrl-b R
# Then enter snapshot path
```

### Deleting Snapshots

```bash
# Delete specific snapshot
ferrix delete-snapshot /path/to/snapshot.ferrix

# Clean up old auto-snapshots
find ~/.ferrix/snapshots/auto -mtime +30 -delete
```

## Automatic Snapshots

### Auto-Save Configuration

Configure in `~/.ferrixrc`:

```bash
# Enable auto-save
set auto-save on

# Interval in seconds (default: 300 = 5 minutes)
set auto-save-interval 300

# Save on detach
set auto-save-on-detach on

# Save on exit
set auto-save-on-exit on

# Maximum auto-snapshots to keep
set auto-save-max-snapshots 10
```

### Auto-Save Locations

Auto-snapshots are stored in:
```
~/.ferrix/snapshots/auto/
├── auto_main_20240120_140000_uuid.ferrix.snapshot
├── auto_main_20240120_140500_uuid.ferrix.snapshot
└── auto_dev_20240120_141000_uuid.ferrix.snapshot
```

## Crash Recovery

### How It Works

1. **Continuous Monitoring**: Ferrix tracks session state continuously
2. **Recovery File**: Creates `.ferrix_recovery` file during operation
3. **Crash Detection**: Checks for recovery file on startup
4. **Automatic Restoration**: Restores sessions if unclean shutdown detected
5. **Clean Shutdown**: Removes recovery file on normal exit

### Recovery Process

When Ferrix starts after a crash:

```bash
$ ferrix
Recovery file found, checking for crashed sessions...
Recovered session main (4c09a048-ff45-43d6-8380-cc3d4623f093) from crash
Recovered session dev (5d19b158-gg56-52e7-9491-dd5d5734f1a4) from crash

# Sessions are automatically restored
$ ferrix list
Active sessions:
  main (4c09a048-ff45-43d6-8380-cc3d4623f093) - 3 windows
  dev (5d19b158-gg56-52e7-9491-dd5d5734f1a4) - 2 windows
```

### Manual Recovery

If automatic recovery fails:

```bash
# List available recovery snapshots
ls -la ~/.ferrix/snapshots/auto/

# Manually load most recent
ferrix load-snapshot ~/.ferrix/snapshots/auto/auto_main_*.snapshot

# Load from specific time
ferrix load-snapshot ~/.ferrix/snapshots/auto/auto_main_20240120_140000*.snapshot
```

## Export and Import

### Exporting Snapshots

```bash
# Export to compressed archive
ferrix export-snapshot /path/to/snapshot.ferrix /tmp/backup.gz

# Export with timestamp
ferrix export-snapshot ~/.ferrix/snapshots/main.snapshot \
  ~/backups/ferrix-main-$(date +%Y%m%d).gz
```

### Importing Snapshots

```bash
# Import from archive
ferrix import-snapshot ~/backups/ferrix-main-20240120.gz

# Import and load immediately
ferrix import-snapshot backup.gz && \
  ferrix load-snapshot ~/.ferrix/snapshots/latest.snapshot
```

### Sharing Sessions

```bash
# On machine A: Export session
ferrix save-snapshot work --name "project-setup"
ferrix export-snapshot ~/.ferrix/snapshots/work*.snapshot project.gz

# Transfer file (scp, rsync, etc.)
scp project.gz user@machine-b:~/

# On machine B: Import and load
ferrix import-snapshot ~/project.gz
ferrix list-snapshots
ferrix load-snapshot ~/.ferrix/snapshots/work*.snapshot
```

## Snapshot Format

### File Structure

Snapshots are JSON files with optional compression:

```json
{
  "metadata": {
    "id": "960f04c2-9667-4196-a9bf-b215d3a7a1ed",
    "name": "before-upgrade",
    "description": "Stable state",
    "created_at": "2024-01-20T14:30:00Z",
    "ferrix_version": "0.1.0",
    "checksum": "a1b2c3d4e5f6"
  },
  "session": {
    "id": "4c09a048-ff45-43d6-8380-cc3d4623f093",
    "name": "main",
    "created_at": "2024-01-20T10:00:00Z",
    "environment": [
      ["PATH", "/usr/bin:/bin"],
      ["HOME", "/home/user"]
    ]
  },
  "windows": [...],
  "panes": [...]
}
```

### Integrity Verification

Snapshots include MD5 checksums for integrity:

```bash
# Verification happens automatically on load
ferrix load-snapshot snapshot.ferrix

# Manual verification
md5sum ~/.ferrix/snapshots/*.snapshot
```

## Advanced Configuration

### Hooks for Snapshots

```bash
# ~/.ferrixrc

# Auto-snapshot before risky operations
hook before-kill-session 'save-snapshot #{session_name} --name "before-kill"'
hook before-kill-server 'save-snapshot --all --name "before-shutdown"'

# Snapshot on specific events
hook after-new-window 'save-snapshot --auto'
hook pane-died 'save-snapshot #{session_name} --name "pane-died"'

# Daily snapshot via cron
# Add to crontab:
# 0 12 * * * ferrix save-snapshot main --name "daily-backup"
```

### Snapshot Strategies

#### Development Workflow
```bash
# Before major changes
alias snapshot-before='ferrix save-snapshot $(ferrix info -t . -F "#{session_name}") --name "before-$(date +%H%M)"'

# After successful build
hook after-run-tests 'save-snapshot --auto --name "tests-passed"'
```

#### Production Sessions
```bash
# Frequent auto-saves
set auto-save-interval 60        # Every minute
set auto-save-max-snapshots 100  # Keep more history

# Backup to remote
hook after-save-snapshot 'rsync -av ~/.ferrix/snapshots/ backup-server:/backups/ferrix/'
```

#### Experimentation
```bash
# Create restore point
ferrix save-snapshot dev --name "restore-point"

# Experiment freely...

# Quick restore if needed
ferrix load-snapshot ~/.ferrix/snapshots/dev*restore-point*
```

## Performance Considerations

### Snapshot Size

Typical snapshot sizes:
- Minimal session: 10-50 KB
- Average session: 100-500 KB
- Large session (many panes/scrollback): 1-10 MB

### Optimization Tips

```bash
# Limit scrollback for smaller snapshots
set history-limit 10000

# Exclude certain windows from snapshots
set @snapshot-exclude-windows "monitoring,logs"

# Compress snapshots
set @snapshot-compression gzip

# Clean old snapshots automatically
set auto-save-max-snapshots 20
```

### Storage Management

```bash
# Check snapshot storage usage
du -sh ~/.ferrix/snapshots/

# Clean snapshots older than 30 days
find ~/.ferrix/snapshots -name "*.snapshot" -mtime +30 -delete

# Keep only last N auto-snapshots
ls -t ~/.ferrix/snapshots/auto/*.snapshot | tail -n +11 | xargs rm -f
```

## Troubleshooting

### Snapshot Won't Load

```bash
# Check file exists and permissions
ls -la /path/to/snapshot
file /path/to/snapshot

# Validate snapshot format
ferrix validate-snapshot /path/to/snapshot

# Try importing if corrupted
ferrix import-snapshot /path/to/snapshot
```

### Recovery Not Working

```bash
# Check recovery file
ls -la ~/.ferrix/.ferrix_recovery

# Manually trigger recovery
ferrix recover --force

# Check auto-snapshots
ls -la ~/.ferrix/snapshots/auto/
```

### Performance Issues

```bash
# Disable auto-save temporarily
set auto-save off

# Reduce snapshot frequency
set auto-save-interval 1800  # 30 minutes

# Clear old snapshots
rm -rf ~/.ferrix/snapshots/auto/*
```

## Best Practices

1. **Regular Snapshots**: Configure reasonable auto-save intervals
2. **Named Snapshots**: Use descriptive names for manual snapshots
3. **Before Changes**: Always snapshot before major changes
4. **Periodic Cleanup**: Remove old snapshots regularly
5. **Off-site Backup**: Sync important snapshots to remote storage
6. **Test Recovery**: Periodically test snapshot restoration
7. **Document Setup**: Save snapshots of complex session setups

## Integration Examples

### Git Hooks

```bash
# .git/hooks/pre-commit
#!/bin/bash
ferrix save-snapshot dev --name "pre-commit-$(git rev-parse --short HEAD)"
```

### CI/CD Pipeline

```yaml
# .github/workflows/snapshot.yml
- name: Save Ferrix Session
  run: |
    ferrix save-snapshot ci --name "ci-build-${{ github.run_number }}"
    ferrix export-snapshot ~/.ferrix/snapshots/ci*.snapshot artifacts/
```

### Backup Script

```bash
#!/bin/bash
# backup-ferrix.sh

BACKUP_DIR="/backup/ferrix/$(date +%Y%m%d)"
mkdir -p "$BACKUP_DIR"

# Export all sessions
for snapshot in ~/.ferrix/snapshots/*.snapshot; do
  name=$(basename "$snapshot" .snapshot)
  ferrix export-snapshot "$snapshot" "$BACKUP_DIR/$name.gz"
done

# Sync to cloud
rclone sync "$BACKUP_DIR" remote:ferrix-backups/
```

## Future Features

Planned enhancements:
- Incremental snapshots
- Snapshot diffing
- Cloud sync integration
- Encrypted snapshots
- Snapshot templates
- Time-machine style browsing
- Cross-platform snapshot compatibility