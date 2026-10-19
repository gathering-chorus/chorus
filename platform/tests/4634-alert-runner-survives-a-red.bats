#!/usr/bin/env bats
# @test-type: unit — the real alert-runner over two fixture rules in a temp dir; logs, state and actions stay in the temp dir
# @domain: alerts
# #4634 — a red check must not stop the runner. Under `set -e` the old
# `result=$(...); rc=$?` exited on the first failing check, so every rule after
# it never ran. Two rules: the first is red, the second fires its action.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
  mkdir -p "$T/rules" "$T/state"
  rule() { # rule <file> <name> <exit>
    printf 'name: %s\ncheck: |\n  echo "%s-said"\n  exit %s\naction: |\n  echo "%s" >> "%s/actions"\n' "$2" "$2" "$3" "$2" "$T" > "$T/rules/$1"
  }
  rule a.yml first-red 1
  rule b.yml second-red 1
}

teardown() { rm -rf "$T"; }

run_runner() {
  env ALERT_RULES_DIR="$T/rules" ALERT_STATE_DIR="$T/state" ALERT_LOG="$T/log" ALERT_THRESHOLD=1 CHORUS_ROOT="$ROOT" \
    bash "$ROOT/proving/scripts/alert-runner.sh"
}

@test "a red rule does not stop the runner: the next rule still runs and fires" {
  run run_runner
  [ "$status" -eq 0 ] || return 1
  grep -q "FIRE first-red" "$T/log" || return 1
  grep -q "FIRE second-red" "$T/log" || return 1
  grep -q "Alert runner complete" "$T/log" || return 1
  [ "$(grep -c . "$T/actions")" -eq 2 ]
}

@test "a green rule logs OK and fires nothing" {
  printf 'name: all-good\ncheck: |\n  echo ok\n' > "$T/rules/c.yml"
  rm "$T/rules/a.yml" "$T/rules/b.yml"
  run run_runner
  [ "$status" -eq 0 ] || return 1
  grep -q "OK all-good" "$T/log" || return 1
  [ ! -f "$T/actions" ]
}
