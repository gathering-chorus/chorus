#!/bin/bash
# bedroom-cleanup.sh — Run on Bedroom Mac via screen share
# From audit #1579, 2026-03-21
# Expected savings: ~14% CPU, ~470MB RAM
set -euo pipefail

echo "=== Bedroom Mac Cleanup ==="
echo ""

# 1. Disable Bluetooth (saves ~4% CPU on headless server)
echo "1. Disabling Bluetooth..."
sudo defaults write /Library/Preferences/com.apple.Bluetooth ControllerPowerState -int 0
sudo killall -HUP bluetoothd 2>/dev/null || true
echo "   ✓ Bluetooth disabled"
echo ""

# 2. Stop CleanMyMac/Toolkit (saves ~7% CPU + 470MB RAM)
echo "2. Stopping CleanMyMac Toolkit..."
launchctl bootout gui/$(id -u)/com.macpaw.CleanMyMac4.Updater 2>/dev/null && echo "   ✓ Updater unloaded" || echo "   - Already unloaded"
pkill -f "Toolkit.*autoLaunched" 2>/dev/null && echo "   ✓ Toolkit process killed" || echo "   - Not running"
echo ""

# 3. Fix Navidrome (restart it)
echo "3. Restarting Navidrome..."
launchctl kickstart -k gui/$(id -u)/com.gathering.navidrome 2>/dev/null && echo "   ✓ Navidrome restarted" || echo "   - Failed to restart (check plist)"
echo ""

# 6. Verify
echo "=== Results ==="
echo "Load: $(uptime | awk -F'load averages:' '{print $2}')"
echo "Top CPU:"
ps -eo %cpu,command -r | head -6 | tail -5
echo ""
echo "Done. Check load again in 5 minutes after daemons settle."
