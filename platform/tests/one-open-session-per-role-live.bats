#!/usr/bin/env bats
# @test-type: integration — reads the live sessions door (read-only)
# @domain: identity
# @needs: machine — prod after the land and one sweep
# @card: #4950 · owner: silas
#
# #4950 AC3 — in prod each agent role has exactly one open pane session. On
# 2026-11-11 every role had two (the 11-08 row stayed open after the 11-09 login).
# A bats suite, not a .test.sh, so the land's prove reads its cases (defects row 79).

setup() {
  API="${CHORUS_IDENTITY_API:-http://localhost:3360}"
  curl -sf --max-time 10 "$API/v1/identity/sessions?endedAt.absent=true&channel=pane&limit=5000" > "$BATS_TEST_TMPDIR/s.json" \
    || { echo "the sessions door at $API did not answer"; return 1; }
}

counts() {
  python3 -c '
import json,sys,collections
rows=json.load(open(sys.argv[1]))["data"]
c=collections.Counter(r.get("ownedBy","") for r in rows if r.get("sessionState")!="closed")
for k,v in sorted(c.items()): print(k, v)' "$BATS_TEST_TMPDIR/s.json"
}

@test "the door lists open pane sessions for at least 3 agent roles (not vacuous)" {
  n=$(counts | wc -l | tr -d ' ')
  [ "$n" -ge 3 ] || { counts; false; }
}

@test "each agent role has exactly one open pane session" {
  over=$(counts | awk '$2 != 1')
  [ -z "$over" ] || { echo "more than one open pane session:"; echo "$over"; false; }
}
