#!/usr/bin/env bats
# @test-type: integration — nudge-health-check.sh on file:// fixtures (no live services)
# @domain: messages
# @card: #4916 · owner: borg-queen
# #4916 — nudge-health-check reads reachability directly from pulse /api/where/<role>.
# tmux list-panes and live_panes are retired (caused false alarm across all roles when run by non-jeff accounts).
# An unreadable source reports UNMEASURED, never a false alert.

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }

@test "#4916: a resolved role whose reachability is reachable PASSES without calling tmux" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-4916-reachable"
  mkdir -p "$W/where" "$W/canary"
  echo '{"data":[{"name":"wren","roleKind":"agent"}]}' > "$W/roles.json"
  printf '{"role":"wren","kind":"resolved","pane":"%%9","reachability":"reachable"}' > "$W/where/wren"
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" \
          CANARY_DIR="$W/canary" ROLES_OVERRIDE="wren" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-wren","sessions":[{"live":true}]}]}' \
          bash "$HEALTH"
  test "$status" -eq 0
  out_has "OK: wren — live run resolved and reachable (%9)"
}

@test "#4916: a resolved role whose reachability is unreachable goes RED, naming unreachable" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-4916-unreachable"
  mkdir -p "$W/where" "$W/canary"
  echo '{"data":[{"name":"silas","roleKind":"agent"}]}' > "$W/roles.json"
  printf '{"role":"silas","kind":"resolved","pane":"%%3","reachability":"unreachable"}' > "$W/where/silas"
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" \
          CANARY_DIR="$W/canary" ROLES_OVERRIDE="silas" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-silas","sessions":[{"live":true}]}]}' \
          bash "$HEALTH"
  test "$status" -ne 0
  out_has "ALERT: silas — live run on pane %3 is unreachable"
}

@test "#4916: role A unread + role B unreachable exits 1 (ALERT, not 2) proving continue" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-4916-unread-and-unreachable"
  mkdir -p "$W/where" "$W/canary"
  echo '{"data":[{"name":"kade","roleKind":"agent"},{"name":"silas","roleKind":"agent"}]}' > "$W/roles.json"
  printf '{"role":"silas","kind":"resolved","pane":"%%3","reachability":"unreachable"}' > "$W/where/silas"
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" \
          CANARY_DIR="$W/canary" ROLES_OVERRIDE="kade silas" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-silas","sessions":[{"live":true}]}]}' bash "$HEALTH"
  test "$status" -eq 1
  out_has "UNMEASURED — where/kade is unreadable"
  out_has "ALERT: silas — live run on pane %3 is unreachable"
}

@test "#4916: reachability unknown is UNMEASURED, never red" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-4916-unknown"
  mkdir -p "$W/where" "$W/canary"
  echo '{"data":[{"name":"silas","roleKind":"agent"}]}' > "$W/roles.json"
  printf '{"role":"silas","kind":"resolved","pane":"%%3","reachability":"unknown"}' > "$W/where/silas"
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" \
          CANARY_DIR="$W/canary" ROLES_OVERRIDE="silas" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-silas","sessions":[{"live":true}]}]}' \
          bash "$HEALTH"
  test "$status" -eq 2
  out_has "UNMEASURED: silas — reachability unknown (not yet delivered)"
}

@test "#4916: live_panes function and tmux list-panes calls are retired from nudge-health-check" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run grep -nE "(live_panes\(\)|tmux list-panes)" "$ROOT/platform/scripts/nudge-health-check.sh"
  [ "$status" -eq 1 ]
}
