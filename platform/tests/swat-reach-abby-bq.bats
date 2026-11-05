#!/usr/bin/env bats
# @test-type: e2e — after land: reads prod pulse's spine in Loki, no fixture
# @domain: messages
# @card: #4889 · owner: silas
# @needs: nightly — after land: the deployed principal and pulse must carry the fix; the card's AC says `check: after test`
# What Jeff sees: a nudge to abby-normal and to borg-queen arrives in their live pane.
# On 11-05 both had Presence panes that no longer existed (%12, %10), so every nudge to
# them failed six times and their senders were told "sent". After #4889 lands, the
# first nudge to each (a fresh trace) must reach nudge.surfaced with no
# nudge.surface.failed. It skips (unmeasured, never a pass) until #4889 is on main and
# each role has been sent a nudge since.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LOKI="${LOKI_URL:-http://localhost:3102}"

land_time() { git -C "$ROOT" log origin/main --first-parent -1 -E --grep='^#4889([^0-9]|$)' --format=%ct 2>/dev/null; }

# pulse's delivery events since $1 (epoch s), one JSON line each
deliveries_since() {
  curl -sf --max-time 20 -G "$LOKI/loki/api/v1/query_range" \
    --data-urlencode 'query={job="platform-chorus"} |~ "\"event\":\"nudge\\.(surfaced|surface\\.failed)\""' \
    --data-urlencode "start=${1}000000000" --data-urlencode "limit=5000" --data-urlencode "direction=forward" |
    python3 -c 'import json,sys; [print(v[1]) for s in json.load(sys.stdin)["data"]["result"] for v in s["values"]]'
}

REACH_PY="$(cat <<'PY'
import json, sys
role = sys.argv[1]
ev = [json.loads(l) for l in sys.stdin if l.strip()]
ev = [e for e in ev if e.get("to") == role]
ev.sort(key=lambda e: e.get("timestamp", ""))
if not ev:
    print(""); sys.exit()
first = ev[0].get("trace_id") or ev[0].get("id")
mine = [e for e in ev if (e.get("trace_id") or e.get("id")) == first]
failed = [e for e in mine if e["event"] == "nudge.surface.failed"]
if failed:
    print(f"red {role}: nudge.surface.failed for trace {first}: {failed[0].get('error') or failed[0].get('reason')}")
elif any(e["event"] == "nudge.surfaced" for e in mine):
    print(f"ok {role}")
else:
    print("")
PY
)"
reach() { python3 -c "$REACH_PY" "$1"; }

@test "AC (after land): a fresh nudge to abby-normal and to borg-queen surfaces in their live pane" {
  t="$(land_time)"
  [ -n "$t" ] || skip "#4889 has not landed yet"
  s="$(deliveries_since "$t")" || skip "unmeasured: Loki did not answer at $LOKI"
  for r in abby-normal borg-queen; do
    got="$(printf '%s\n' "$s" | reach "$r")"
    [ -n "$got" ] || skip "no nudge delivered or failed for $r since #4889 landed yet"
    [[ "$got" == ok* ]] || { echo "$got"; return 1; }
  done
}

ev() { printf '{"timestamp":"2026-11-05T%s-05:00","event":"%s","to":"%s","trace_id":"%s"%s}\n' "$1" "$2" "$3" "$4" "${5:-}"; }

@test "negative proof: a failed surface is red, a surfaced one is ok, another role's is not counted" {
  [[ "$( { ev 07:43:37 nudge.surface.failed abby-normal a ',"error":"can'"'"'t find pane: %12"'; } | reach abby-normal)" == "red abby-normal"* ]] || return 1
  [ "$( { ev 07:43:37 nudge.surfaced abby-normal b; } | reach abby-normal)" = "ok abby-normal" ] || return 1
  [ -z "$( { ev 07:43:37 nudge.surfaced kade c; } | reach borg-queen)" ] || return 1
}
