#!/usr/bin/env bats
# @test-type: unit
# 4057 — deep-health measures LOADED AGENTS, not log files.
#
# WHY THIS EXISTS. On 2026-09-01 health said "warning, 28" and 20 of those were
# log files belonging to LaunchAgents retired months ago (werk-sync 2759h stale,
# posture-capture 3526h, session-health 3295h). The check walked *.log on disk,
# so it was a function of the filesystem rather than of what is running. It could
# not go green — nobody can make a deleted agent write again — and it could not
# go red, because everything it found landed in a warning tier. Jeff: "grey does
# not work for me."
#
# The two tests that matter here are NEGATIVE PROOFS (#3734): each puts the
# system in the state the check exists to catch, or the state it must ignore,
# and asserts the check separates them. The old check could not distinguish
# "a live agent went silent" from "a dead agent's file is old" — that is exactly
# the two-states-it-cannot-separate shape, and it survived because no fixture
# ever asked it to tell them apart.
#
# NOTE ON STYLE. Every assertion here is written `... || return 1`. A bare
# `[[ ]]` did NOT abort the test under this bats — only the last command's
# status counted, so an earlier failing assertion passed silently. The first
# version of this file hit exactly that: test 2 reported ok against the old
# code while its own positive control was failing. Do not simplify these back
# to bare brackets.

setup() {
  SCRIPT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/deep-health.sh"
  [ -f "$SCRIPT" ] || skip "deep-health.sh not found"
  W="$BATS_TEST_TMPDIR"
  mkdir -p "$W/logs" "$W/agents" "$W/bin"
  # The world this test brings: no live nudge, no live spine, no shared state
  # file, no shared JSON artifact (#3528, #3615 — a test may not write a prod
  # surface).
  printf '#!/bin/sh\nexit 0\n' > "$W/bin/noop"; chmod +x "$W/bin/noop"
  export HEALTH_LOG_DIR="$W/logs"
  export HEALTH_PLIST_DIR="$W/agents"
  export HEALTH_AGENT_LIST="$W/loaded.txt"
  export HEALTH_JSON_OUT="$W/health.json"
  export HEALTH_STATE_FILE="$W/state.txt"
  export HEALTH_OPS_NUDGE="$W/bin/noop"
  export HEALTH_CHORUS_LOG="$W/bin/noop"
  : > "$W/loaded.txt"
}

# Write a plist declaring a StandardOutPath, the way a real agent does.
# Default fixture is a SCHEDULED agent (StartInterval), because freshness is
# only a meaningful question for something with a cadence. A persistent daemon
# is liveness-checked instead — see the KeepAlive test at the bottom.
plist_for() {
  local label="$1" logpath="$2" interval="${3:-3600}"
  cat > "$W/agents/$label.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>$label</string>
  <key>StandardOutPath</key><string>$logpath</string>
  <key>StartInterval</key><integer>$interval</integer>
</dict></plist>
EOF
}

# A KeepAlive daemon: no cadence, silence is normal, aliveness is the question.
plist_persistent() {
  local label="$1" logpath="$2"
  cat > "$W/agents/$label.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>$label</string>
  <key>StandardOutPath</key><string>$logpath</string>
  <key>KeepAlive</key><true/>
</dict></plist>
EOF
}

# Run the script and hand back only the log-freshness lines. Other checks probe
# the live box and are none of this file's business.
freshness_lines() {
  bash "$SCRIPT" 2>/dev/null | grep -E 'log-freshness|liveness:' || true
}

@test "NEGATIVE PROOF: a LOADED agent whose log is stale is a FAILURE" {
  local log="$W/logs/com.chorus.probe.log"
  echo "old output" > "$log"
  touch -t 202601010000 "$log"          # far past every threshold
  plist_for "com.chorus.probe" "$log"
  echo "com.chorus.probe" > "$W/loaded.txt"

  run freshness_lines
  [[ "$output" == *"com.chorus.probe.log"* ]] || { echo "ASSERT FAILED: [[ '$output' == *'com.chorus.probe.log'* ]]"; return 1; }
  [[ "$output" == *"stale"* ]] || { echo "ASSERT FAILED: [[ '$output' == *'stale'* ]]"; return 1; }
  # and it must be a failure, not a warning: the script exits 1 on degraded
  run bash "$SCRIPT"
  [ "$status" -eq 1 ] || { echo "ASSERT FAILED: [ '$status' -eq 1 ]"; return 1; }
  grep -q '"status":"degraded"' "$W/health.json"
  grep -q 'com.chorus.probe.log' "$W/health.json"
  # the finding must be in details (failures), never in warnings
  python3 - "$W/health.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
det=[x for x in d["details"] if "com.chorus.probe.log" in x]
warn=[x for x in d["warnings"] if "com.chorus.probe.log" in x]
assert det, "stale loaded agent not in failures: %r" % d["details"]
assert not warn, "stale loaded agent leaked into warnings: %r" % warn
PY
}

@test "NEGATIVE PROOF: an orphan log with NO loaded agent produces no finding" {
  # This is the test that stops the fix from being a relabel. Without it,
  # 20 warnings simply become 20 failures and Jeff's lamp is red forever
  # instead of grey forever — which is worse, not better.
  #
  # It carries a POSITIVE CONTROL on purpose. The first version of this test
  # asserted only the absence of the orphan, and it PASSED against the old
  # file-walking code — which never saw this fixture world at all. An absence
  # assertion alone cannot tell "correctly ignored" from "never looked", and
  # that is the same defect the card is about (#3734, the worked case). The
  # stale-but-LOADED agent below must appear in the same run, so a check that
  # looked at nothing fails here instead of passing quietly.
  local orphan="$W/logs/com.chorus.retired-months-ago.log"
  echo "last written in April" > "$orphan"
  touch -t 202604010000 "$orphan"
  plist_for "com.chorus.retired-months-ago" "$orphan"   # plist may even linger
  # ...and a LOADED agent that IS stale, as the control.
  local live="$W/logs/com.chorus.control.log"
  echo "old" > "$live"; touch -t 202601010000 "$live"
  plist_for "com.chorus.control" "$live"
  echo "com.chorus.control" > "$W/loaded.txt"           # launchd has the orphan NOT loaded

  run freshness_lines
  [[ "$output" == *"com.chorus.control.log"* ]] || { echo "ASSERT FAILED: [[ '$output' == *'com.chorus.control.log'* ]]"; return 1; }         # control: the loop ran
  [[ "$output" != *"retired-months-ago"* ]] || { echo "ASSERT FAILED: [[ '$output' != *'retired-months-ago'* ]]"; return 1; }             # the actual proof
}

@test "a LOADED agent with a fresh log produces no finding" {
  local log="$W/logs/com.chorus.fresh.log"
  echo "just wrote" > "$log"
  plist_for "com.chorus.fresh" "$log"
  echo "com.chorus.fresh" > "$W/loaded.txt"

  run freshness_lines
  [[ "$output" != *"com.chorus.fresh.log"* ]] || { echo "ASSERT FAILED: [[ '$output' != *'com.chorus.fresh.log'* ]]"; return 1; }
}

@test "a LOADED agent with no StandardOutPath is a FAILURE, not silence" {
  # An agent nobody can observe must be loud about it. The old check simply
  # never saw such an agent, because it started from files.
  cat > "$W/agents/com.chorus.blind.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>com.chorus.blind</string>
  <key>StartInterval</key><integer>3600</integer>
</dict></plist>
EOF
  echo "com.chorus.blind" > "$W/loaded.txt"

  run bash "$SCRIPT"
  grep -q 'com.chorus.blind' "$W/health.json"
  python3 - "$W/health.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
assert any("com.chorus.blind" in x for x in d["details"]), d["details"]
PY
}

@test "a LOADED agent whose log does not exist is a FAILURE" {
  plist_for "com.chorus.never-wrote" "$W/logs/com.chorus.never-wrote.log"
  echo "com.chorus.never-wrote" > "$W/loaded.txt"

  run freshness_lines
  [[ "$output" == *"never-wrote"* ]] || { echo "ASSERT FAILED: [[ '$output' == *'never-wrote'* ]]"; return 1; }
}

@test "the script never writes the live health artifact when seams are set" {
  echo "com.chorus.live" > "$W/loaded.txt"
  local log="$W/logs/com.chorus.live.log"; echo x > "$log"
  plist_for "com.chorus.live" "$log"
  run bash "$SCRIPT"
  [ -f "$W/health.json" ] || { echo "ASSERT FAILED: [ -f '$W/health.json' ]"; return 1; }
  [ -f "$W/state.txt" ] || true
}

@test "a PERSISTENT daemon is judged on liveness, not on log silence" {
  # loki, fuseki, mysql and the werk MCP daemons all sit quiet by design. Judging
  # them on mtime is how 12 healthy services looked broken the first time this
  # check started failing instead of warning. The plist says which kind they are.
  local log="$W/logs/com.chorus.daemonish.log"
  echo "started, then quiet" > "$log"
  touch -t 202601010000 "$log"          # ancient, and that is FINE for a daemon
  plist_persistent "com.chorus.daemonish" "$log"
  echo "com.chorus.daemonish" > "$W/loaded.txt"

  run bash "$SCRIPT"
  python3 - "$W/health.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
stale=[x for x in d["details"] if "daemonish" in x and "stale" in x]
assert not stale, "persistent daemon judged on log staleness: %r" % stale
PY
}
