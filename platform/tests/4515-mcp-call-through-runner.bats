#!/usr/bin/env bats
# @test-type: unit
# @domain: services
# 4515-mcp-call-through-runner.bats — #4515 AC2. What Jeff sees: the nightly
# crawls and the hourly services check keep running after the old MCP server
# is deleted, because chorus-mcp-call.sh hands each call to the runner.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
CALL="$REPO/platform/scripts/chorus-mcp-call.sh"

setup() {
  FAKE="$BATS_TEST_TMPDIR/chorus-runner"
  printf '#!/bin/sh\necho "ROLE=$ROLE"\nfor a in "$@"; do echo "ARG=$a"; done\nexit ${FAKE_EXIT:-0}\n' > "$FAKE"
  chmod +x "$FAKE"
  export CHORUS_RUNNER_BIN="$FAKE"
}

@test "a launchd job's call reaches the runner as that role, with its arguments" {
  run "$CALL" silas herald_run '{"role":"silas","sensor":"services","wait":true}'
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"ROLE=silas"* ]] || return 1
  [[ "$output" == *"ARG=call"* ]] || return 1
  [[ "$output" == *"ARG=herald_run"* ]] || return 1
  [[ "$output" == *"ARG=sensor=services"* ]] || return 1
  [[ "$output" == *"ARG=wait=true"* ]] || return 1
}

@test "NEGATIVE PROOF: a role key in the JSON never becomes an argument" {
  run "$CALL" kade herald_run '{"role":"silas","sensor":"files"}'
  [[ "$output" == *"ROLE=kade"* ]] || return 1
  [[ "$output" != *"ARG=role="* ]] || return 1
}

@test "a list becomes one argument per item, in order" {
  run "$CALL" silas athena-deploy '{"args":["scope","/r","a..b"]}'
  [[ "$output" == *$'ARG=args=scope\nARG=args=/r\nARG=args=a..b'* ]] || return 1
}

@test "NEGATIVE PROOF: the runner's refusal is the caller's failure" {
  FAKE_EXIT=1 run "$CALL" silas herald_run '{"sensor":"nope"}'
  [ "$status" -eq 1 ] || return 1
}

@test "NEGATIVE PROOF: arguments that are not JSON fail loud and run nothing" {
  run "$CALL" silas herald_run 'sensor=files'
  [ "$status" -ne 0 ] || return 1
  [[ "$output" != *"ARG=call"* ]] || return 1
}

@test "the script no longer talks to the old server" {
  ! grep -qE '3341|Mcp-Session-Id|X-Chorus-Role' "$CALL" || return 1
}
