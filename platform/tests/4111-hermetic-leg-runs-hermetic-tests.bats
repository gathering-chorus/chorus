#!/usr/bin/env bats
# @test-type: unit — runs werk-test in a throwaway werk with a recording jest, and
#   the real jest --listTests against platform/api's config; no live service
# @domain: tests — the product domain this suite guards (#4334)
#
# #4111 — the hermetic leg must run the hermetic project, not both.
#
# platform/api's jest config declares two projects, `hermetic` and
# `integration`. Bare jest runs BOTH. Inside act there is no live stack, so on
# run 29 every *.integration.test.ts failed on a service it could not reach:
#
#     jest:platform/api   150 failed of 2355, 15 suites
#     hermetic project alone: 2025 passed, 0 failed, 21s
#
# None of those 150 were about the code. The registry-unreachable fallback made
# it worse by running the FULL package suite, so a registry outage rendered as a
# wall of red indistinguishable from a broken build.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  CFG="$ROOT/platform/api/jest.config.js"
  RUNNER_BIN="$ROOT/platform/services/werk-test/target/release/werk-test"
  JEST="$ROOT/platform/api/node_modules/.bin/jest"
}

# #4336 — every case used to grep jest.config.js or the runner's Rust source.
# Now they run things. Two helpers:
#
#   real_jest   the real jest in platform/api, --listTests only (lists files,
#               runs nothing). Absent node_modules = UNMEASURED, never a pass.
#   run_runner  the built werk-test verb against a throwaway werk: one commit
#               touching platform/api, a copy of the real jest config, and a
#               jest stub that records the argv and the RUN_INTEGRATION the
#               runner handed it. Registry, stack probes and result posts all
#               point at a closed port; CHORUS_HOME is unset, so no spine write
#               and no token. Nothing leaves $BATS_TEST_TMPDIR.

need_jest() {
  [ -x "$JEST" ] || skip "UNMEASURED — platform/api/node_modules absent in this tree, jest cannot list (#4336)"
}

# real_jest <RUN_INTEGRATION value or ""> <jest args...> → integration-file count on stdout
real_jest_count() {
  local ri="$1"; shift
  (cd "$ROOT/platform/api" && env -u RUN_INTEGRATION ${ri:+RUN_INTEGRATION=$ri} "$JEST" --listTests "$@" 2>/dev/null) \
    | grep -c 'integration\.test\.ts' || true
}

# run_runner <config-file> [probe spec] — leaves $W (the werk) and $REC (jest record)
run_runner() {
  local cfg="$1" probes="${2:-dead=http://127.0.0.1:9/}"
  [ -x "$RUNNER_BIN" ] || skip "UNMEASURED — werk-test is not built in this tree (#4336)"
  local base="$BATS_TEST_TMPDIR/werks"
  W="$base/kade-9999"; REC="$BATS_TEST_TMPDIR/jest-record.log"
  rm -rf "$base" "$REC"
  mkdir -p "$W/platform/api/src" "$W/platform/api/node_modules/.bin"
  cp "$cfg" "$W/platform/api/jest.config.js"
  cp "$ROOT/platform/api/package.json" "$W/platform/api/"
  printf '#!/bin/bash\necho "ARGV $*" >> "%s"\necho "RUN_INTEGRATION=${RUN_INTEGRATION:-}" >> "%s"\nfor a in "$@"; do [ "$a" = --json ] && echo "{\\"numFailedTests\\":0,\\"testResults\\":[]}"; done\nexit 0\n' \
    "$REC" "$REC" > "$W/platform/api/node_modules/.bin/jest"
  chmod +x "$W/platform/api/node_modules/.bin/jest"
  g() { git -C "$W" -c user.email=t@t -c user.name=t -c commit.gpgsign=false "$@"; }
  g init -q && g add -A && g commit -qm base && g update-ref refs/remotes/origin/main HEAD
  echo 'export const x = 1;' > "$W/platform/api/src/x.ts"
  g add -A && g commit -qm change
  run env -u CHORUS_HOME -u RUN_INTEGRATION TMPDIR="$BATS_TEST_TMPDIR" \
    CHORUS_WERK_BASE="$base" \
    OWL_API_TESTS=http://127.0.0.1:9/tests \
    OWL_API_TESTRESULTS_BATCH=http://127.0.0.1:9/results \
    OWL_API_TESTSUITERUNS=http://127.0.0.1:9/runs \
    WERK_STACK_PROBES="$probes" WERK_STACK_PROBE_TIMEOUT=1 \
    "$RUNNER_BIN" 9999 kade
}

# The full jest call (not the --findRelatedTests listing) the runner made.
jest_run_argv() { grep '^ARGV .*--json' "$REC" | head -1; }
jest_run_env()  { grep -A1 '^ARGV .*--json' "$REC" | grep '^RUN_INTEGRATION=' | head -1; }

@test "the jest config still declares the two projects this split depends on" {
  need_jest
  # #4336 — ask jest, not the file text: each project must select files.
  [ "$(cd "$ROOT/platform/api" && RUN_INTEGRATION=true "$JEST" --listTests --selectProjects hermetic 2>/dev/null | grep -c '\.test\.ts')" -ge 1 ]
  [ "$(real_jest_count true --selectProjects integration)" -ge 1 ]
}

@test "the hermetic project excludes the integration tier" {
  # If this stops being true the flag below buys nothing — the hermetic project
  # would itself be running needs-stack tests.
  need_jest
  [ "$(real_jest_count true --selectProjects hermetic)" -eq 0 ]
}

@test "run_jest selects the hermetic project only when the integration tier is OFF (#4139)" {
  # #4336 — run the runner. Registry unreachable (the act case #4111 was about)
  # and the stack probe pointed at a closed port: the tier is OFF, so jest must
  # get --selectProjects hermetic and must not be told RUN_INTEGRATION=true.
  run_runner "$CFG"
  [ "$status" -ne 127 ]
  [ -n "$(jest_run_argv)" ]
  [[ "$(jest_run_env)" != "RUN_INTEGRATION=true" ]] || return 1
  [[ "$(jest_run_argv)" == *"--selectProjects hermetic"* ]] || return 1
}

@test "NEGATIVE PROOF: with the tier ON the runner passes no --selectProjects (the #4111 state)" {
  # the state #4111 could not separate: stack up, RUN_INTEGRATION=true, and
  # jest still told to run hermetic only. Whenever the runner hands jest
  # RUN_INTEGRATION=true, the call must carry no --selectProjects.
  run_runner "$CFG"
  [ -n "$(jest_run_argv)" ]
  if [ "$(jest_run_env)" = "RUN_INTEGRATION=true" ]; then
    [[ "$(jest_run_argv)" != *"--selectProjects"* ]] || return 1
  fi
}

@test "NEGATIVE PROOF: the guard says no for a config without the project" {
  # A config with no `hermetic` project must never get --selectProjects
  # hermetic (a warning and an empty run: a vacuous green). Paired with the
  # config that has one, so this cannot pass merely because the flag is never
  # passed at all.
  grep -v "displayName: 'hermetic'" "$CFG" > "$BATS_TEST_TMPDIR/no-hermetic.config.js"
  run_runner "$BATS_TEST_TMPDIR/no-hermetic.config.js"
  [ -n "$(jest_run_argv)" ]
  [[ "$(jest_run_argv)" != *"--selectProjects"* ]] || return 1
  run_runner "$CFG"
  [[ "$(jest_run_argv)" == *"--selectProjects hermetic"* ]] || return 1
}

@test "by hand: the runner's own jest call lists integration files only with the tier on (skip-if-absent: needs node_modules)" {
  need_jest
  # #4336 — replay what the runner actually handed jest (its flags and its
  # RUN_INTEGRATION) against the real config, as --listTests.
  run_runner "$CFG"
  argv="$(jest_run_argv)"; env_ri="$(jest_run_env)"; env_ri="${env_ri#RUN_INTEGRATION=}"
  sel=""; [[ "$argv" == *"--selectProjects hermetic"* ]] && sel="--selectProjects hermetic"
  # tier on by hand: bare jest with the flag lists the integration files
  [ "$(real_jest_count true)" -ge 1 ]
  # the runner's call with the stack down: none
  [ "$(real_jest_count "$env_ri" $sel)" -eq 0 ]
}
