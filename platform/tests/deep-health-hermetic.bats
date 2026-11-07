#!/usr/bin/env bats
# @test-type: unit — hermetic. Brings its own world (#3528): every probe points
# @domain: monitors — the product domain this suite guards (#4334)
# at a closed loopback port, logs/plists/state/nudge/spine are all seams into
# $BATS_TEST_TMPDIR, HOME is the tmpdir. No live host, no ~/.chorus.
#
# #4138 — "can we deal with the deep health tests!" (Jeff 2026-09-10 20:16).
# Seven files and 43 tests ran the real script against the live box, so a
# Bedroom reboot read as 15 nightly reds with no code change behind them.
# This file is the one proof that survives: deep-health ALWAYS prints a
# summary, and when it cannot, it says so in its own output. The scheduled
# run (com.chorus.deep-health, every 5 min) is the monitor; the nightly only
# proves the monitor cannot go silent.
load test_helper

DEEP_HEALTH="$CHORUS_ROOT/platform/scripts/deep-health.sh"

setup() {
  W="$BATS_TEST_TMPDIR"
  mkdir -p "$W/logs" "$W/agents" "$W/bin" "$W/home"
  printf '#!/bin/sh\nexit 0\n' > "$W/bin/noop"; chmod +x "$W/bin/noop"
  # a nudge stub that records it was called, so "alerts ops" is measured
  printf '#!/bin/sh\necho "$*" >> "%s/nudged.txt"\n' "$W" > "$W/bin/nudge"; chmod +x "$W/bin/nudge"
  export HOME="$W/home"
  export HEALTH_LOG_DIR="$W/logs"
  export HEALTH_LOG_FILE="$W/logs/deep-health.log"
  export HEALTH_PLIST_DIR="$W/agents"
  export HEALTH_AGENT_LIST="$W/loaded.txt"; : > "$W/loaded.txt"
  export HEALTH_JSON_OUT="$W/health.json"
  export HEALTH_STATE_FILE="$W/state.txt"
  export HEALTH_OPS_NUDGE="$W/bin/nudge"
  export HEALTH_CHORUS_LOG="$W/bin/noop"
  export HEALTH_BOOT_AUDIT="$W/bin/noop"
  # #4141 — the load reclassify step (#3753) turns refused probes into warnings
  # when the box is busy; a test brings its own load, or it reads the box.
  export WERK_TEST_LOAD_STUB=0.1
  # three probes, all refused: nothing listens on these loopback ports
  # the Gathering app and Caddy checks refuse too (#4138 reopen): no test reads the live :3002/:3000
  export HEALTH_GATHERING_URL=http://127.0.0.1:39494/ HEALTH_CADDY_URL=http://127.0.0.1:39495/
  export HEALTH_ENDPOINTS_FILE="$W/endpoints.txt"
  printf '%s\n' \
    "http://127.0.0.1:39491/health|fixture-a|refused a" \
    "http://127.0.0.1:39492/health|fixture-b|refused b" \
    "http://127.0.0.1:39493/health|fixture-c|refused c" > "$W/endpoints.txt"
}

@test "NEGATIVE PROOF: the unguarded curl idiom dies with no output on a refused probe; the guarded one reaches the summary" {
  # The exact line class that killed the 09-10 run, side by side. Without this
  # the full-run test below stays green against a script that never probes.
  cat > "$W/unguarded.sh" <<'UNG'
#!/usr/bin/env bash
set -euo pipefail
probe_code=$(curl -s --max-time 2 -o /dev/null -w "%{http_code}" "http://127.0.0.1:39491/health" 2>/dev/null); probe_exit=$?
echo "SUMMARY reached (exit=$probe_exit code=$probe_code)"
UNG
  cat > "$W/guarded.sh" <<'GRD'
#!/usr/bin/env bash
set -euo pipefail
probe_exit=0; probe_code=$(curl -s --max-time 2 -o /dev/null -w "%{http_code}" "http://127.0.0.1:39491/health" 2>/dev/null) || probe_exit=$?
echo "SUMMARY reached (exit=$probe_exit code=$probe_code)"
GRD
  run bash "$W/unguarded.sh"
  [ "$status" -eq 7 ] || return 1
  [ -z "$output" ] || return 1
  run bash "$W/guarded.sh"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == "SUMMARY reached (exit=7 code=000)" ]] || return 1
}

# #4446 — a run that FINDS problems is a successful run: it exits 0 and its
# findings are the record (health.json + ops.health.deep_check_failed). Only a
# run that dies is a failed run (service.failed), the negative proof below.
@test "every probe refused: the script prints a summary, names each DOWN endpoint, exits 0 with the findings recorded" {
  run bash "$DEEP_HEALTH"
  [ "$status" -eq 0 ] || return 1
  [ -n "$output" ] || return 1
  [[ "$output" == *"deep-health: "*"failure(s)"* ]] || return 1
  for n in fixture-a fixture-b fixture-c; do
    [[ "$output" == *"$n"* ]] || { echo "missing $n in: $output"; return 1; }
  done
  [[ "$output" != *"DIED before summary"* ]] || return 1
  [ -f "$W/health.json" ] || return 1
  grep -q '"status":"degraded"' "$W/health.json" || return 1
}

@test "the summary line lands in the log the scheduled run writes (the monitor's own trail)" {
  run bash "$DEEP_HEALTH"
  grep -q "deep-health: .*failure(s)" "$W/logs/deep-health.log" || return 1
}

@test "NEGATIVE PROOF: a death mid-run is one line naming it, exit nonzero, and ops is nudged — never silence" {
  HEALTH_FAULT_INJECT=1 run bash "$DEEP_HEALTH"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"deep-health: DIED before summary (exit 1 near line "* ]] || return 1
  [[ "$output" != *"failure(s)"* ]] || return 1
  grep -q "DIED before summary" "$W/nudged.txt" || return 1
}

@test "a boot-herd finding still reaches the summary (the 09-04 class stays covered)" {
  cat > "$W/bin/audit" <<'STUB'
#!/usr/bin/env bash
echo "boot-herd: com.chorus.fixture RunAtLoad=true with interval=3600 (/dev/null)"
exit 1
STUB
  chmod +x "$W/bin/audit"
  HEALTH_BOOT_AUDIT="$W/bin/audit" run bash "$DEEP_HEALTH"
  [ -n "$output" ] || return 1
  [[ "$output" == *"failure(s)"* ]] || return 1
  # the herd finding is a WARNING tier row: it lands in the JSON the pulse reads
  grep -q '"boot-herd: 1 interval LaunchAgent' "$W/health.json" || { cat "$W/health.json"; return 1; }
}

# #4138 reopen (Jeff "ok" 2026-10-12 14:58) — the 10-12 runs died with exit 7
# near line 349: the Gathering-app and Caddy checks were bare $(curl) under
# set -e, outside the guarded probe loop, and hard-wired to the live ports, so
# this file never saw them refuse. Both now take a URL seam.
@test "Gathering app and Caddy refused: each is named DOWN in the summary, the run never dies" {
  run bash "$DEEP_HEALTH"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" != *"DIED before summary"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"gathering-app: 127.0.0.1:39494 returned 000"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"caddy-edge: 127.0.0.1:39495 returned 000"* ]] || { echo "$output"; return 1; }
}

# Every command substitution that runs curl must carry its own || — under
# set -e a refused port is otherwise the death of the whole run.
# A statement continued with a trailing backslash is read as one line; inside
# or after the substitution, any || keeps a refused port from ending the run.
unguarded_curls() {
  awk '{ if (buf == "") start = NR; buf = buf $0 }
       /\\$/ { sub(/\\$/, "", buf); next }
       { if (buf ~ /=\$\((curl|ssh) / && buf !~ /\|\|/) print start ":" buf; buf = "" }' "$1"
}

@test "no curl or ssh assignment in deep-health is left without its own || guard" {
  found=$(unguarded_curls "$DEEP_HEALTH")
  [ -z "$found" ] || { echo "unguarded: $found"; return 1; }
}

@test "NEGATIVE PROOF: the guard check names an unguarded curl assignment" {
  printf 'ok=$(curl -s http://x/) || ok=000\nalso=$(curl -s \\\n  http://x/ || echo 000)\nbad=$(curl -s http://x/ 2>/dev/null)\n' > "$W/sample.sh"
  found=$(unguarded_curls "$W/sample.sh")
  [[ "$found" == "4:bad="* ]] || { echo "got: $found"; return 1; }
  [[ "$found" != *"ok="* && "$found" != *"also="* ]] || return 1
}

@test "NEGATIVE PROOF: the guard check names an unguarded ssh assignment (#4692)" {
  printf 'ok=$(ssh h "x") && rc=0 || rc=$?\nbad=$(ssh h "curl x" 2>/dev/null)\n' > "$W/sample.sh"
  found=$(unguarded_curls "$W/sample.sh")
  [[ "$found" == "2:bad="* ]] || { echo "got: $found"; return 1; }
}

@test "Bedroom's Loki probe timing out (rc 28) is a warning in the summary, never a death (#4692)" {
  printf '#!/bin/sh\nexit 28\n' > "$W/bin/ssh"; chmod +x "$W/bin/ssh"
  PATH="$W/bin:$PATH" run bash "$DEEP_HEALTH"
  [[ "$output" != *"DIED before summary"* ]] || { echo "$output" | tail -5; return 1; }
  grep -q 'loki-bedroom-slow' "$HEALTH_JSON_OUT" || { cat "$HEALTH_JSON_OUT"; return 1; }
}

@test "deep-health syntax is valid bash" {
  bash -n "$DEEP_HEALTH"
}
