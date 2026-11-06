#!/usr/bin/env bats
# @test-type: integration — nudge-health-check.sh on file:// fixtures (no live services)
# @domain: messages
# @card: #4895 · owner: silas
# #4895 — the nudge health check asks pulse where a role is (/api/where/<role>, the
# resolution delivery itself uses) instead of the retired ~/.chorus/sessions registry,
# which kept reporting dead panes %16/%14 on 11-05/06 after the roles had moved.

out_has()  { printf '%s' "$output" | grep -qF -- "$1"; }

@test "#4895 NEGATIVE PROOF: a live run's Presence on a pane tmux no longer has goes RED, naming the pane" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-4895-dead"
  mkdir -p "$W/where" "$W/canary"
  echo '{"data":[{"name":"abby-normal","roleKind":"agent"}]}' > "$W/roles.json"
  printf '{"role":"abby-normal","kind":"resolved","pane":"%%16"}' > "$W/where/abby-normal"
  # the 11-05 state: the Presence names %16, tmux has %17
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" \
          TMUX_PANES_OVERRIDE="%17" CANARY_DIR="$W/canary" ROLES_OVERRIDE="abby-normal" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-abby-normal","sessions":[{"live":true}]}]}' \
          bash "$HEALTH"
  test "$status" -ne 0
  out_has "ALERT: abby-normal — live run's Presence names pane %16, which tmux does NOT have"
  # the pane is live: green
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/where" \
          TMUX_PANES_OVERRIDE="%16 %17" CANARY_DIR="$W/canary" ROLES_OVERRIDE="abby-normal" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[{"aboutPrincipal":"principal-abby-normal","sessions":[{"live":true}]}]}' \
          bash "$HEALTH"
  test "$status" -eq 0
}

@test "#4895 NEGATIVE PROOF: where a role is, unreadable, is RED and never a pass" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  HEALTH="$ROOT/platform/scripts/nudge-health-check.sh"
  W="$BATS_TEST_TMPDIR/hc-4895-unread"
  mkdir -p "$W/canary"
  echo '{"data":[{"name":"kade","roleKind":"agent"}]}' > "$W/roles.json"
  run env CHORUS_ROOT="$ROOT" CHORUS_ROLES_URL="file://$W/roles.json" WHERE_URL="file://$W/no-such-dir" \
          TMUX_PANES_OVERRIDE="" CANARY_DIR="$W/canary" ROLES_OVERRIDE="kade" MESSAGES_DB=/dev/null \
          pulse_actors='{"actors":[]}' bash "$HEALTH"
  test "$status" -ne 0
  out_has "ALERT: kade — where it is could not be read"
}

@test "#4895: the health check no longer reads the retired sessions registry" {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  run grep -nE "SESSIONS_DIR|/sessions/|registered_at" "$ROOT/platform/scripts/nudge-health-check.sh"
  [ "$status" -eq 1 ]
}

