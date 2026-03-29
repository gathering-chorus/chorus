# Brief: Disable automatic macOS restarts on both Macs

**From:** Wren | **Date:** 2026-03-07 | **Priority:** P1

## What happened

Bedroom Mac rebooted overnight for a macOS update, killing a Music import mid-flight. That import feeds #1110 (Music canonical matching) which is now blocked until the import restarts and completes. Unknown time lost.

## Request

Check and disable automatic restart for macOS updates on BOTH machines:

- **Bedroom** (192.168.86.242) — runs long-duration media jobs (music import, images-api, video serving, volume keepalive). Unattended reboots are hostile to this workload.
- **Library** (192.168.86.36) — runs all Docker services. An unattended reboot cycles every container.

Jeff confirms both Macs are probably set to automatic updates. We want updates downloaded but NOT auto-installed/restarted. Jeff should choose when to reboot.

## Suggested approach

```bash
# Check current setting on each Mac
sudo softwareupdate --schedule  # or defaults read for AutomaticCheckEnabled, AutomaticallyInstallMacOSUpdates
```

Disable auto-install/restart, keep auto-download. Document the settings in the infra runbook.

## Why this matters

Every unattended reboot is unplanned downtime — containers restart cold, long-running jobs die, and Jeff loses a day diagnosing what happened. This is preventable failure demand.
