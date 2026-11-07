#!/usr/bin/env bats
# @test-type: integration — nudge-health-check.sh on file:// fixtures (no live services)
# @domain: messages
# @card: #4895 · owner: silas
# #4895 — the nudge health check asks pulse where a role is (/api/where/<role>, the
# resolution delivery itself uses) instead of the retired ~/.chorus/sessions registry,
# which kept reporting dead panes %16/%14 on 11-05/06 after the roles had moved.

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }

@test "#4895 NEGATIVE PROOF: a live run on a pane whose reachability is unreachable goes RED" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-4895-dead"
  mkdir -p "$W/where" "$W/canary"
  echo '{"data":[{"name":"abby-normal","roleKind":"agent"}]}' > "$W/roles.json"
  printf '{"role":"abby-normal","kind":"resolved","pane":"%%16","reachability":"unreachable"}' > "$W/where/abby-normal"
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" \
          CANARY_DIR="$W/canary" ROLES_OVERRIDE="abby-normal" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-abby-normal","sessions":[{"live":true}]}]}' \
          bash "$HEALTH"
  test "$status" -ne 0
  out_has "ALERT: abby-normal — live run on pane %16 is unreachable"
  # when reachable: green
  printf '{"role":"abby-normal","kind":"resolved","pane":"%%16","reachability":"reachable"}' > "$W/where/abby-normal"
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" \
          CANARY_DIR="$W/canary" ROLES_OVERRIDE="abby-normal" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-abby-normal","sessions":[{"live":true}]}]}' \
          bash "$HEALTH"
  test "$status" -eq 0
}

@test "#4895 NEGATIVE PROOF: where a role is, unreadable, is UNMEASURED (exit 2) and never a pass" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-4895-unread"
  mkdir -p "$W/canary"
  echo '{"data":[{"name":"kade","roleKind":"agent"}]}' > "$W/roles.json"
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/no-such-dir" \
          CANARY_DIR="$W/canary" ROLES_OVERRIDE="kade" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[]}' bash "$HEALTH"
  test "$status" -eq 2
  out_has "UNMEASURED — where/kade is unreadable"
}

@test "#4895: the health check no longer reads the retired sessions registry" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run grep -nE "SESSIONS_DIR|/sessions/|registered_at" "$ROOT/platform/scripts/nudge-health-check.sh"
  [ "$status" -eq 1 ]
}

