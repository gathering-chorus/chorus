#!/usr/bin/env bash
# host-pressure.check.sh — #3714: is the MACHINE able to serve at all?
#
# 2026-07-30 09:37-09:50: clearing-http, gathering-app, cloudflare-tunnel and
# clearing-external all fired inside 13 minutes. Nothing was broken. The host had
# 494MB free with 13.2GB swapped; every service accepted its connection and then
# never answered. Four alarms, four accused subsystems, one cause — and not one
# of them mentioned memory. Freeing a single 1.1GB runaway brought them all back.
#
# This check exists so that condition has ONE name. Service checks that depend on
# the host answering should defer to it rather than each blaming themselves.
set -uo pipefail

# #4651: fire on the kernel's own verdict, not on a threshold of ours. On
# 2026-10-19 this check fired 9 times with swap at 91-95% and every service
# answering; the kernel's memory-pressure level read 2 (warn) each time. Swap %
# cannot tell "busy" from "cannot serve" — the kernel's level can: 1 normal,
# 2 warn, 4 critical. Free % and swap % stay in the observation for the reader.
#
# Seams for the test (test-host-pressure-4651.sh): HOST_PRESSURE_LEVEL_NOW,
# HOST_PRESSURE_FREE_NOW, HOST_PRESSURE_SWAP_NOW replace the live reads when set.
if [ -n "${HOST_PRESSURE_LEVEL_NOW+x}" ]; then level="$HOST_PRESSURE_LEVEL_NOW"
else level=$(sysctl -n kern.memorystatus_vm_pressure_level 2>/dev/null); fi
case "$level" in
  1) level_name=normal ;; 2) level_name=warn ;; 4) level_name=critical ;;
  *) echo "host-pressure-unknown: kern.memorystatus_vm_pressure_level unreadable (${level:-empty})"; exit 0 ;;
esac

# "Pages free" is NOT a signal — the kernel keeps it near zero by design. Use the
# figure `memory_pressure` reports (free + inactive + speculative).
if [ -n "${HOST_PRESSURE_FREE_NOW+x}" ]; then free_pct="$HOST_PRESSURE_FREE_NOW"
else free_pct=$(memory_pressure 2>/dev/null | sed -nE 's/.*free percentage: *([0-9]+)%?.*/\1/p' | head -1); fi

if [ -n "${HOST_PRESSURE_SWAP_NOW+x}" ]; then swap_pct="$HOST_PRESSURE_SWAP_NOW"
else
  read -r swap_total swap_used <<<"$(sysctl -n vm.swapusage 2>/dev/null \
    | sed -E 's/.*total = ([0-9.]+)M.*used = ([0-9.]+)M.*/\1 \2/')"
  swap_pct=$(python3 -c "print(int(100*$swap_used/$swap_total) if $swap_total>0 else 0)" 2>/dev/null || echo unknown)
fi

obs="level=${level} (${level_name}; fires at 4 critical) free_pct=${free_pct:-unknown} swap_used_pct=${swap_pct:-unknown}"

# Name the biggest resident process — on 2026-07-30 that one line WAS the fix.
top=$(ps -Ao rss,pid,comm -m 2>/dev/null | sed -n '2p' | awk '{printf "%dMB pid=%s %s", $1/1024, $2, $3}')

if [ "$level" -eq 4 ]; then
  echo "host-under-pressure: ${obs} largest=${top}"
  exit 1
fi
echo "ok: ${obs}"
exit 0
