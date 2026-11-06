#!/usr/bin/env bats
# @test-type: e2e — after land: reads prod pulse's spine in Loki, no fixture
# @domain: messages
# @card: #4889 · owner: silas
# @needs: nightly — after land: reads the first real run-finish wake; the card's AC says `check: after test` (borg-prove re-runs it at the next land)
# What Jeff sees: a run's finish reaches its owner when it happens. After #4889 lands, the
# first real run-finish wake (pulse's terminal.* events carry wake=run-finished, written by
# platform/pulse/src/delivery-worker.ts) is announced, never suppressed, and surfaced in the
# owner's pane within 60s. It skips (unmeasured, never a pass) until #4889 is on main and a
# wake has been sent after the land.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LOKI="${LOKI_URL:-http://localhost:3102}"

land_time() { git -C "$ROOT" log origin/main --first-parent -1 -E --grep='^#4889([^0-9]|$)' --format=%ct 2>/dev/null; }

# pulse's boundary and delivery events since $1 (epoch s), one JSON line each, oldest first
spine_since() {
  curl -sf --max-time 20 -G "$LOKI/loki/api/v1/query_range" \
    --data-urlencode 'query={job="platform-chorus"} |~ "\"event\":\"(terminal.announced|terminal.suppressed|nudge.surfaced)\""' \
    --data-urlencode "start=${1}000000000" --data-urlencode "limit=5000" --data-urlencode "direction=forward" |
    python3 -c 'import json,sys; [print(v[1]) for s in json.load(sys.stdin)["data"]["result"] for v in s["values"]]' |
    python3 -c 'import json,sys
rows=[json.loads(l) for l in sys.stdin if l.strip()]
rows.sort(key=lambda r: r.get("timestamp",""))
[print(json.dumps(r)) for r in rows]'
}

# the verdict on the first run-finish wake in a spine stream (stdin): "" (none yet),
# "ok <seconds>", or "red <why>". Pure.
VERDICT_PY="$(cat <<'PY'
import json, sys
from datetime import datetime
ev = [json.loads(l) for l in sys.stdin if l.strip()]
t = lambda e: datetime.fromisoformat(e["timestamp"]).timestamp()
sup = [e for e in ev if e.get("event") == "terminal.suppressed" and e.get("wake") == "run-finished"]
ann = [e for e in ev if e.get("event") == "terminal.announced" and e.get("wake") == "run-finished"]
first = min(sup + ann, key=t, default=None)
if first is None:
    print("")
elif first["event"] == "terminal.suppressed":
    print(f"red suppressed: trace {first.get('trace_id')} reason {first.get('reason')}")
else:
    got = [e for e in ev if e.get("event") == "nudge.surfaced" and e.get("trace_id") == first.get("trace_id") and t(e) >= t(first)]
    if not got:
        print(f"red never surfaced: trace {first.get('trace_id')}")
    else:
        d = t(got[0]) - t(first)
        print(f"ok {d:.0f}" if d <= 60 else f"red surfaced after {d:.0f}s: trace {first.get('trace_id')}")
PY
)"
verdict() { python3 -c "$VERDICT_PY"; }

@test "AC (after land): the first real run-finish wake is announced and reaches its owner's pane within 60s" {
  t="$(land_time)"
  [ -n "$t" ] || skip "#4889 has not landed yet"
  s="$(spine_since "$t")" || skip "unmeasured: Loki did not answer at $LOKI"
  got="$(printf '%s\n' "$s" | verdict)"
  [ -n "$got" ] || skip "no run-finish wake sent since #4889 landed yet"
  [[ "$got" == ok* ]] || { echo "$got"; return 1; }
}

ev() { printf '{"timestamp":"2026-11-05T%s-05:00","event":"%s","trace_id":"%s"%s}\n' "$1" "$2" "$3" "${4:-}"; }

@test "negative proof: a suppressed wake, a wake never surfaced, and a late one are each red" {
  W=',"wake":"run-finished"'
  [[ "$( { ev 06:29:08 terminal.suppressed a "$W,\"reason\":\"dup-class\""; } | verdict)" == "red suppressed"* ]] || return 1
  [[ "$( { ev 06:29:08 terminal.announced b "$W"; } | verdict)" == "red never surfaced"* ]] || return 1
  [[ "$( { ev 06:29:08 terminal.announced c "$W"; ev 06:58:11 nudge.surfaced c; } | verdict)" == "red surfaced after 1743s"* ]] || return 1
  # control: announced and surfaced 2s later
  [ "$( { ev 06:29:08 terminal.announced d "$W"; ev 06:29:10 nudge.surfaced d; } | verdict)" = "ok 2" ] || return 1
  # an alert with no wake mark is not the wake this checks
  [ -z "$( { ev 06:29:08 terminal.suppressed e ',"reason":"dup-class"'; } | verdict)" ] || return 1
}
