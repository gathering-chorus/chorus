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

# macOS-meaningful signals only. "Pages free" is NOT one — the kernel keeps it
# near zero by design, so a floor on it fires constantly. That is the disease this
# card is curing, and the first draft of this check had it. Use the same figure
# `memory_pressure` reports (free + inactive + speculative are all reclaimable),
# plus swap utilisation.
FREE_PCT_FLOOR="${HOST_PRESSURE_FREE_PCT:-15}"
SWAP_USED_PCT_CEIL="${HOST_PRESSURE_SWAP_PCT:-90}"

free_pct=$(memory_pressure 2>/dev/null | sed -nE 's/.*free percentage: *([0-9]+)%?.*/\1/p' | head -1)
[ -n "${free_pct:-}" ] || { echo "host-pressure-unknown: memory_pressure unreadable"; exit 0; }

read -r swap_total swap_used <<<"$(sysctl -n vm.swapusage 2>/dev/null \
  | sed -E 's/.*total = ([0-9.]+)M.*used = ([0-9.]+)M.*/\1 \2/')"
[ -n "${swap_used:-}" ] || { echo "host-pressure-unknown: vm.swapusage unreadable"; exit 0; }
swap_pct=$(python3 -c "print(int(100*$swap_used/$swap_total) if $swap_total>0 else 0)" 2>/dev/null || echo 0)

obs="free_pct=${free_pct} swap_used_pct=${swap_pct} (floor=${FREE_PCT_FLOOR}% ceil=${SWAP_USED_PCT_CEIL}%)"

# Name the biggest resident process — on 2026-07-30 that one line WAS the fix.
top=$(ps -Ao rss,pid,comm -m 2>/dev/null | sed -n '2p' | awk '{printf "%dMB pid=%s %s", $1/1024, $2, $3}')

if [ "$free_pct" -lt "$FREE_PCT_FLOOR" ] || [ "$swap_pct" -gt "$SWAP_USED_PCT_CEIL" ]; then
  echo "host-under-pressure: ${obs} largest=${top}"
  exit 1
fi
echo "ok: ${obs}"
exit 0
