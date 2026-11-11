#!/usr/bin/env bats
# @test-type: integration — reads the live model door and pulse's message store (read-only)
# @domain: alerts
# @needs: machine — prod after the land
# @card: #4957 · owner: silas
#
# #4957 AC4 — the three loudest alert sources (nightly RED now, crawler-out-of-sync,
# deep-health) write AlertCase rows and no longer nudge. Measured 11-11 04:50–09:50:
# 116 alerts typed into role panes, these three 64 of them.
# The window is the last AC4_WINDOW_SECS (default one hour). NEGATIVE PROOF: run it
# before the land and the old nudges are counted, so the check goes red.
#
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert, 2026-09-16).

setup() {
  DOOR="${CHORUS_MODEL_DOOR:-http://localhost:3360}"
  LOKI="${LOKI_URL:-http://localhost:3102}"
  WINDOW="${AC4_WINDOW_SECS:-3600}"
}

@test "the AlertCase route answers with a listing (the queue exists)" {
  run curl -sf --max-time 10 "$DOOR/v1/alerts/alertcases?limit=5"
  [ "$status" -eq 0 ] || { echo "the alertcases route did not answer"; false; }
  printf '%s' "$output" | python3 -c 'import json,sys; assert isinstance(json.load(sys.stdin)["data"], list)'
}

@test "no nudge in the window carries the three loudest alerts (Loki, one bounded count)" {
  # one metric query over the spine's nudge.emitted lines, never a scan of pulse's store
  # (11-11 10:24: a newest-first read of 23k nudges stalled pulse)
  count() {
    curl -sfG --max-time 20 "$LOKI/loki/api/v1/query" --data-urlencode "query=sum(count_over_time({job=\"platform-chorus\"} |= \"\\\"event\\\":\\\"nudge.emitted\\\"\" $1 [${WINDOW}s]))" \
      | python3 -c 'import json,sys; r=json.load(sys.stdin)["data"]["result"]; print(int(float(r[0]["value"][1])) if r else 0)'
  }
  all=$(count "") || { echo "Loki did not answer"; false; }
  loud=$(count '|~ "content=(nightly RED now|crawler-out-of-sync:|deep-health:)"') || { echo "Loki did not answer"; false; }
  echo "$all nudges in the last ${WINDOW}s, $loud from the three loudest sources"
  [ "$all" -gt 0 ] || { echo "no nudges at all in the window: the count is not reading the spine"; false; }
  [ "$loud" -eq 0 ]
}
