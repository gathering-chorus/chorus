#!/usr/bin/env bash
# @domain: awareness
# @test-type: integration:prod — reads Loki for the prompt hook's request/response pairs
# @needs: machine — Loki on :3102 with the platform-chorus stream
# #4807 AC5 — no prompt loses its context. Each prompt logs context.inject.request; a response
# logs context.inject.response. A request with no response is a lost prompt: Jeff sees the
# timeout banner and the role gets no context (2026-10-30 08:00-09:00: wren 17/37, silas 13/49,
# kade 6/30). PASS: for every role, responses = requests over the window (default 1 h).
set -uo pipefail
LOKI="${LOKI_URL:-http://localhost:3102}"; WINDOW="${WINDOW:-1h}"
# #4915 round 5 — the window ends at the newest prompt at the source, not at now: at 04:00 nobody
# prompts, and "nothing in the last hour" is not a lost prompt. No prompt in 24h is UNMEASURED (2).
ANCHOR_LINE=$(python3 "$(dirname "$0")/lib/live_anchor.py" newest '{job="platform-chorus", component="context-inject"} | json | event="context.inject.request"') \
  || { echo "UNMEASURED: no context.inject.request at the source (Loki) in 24h"; exit 2; }
ANCHOR="${ANCHOR_LINE%%$'\t'*}"
# the newest request's response takes a moment: end the window a minute after it
AT=$(python3 -c "import sys; print(int(float(sys.argv[1])) + 60)" "$ANCHOR")
q="sum by (role, event) (count_over_time({job=\"platform-chorus\", component=\"context-inject\"} | json | event=~\"context.inject.(request|response)\" [$WINDOW]))"
body=$(curl -s -m 20 -G "$LOKI/loki/api/v1/query" --data-urlencode "query=$q" --data-urlencode "time=$AT") || { echo "FAIL: Loki unreachable at $LOKI"; exit 1; }
python3 - "$body" <<'EOF'
import json, sys
try:
    res = json.loads(sys.argv[1])["data"]["result"]
except Exception as e:
    print(f"FAIL: Loki answer unreadable: {e}"); sys.exit(1)
counts = {}
for r in res:
    m = r["metric"]; counts.setdefault(m.get("role", "?"), {})[m["event"].rsplit(".", 1)[1]] = int(float(r["value"][1]))
if not counts:
    print("FAIL: no context-inject events in the window — nothing measured"); sys.exit(1)
lost = 0
for role, c in sorted(counts.items()):
    req, resp = c.get("request", 0), c.get("response", 0)
    lost += max(0, req - resp)
    print(f"{role:12} requests={req} responses={resp} unanswered={max(0, req - resp)}")
print("PASS" if lost == 0 else f"FAIL: {lost} prompts got no response")
sys.exit(0 if lost == 0 else 1)
EOF
