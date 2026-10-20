#!/usr/bin/env bash
# @domain: alerts — placed by chorus-crawl's domain rules (#4538)
# test-host-pressure-4651.sh — #4651: host-pressure fires on the kernel's own
# memory-pressure level reaching critical, not on swap %. On 2026-10-19 it fired
# 9 times with swap at 91-95% and the kernel at level 2 (warn) every time; the
# services all answered. Swap alone cannot separate "busy" from "cannot serve".
set -uo pipefail
CHECK="$(cd "$(dirname "$0")" && pwd)/host-pressure.check.sh"
FAIL=0; ok(){ echo "  PASS: $1"; }; no(){ echo "  FAIL: $1"; FAIL=1; }

run(){ HOST_PRESSURE_LEVEL_NOW="$1" HOST_PRESSURE_FREE_NOW="$2" HOST_PRESSURE_SWAP_NOW="$3" bash "$CHECK" 2>&1; }

# 1 — negative proof: the 10-19 alerts (swap 92-94%, level 2) do NOT fire
out=$(run 2 45 94); rc=$?
[ $rc -eq 0 ] && ok "swap 94% at level 2 (warn) stays quiet" || no "fired at level 2: $out"
out=$(run 1 45 94); rc=$?
[ $rc -eq 0 ] && ok "swap 94% at level 1 (normal) stays quiet" || no "fired at level 1: $out"

# 1b — replay: the three 10-19 alerts whose kernel level was sampled (card
# comments: 21:25 swap 95 free 36, 22:20 swap 91 free 38, 23:25 swap 92 free 45,
# all level 2). The old rule fired on each; the new one fires on none.
fires=0
for row in "2 36 95" "2 38 91" "2 45 92"; do run $row >/dev/null; [ $? -ne 0 ] && fires=$((fires+1)); done
[ $fires -eq 0 ] && ok "10-19 replay: 0 of 3 fire" || no "10-19 replay fired $fires of 3"

# 2 — FIRES when the kernel says critical, whatever swap reads
out=$(run 4 30 40); rc=$?
[ $rc -ne 0 ] && ok "level 4 (critical) fires even at swap 40%" || no "did NOT fire at critical: $out"
echo "$out" | grep -q "^host-under-pressure:" && ok "critical names the condition" || no "critical output wrong: $out"

# 3 — every observation carries the level, free and swap
for args in "2 45 94" "4 30 40"; do
  out=$(run $args)
  echo "$out" | grep -q "level=" && echo "$out" | grep -q "free_pct=" && echo "$out" | grep -q "swap_used_pct=" \
    && ok "obs carries level, free, swap ($args)" || no "obs missing a reading ($args): $out"
done
out=$(run 2 45 94)
echo "$out" | grep -q "level=2 (warn; fires at 4 critical)" && ok "level and threshold are named" || no "level or threshold not named: $out"

# 4 — unreadable level = UNKNOWN, no fire, and says so
out=$(run "" 45 94); rc=$?
[ $rc -eq 0 ] && echo "$out" | grep -q "host-pressure-unknown" && ok "unreadable level → stated unknown, no false fire" || no "unknown handling wrong: rc=$rc $out"
out=$(run abc 45 94); rc=$?
[ $rc -eq 0 ] && echo "$out" | grep -q "host-pressure-unknown" && ok "non-numeric level → stated unknown" || no "non-numeric level wrong: rc=$rc $out"

# 5 — the live box reads a real level (no seams): never unknown on macOS
out=$(bash "$CHECK" 2>&1)
echo "$out" | grep -q "level=" && ok "live read carries a level: ${out%% largest=*}" || no "live read has no level: $out"

echo; [ $FAIL -eq 0 ] && { echo PASS; exit 0; } || { echo RED; exit 1; }
