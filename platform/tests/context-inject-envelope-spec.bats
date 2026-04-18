#!/usr/bin/env bats
# context-inject-envelope-spec.bats
#
# Runtime specification for the per-prompt context-injection envelope.
# Posts a fake UserPromptSubmit to the live chorus-hooks daemon, parses the
# response, and asserts each primitive section (Pulse, Spine, Athena) is
# actually populated with data — not just that the header string appears
# somewhere in the source.
#
# Why: an earlier version of this spec was grep-based (check the source file
# for "## Pulse"). That passed while runtime output was empty, which is
# exactly the scaffolding-without-signal pattern we're trying to stop.
#
# Prerequisites (asserted by the tests themselves):
#   - chorus-hooks daemon running on /tmp/chorus-hooks.sock
#   - Binary mtime on disk matches what the daemon was started with (no
#     stale-daemon, rebuild-on-disk gap)
#   - silas has a WIP card in pulse-latest.json (Athena section requires it)

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SOCKET="/tmp/chorus-hooks.sock"
BINARY="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hooks"

envelope() {
  # Emit a UserPromptSubmit with the given prompt/session; return the
  # stderr portion of the response (where context-synthesis lives).
  local prompt="$1" session="${2:-spec-test}"
  printf '{"hook_event_name":"UserPromptSubmit","prompt":"%s","session_id":"%s"}' "$prompt" "$session" | \
    curl -s --unix-socket "$SOCKET" -X POST -H 'Content-Type: application/json' \
      --data @- http://localhost/user-prompt-submit | \
    python3 -c "import json,sys; print(json.load(sys.stdin).get('stderr',''))"
}

@test "prereq: chorus-hooks socket exists" {
  [ -S "$SOCKET" ]
}

@test "prereq: chorus-hooks daemon running" {
  pid=$(launchctl list | awk '$3 == "com.chorus.hooks" { print $1 }')
  [ -n "$pid" ] && [ "$pid" != "-" ]
}

@test "prereq: running daemon binary is not older than source binary on disk" {
  daemon_pid=$(launchctl list | awk '$3 == "com.chorus.hooks" { print $1 }')
  [ -n "$daemon_pid" ]
  # Process start time in epoch
  daemon_start=$(ps -p "$daemon_pid" -o lstart= | xargs -I{} date -j -f '%a %b %d %T %Y' '{}' +%s)
  binary_mtime=$(stat -f %m "$BINARY")
  # Binary can be older than daemon (same build), or newer means rebuild-not-restarted
  if [ "$binary_mtime" -gt "$daemon_start" ]; then
    echo "Binary on disk ($binary_mtime) newer than daemon start ($daemon_start) — daemon running stale code" >&2
    echo "Fix: launchctl kickstart -k gui/\$(id -u)/com.chorus.hooks" >&2
    false
  fi
}

@test "runtime: envelope contains populated Pulse section" {
  out=$(envelope "spec verifying pulse section appears with health and wip data")
  [[ "$out" == *"## Pulse"* ]] || { echo "Pulse header missing in stderr"; echo "$out" >&2; false; }
  # Pulse must include at least one of: health status OR wip_cards OR role state
  has_data=false
  [[ "$out" == *"health:"* ]] && has_data=true
  [[ "$out" == *"wip_cards:"* ]] && has_data=true
  [[ "$out" == *"role "* ]] && has_data=true
  [ "$has_data" = "true" ] || { echo "Pulse header emitted but section has no data"; echo "$out" >&2; false; }
}

@test "runtime: envelope contains populated Spine section" {
  # Ensure there's at least one spine event to surface (emit one)
  "${CHORUS_ROOT}/platform/scripts/chorus-log" spec.envelope.probe silas source=bats >/dev/null 2>&1 || true
  out=$(envelope "spec verifying spine section appears with recent events")
  [[ "$out" == *"## Spine"* ]] || { echo "Spine header missing"; echo "$out" >&2; false; }
  # Must emit at least one line like "  [<ts>] <role> → <event>"
  [[ "$out" == *" → "* ]] || { echo "Spine header emitted but no events listed"; echo "$out" >&2; false; }
}

@test "runtime: envelope contains populated Athena section when role has WIP card" {
  # Read pulse snapshot — if silas has no WIP, the test setup is wrong
  wip=$(python3 -c "
import json
d = json.load(open('/tmp/pulse-latest.json'))
wip = d.get('board',{}).get('wip_cards',[])
silas_wip = [c for c in wip if str(c.get('owner','')).lower() == 'silas']
print(len(silas_wip))
" 2>/dev/null)
  if [ "${wip:-0}" -eq 0 ]; then
    skip "silas has no WIP card in pulse — Athena section is conditional and correctly absent"
  fi
  out=$(envelope "spec verifying athena section appears with domain context")
  [[ "$out" == *"## Athena"* ]] || { echo "Athena header missing while silas has WIP"; echo "$out" >&2; false; }
  # Athena must include domain name
  [[ "$out" == *"domain:"* ]] || { echo "Athena header emitted but no domain data"; echo "$out" >&2; false; }
}
