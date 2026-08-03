#!/usr/bin/env bash
# test-identity-refusal-rate-3737.sh — #3737: the check must FIRE when governed
# writes are being refused for lack of identity, and stay quiet when they're not.
# This is the alarm that would have caught the 2026-07-27→08-02 outage on day one.
set -uo pipefail
CHECK="$(cd "$(dirname "$0")" && pwd)/identity-refusal-rate.check.sh"
FAIL=0; ok(){ echo "  PASS: $1"; }; no(){ echo "  FAIL: $1"; FAIL=1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
LOG="$TMP/spine.log"
now_iso(){ python3 -c "import datetime;print(datetime.datetime.now().astimezone().strftime('%Y-%m-%dT%H:%M:%S.000%z'))"; }
plant(){ local n=$1 reason=$2; for _ in $(seq 1 "$n"); do
  printf '{"timestamp":"%s","event":"model.refused","role":"silas","reason":"%s"}\n' "$(now_iso)" "$reason" >> "$LOG"; done; }

# 1 — quiet below the floor (today's recovered state: a handful of refusals)
plant 3 identity-token-required
SPINE_LOG="$LOG" REFUSAL_WINDOW_MIN=15 REFUSAL_FLOOR=15 bash "$CHECK" >/dev/null 2>&1
[ $? -eq 0 ] && ok "quiet below the floor (3 refusals, floor 15 → no fire)" || no "fired below the floor"

# 2 — FIRES above the floor (the outage: sustained refusals)
plant 30 identity-token-required
out=$(SPINE_LOG="$LOG" REFUSAL_WINDOW_MIN=15 REFUSAL_FLOOR=15 bash "$CHECK" 2>&1); rc=$?
[ $rc -ne 0 ] && ok "FIRES above the floor (33 refusals, floor 15 → alert)" || no "did NOT fire on 33 refusals (the outage would recur silently)"
echo "$out" | grep -q "33\|identity" && ok "obs carries the count for the nudge" || no "obs missing the count: $out"

# 3 — a stale window doesn't fire on OLD refusals (only counts within the window)
OLDLOG="$TMP/old.log"
old_iso=$(python3 -c "import datetime;print((datetime.datetime.now().astimezone()-datetime.timedelta(hours=3)).strftime('%Y-%m-%dT%H:%M:%S.000%z'))")
for _ in $(seq 1 50); do printf '{"timestamp":"%s","event":"model.refused","reason":"identity-token-required"}\n' "$old_iso" >> "$OLDLOG"; done
SPINE_LOG="$OLDLOG" REFUSAL_WINDOW_MIN=15 REFUSAL_FLOOR=15 bash "$CHECK" >/dev/null 2>&1
[ $? -eq 0 ] && ok "3h-old refusals outside the window do not fire (rate, not lifetime total)" || no "counted stale refusals outside the window"

# 4 — unreadable log = UNKNOWN, does not fire (no false alarm), but says so
out=$(SPINE_LOG="$TMP/nonexistent.log" bash "$CHECK" 2>&1); rc=$?
[ $rc -eq 0 ] && echo "$out" | grep -qi "unknown\|unreadable" && ok "unreadable spine → stated unknown, no false fire" || no "unknown handling wrong: rc=$rc $out"

echo; [ $FAIL -eq 0 ] && { echo PASS; exit 0; } || { echo RED; exit 1; }
