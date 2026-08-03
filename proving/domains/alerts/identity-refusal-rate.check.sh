#!/usr/bin/env bash
# identity-refusal-rate.check.sh — #3737: is the governed writer refusing writes
# for lack of identity, right now?
#
# 2026-07-27 11:00 → 08-02: the DAL went fail-closed on writes without a verified
# CHORUS_IDENTITY_TOKEN (#3687), and when mints intermittently failed, writes were
# refused — 21,764 of them over a week. Every refusal logged to the spine. ZERO
# alerts. A fail-closed writer refusing everything in silence is indistinguishable
# from a writer nobody used, so nobody looked. Jeff found it by noticing data he
# had posted did not exist.
#
# This check gives that condition a NAME and a rate. It counts model.refused with
# reason identity-token-required within a rolling window; a sustained nonzero rate
# means real writes are being lost. Exit non-zero to fire (#3571 exit-is-verdict).
set -uo pipefail

SPINE_LOG="${SPINE_LOG:-$HOME/.chorus/chorus.log}"
WINDOW_MIN="${REFUSAL_WINDOW_MIN:-15}"
FLOOR="${REFUSAL_FLOOR:-15}"
TAIL_BYTES="${REFUSAL_TAIL_BYTES:-5000000}"   # recent events live at the tail; bound the scan

# Unknown, not zero: a monitor that cannot read its own signal must SAY so, never
# report a comforting 0. It exits 0 (no false alarm) but names the unknown loudly
# — a persistent unknown is itself visible in the run log.
if [ ! -r "$SPINE_LOG" ]; then
  echo "identity-refusal-rate: UNKNOWN — spine log unreadable ($SPINE_LOG)"
  exit 0
fi

read -r count top <<<"$(tail -c "$TAIL_BYTES" "$SPINE_LOG" 2>/dev/null \
  | grep '"reason":"identity-token-required"' \
  | WINDOW_MIN="$WINDOW_MIN" python3 -c '
import sys, os, json, datetime
win = int(os.environ["WINDOW_MIN"])
cutoff = datetime.datetime.now().astimezone() - datetime.timedelta(minutes=win)
n = 0; roles = {}
for line in sys.stdin:
    try:
        d = json.loads(line)
        ts = datetime.datetime.fromisoformat(d["timestamp"])
        if ts.tzinfo is None:
            ts = ts.astimezone()
        if ts >= cutoff:
            n += 1
            r = d.get("role", "?"); roles[r] = roles.get(r, 0) + 1
    except Exception:
        continue
top = max(roles, key=roles.get) if roles else "-"
print(n, top)
' 2>/dev/null)"

count="${count:-0}"; top="${top:--}"
obs="refusals=${count} in ${WINDOW_MIN}m (floor=${FLOOR}, top-role=${top}) — identity-token-required"

if [ "$count" -gt "$FLOOR" ] 2>/dev/null; then
  echo "identity-refusal-rate: FIRING — ${obs}. Governed writes are being lost: the minter can't produce a verified token, so the DAL fail-closes. Check CSS/.oidc health and chorus-identity-token; re-mint clears a stale cache."
  exit 1
fi
echo "identity-refusal-rate: ok — ${obs}"
exit 0
