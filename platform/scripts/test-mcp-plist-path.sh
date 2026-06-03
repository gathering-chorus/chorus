#!/usr/bin/env bash
# test-mcp-plist-path.sh — #3195 regression guard.
#
# The com.chorus.mcp daemon execs every werk verb (werk-build, chorus-bin-install,
# ...). If its EnvironmentVariables PATH lacks ~/.chorus/bin (where werk-build lives)
# or platform/scripts (where chorus-bin-install lives), every daemon-driven deploy
# ENOENTs at the first spawn — the drift that broke team-wide deploy on 2026-06-02.
#
# This asserts the VERSION-CONTROLLED source plist carries both dirs, so a redeploy
# from it can never reintroduce the gap (the live edit was the symptom; the source
# matching it is the durable fix). Pass a plist path as $1 to test a specific file
# (used to prove RED against the pre-fix plist).
set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
PLIST="${1:-${CHORUS_ROOT}/platform/launchagents/com.chorus.mcp.plist}"
# Check portable path substrings, not CHORUS_ROOT-relative absolutes: the plist
# hardcodes canonical paths (the LaunchAgent always runs canonical's wrapper), so a
# werk-vs-canonical CHORUS_ROOT must not change the expectation. The drift this guards
# was BOTH dirs absent entirely, which the substring presence catches.
REQUIRED=("/.chorus/bin" "/platform/scripts" "/.cargo/bin")

if [ ! -f "$PLIST" ]; then
  echo "not ok - MCP daemon plist not found: $PLIST"
  exit 1
fi

path_line="$(grep -A1 '<key>PATH</key>' "$PLIST" | grep '<string>' | head -1)"
fail=0
for dir in "${REQUIRED[@]}"; do
  if printf '%s' "$path_line" | grep -qF "$dir"; then
    echo "ok - com.chorus.mcp PATH contains $dir"
  else
    echo "not ok - com.chorus.mcp PATH is MISSING $dir (daemon-driven deploys will ENOENT)"
    fail=1
  fi
done

if [ "$fail" -eq 0 ]; then
  echo "PASS: com.chorus.mcp PATH carries the chorus tool dirs"
else
  echo "FAIL: com.chorus.mcp PATH incomplete — verbs the daemon execs cannot resolve"
fi
exit "$fail"
