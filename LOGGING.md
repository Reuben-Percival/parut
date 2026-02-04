# Parut Logging System

## Overview

Parut now includes a comprehensive logging system that tracks all application operations, package installations, removals, updates, and errors.

## Log Location

All logs are stored in:
```
~/.parut/parut.log
```

For example, if your username is "terra", the log file will be at:
```
/home/terra/.parut/parut.log
```

## What Gets Logged

### Application Events
- Application start and exit
- UI initialization
- Refresh operations

### Package Operations
- Package searches (with query and result count)
- Package installations (start and completion)
- Package removals (start and completion)
- System updates (start and completion)
- PKGBUILD fetches for AUR packages

### Package Listing
- Installed package lists (with count and AUR package count)
- Available updates (with count)

### Errors and Warnings
- Failed operations with error messages
- Search failures
- Installation/removal failures
- PKGBUILD fetch errors

## Log Format

Each log entry includes:
- **Timestamp**: `YYYY-MM-DD HH:MM:SS`
- **Level**: INFO, WARN, ERROR, or DEBUG
- **Message**: Description of the event

Example log entries:
```
[2024-02-04 14:30:15] INFO: Parut application starting
[2024-02-04 14:30:16] INFO: Building UI
[2024-02-04 14:30:17] INFO: Listed 1250 installed packages (45 from AUR)
[2024-02-04 14:30:18] INFO: Found 12 available updates
[2024-02-04 14:32:05] INFO: Starting installation of package: firefox
[2024-02-04 14:32:45] INFO: Successfully installed package: firefox
[2024-02-04 14:35:22] ERROR: Failed to install package vim-enhanced: Operation failed
```

## Log Levels

- **INFO**: Normal operation events (installations, searches, refreshes)
- **WARN**: Warning messages (non-critical issues)
- **ERROR**: Error messages (failed operations)
- **DEBUG**: Detailed debug information (search queries, package sources)

## Viewing Logs

You can view the logs in real-time using:

```bash
# View entire log
cat ~/.parut/parut.log

# View last 20 lines
tail -n 20 ~/.parut/parut.log

# Follow log in real-time (watch as things happen)
tail -f ~/.parut/parut.log

# View only errors
grep ERROR ~/.parut/parut.log

# View logs from today
grep "$(date +%Y-%m-%d)" ~/.parut/parut.log
```

## Log Rotation

The log file will grow over time. You can manually clear it if needed:

```bash
# Clear the log file
> ~/.parut/parut.log

# Or delete it (will be recreated on next app start)
rm ~/.parut/parut.log
```

Consider setting up logrotate or a simple cron job if you want automatic log management:

```bash
# Add to crontab (monthly cleanup)
0 0 1 * * truncate -s 0 ~/.parut/parut.log
```

## Privacy Notes

The logs contain:
- Package names you've searched for, installed, or removed
- Timestamps of operations
- Error messages from package operations

The logs do NOT contain:
- Personal data beyond username (from filesystem path)
- Network traffic or sensitive information
- Passwords or authentication tokens

## Debugging

If you encounter issues with Parut, the log file is the first place to look. When reporting bugs, include relevant log excerpts (but remove any sensitive paths if needed).

## Implementation Details

The logging system uses:
- **chrono**: For timestamp generation
- **dirs**: For locating the user's home directory
- Thread-safe singleton logger
- Automatic directory creation
- Fallback to `/tmp/parut` if home directory is unavailable
