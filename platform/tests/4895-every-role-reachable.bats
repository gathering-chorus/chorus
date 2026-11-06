#!/usr/bin/env bats
# @test-type: integration — reads the running pulse and messages.db after the land
# @domain: messages
# @needs: machine — reads prod after the land (AC2 check: after); a card run leaves it to borg-prove and the nightly (#4539)
# @card: #4895 · owner: silas
# #4895 AC2 — every agent role, Abby and BQ included, is reached at its live pane: pulse
# resolves it to a pane tmux has, its Presence reads reachable, and the newest nudge to it
# in the last day was delivered. Each role runs this from its own account after the land.

PULSE="${PULSE_URL:-http://localhost:3475}"
# prod's messages.db sits in the main checkout (the werk's .git points at it), never in a werk
MAIN="$(cd "$BATS_TEST_DIRNAME/../.." && cd "$(git rev-parse --git-common-dir)/.." && pwd)"
DB="${CHORUS_MESSAGES_DB:-$MAIN/platform/pulse/messages.db}"

# verdict <role> <where-json> <newest-nudge-status>: GREEN, or RED naming what failed
verdict() {
  python3 - "$1" "$2" "$3" <<'PY'
import json, sys
role, where, last = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    w = json.loads(where)
except Exception:
    print(f"RED {role}: where unreadable"); sys.exit(1)
if w.get("kind") not in ("resolved", "agent"):
    print(f"RED {role}: pulse resolves {w.get('kind')}"); sys.exit(1)
if last != "delivered":
    print(f"RED {role}: newest nudge {last or 'none in 24h'}"); sys.exit(1)
print(f"GREEN {role}: {w.get('kind')} {w.get('pane','')} · newest nudge delivered")
PY
}

@test "every agent role resolves to a live address and its newest nudge was delivered" {
  roles=$(curl -sf --max-time 5 "http://localhost:3360/v1/roles/roles?limit=500" | python3 -c 'import json,sys; print(" ".join(r["name"] for r in json.load(sys.stdin)["data"] if r.get("roleKind")=="agent"))')
  [ -n "$roles" ] || { echo "UNMEASURED: roles door unreadable"; return 1; }
  bad=0
  for r in $roles; do
    where=$(curl -sf --max-time 5 "$PULSE/api/where/$r" || echo '')
    last=$(sqlite3 -readonly "$DB" "select delivery_status from messages where type='nudge' and \"to\"='$r' and created_at >= datetime('now','-24 hours') order by id desc limit 1" 2>/dev/null)
    run verdict "$r" "$where" "$last"; echo "$output"
    [ "$status" -eq 0 ] || bad=1
  done
  [ "$bad" -eq 0 ]
}

@test "negative proof: a dead-pane failure, an offline role or no nudge reads red" {
  run verdict abby-normal '{"kind":"resolved","pane":"%16"}' failed
  [ "$status" -eq 1 ] && [[ "$output" == *"newest nudge failed"* ]]
  run verdict borg-queen '{"kind":"dead"}' delivered
  [ "$status" -eq 1 ] && [[ "$output" == *"resolves dead"* ]]
  run verdict wren '{"kind":"resolved","pane":"%9"}' ''
  [ "$status" -eq 1 ] && [[ "$output" == *"none in 24h"* ]]
  run verdict kade '{"kind":"resolved","pane":"%1"}' delivered
  [ "$status" -eq 0 ]
}
