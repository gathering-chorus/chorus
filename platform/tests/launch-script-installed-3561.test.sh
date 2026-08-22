#!/bin/bash
# @test-type: fitness
#
# #3561 — a LaunchAgent's ProgramArguments must point at something a deploy path
# actually writes.
#
# Found live at the land, 2026-08-22: `launchctl kickstart com.chorus.athena-make`
# failed with "Could not find service". The plist execs
# ~/.chorus/bin/athena-make-launch.sh, the deploy shipped only the BINARY, and
# nothing in the repo ever installed a launch script. Silas hand-installed it to
# unblock the land — which is precisely the state that re-breaks silently the next
# time anyone cleans ~/.chorus/bin, because a hand-placed file has no owner.
#
# CLAUDE.md documents the build/install split for BINARIES (cdhash stability, so
# TCC grants survive rebuilds). Launch scripts were never in that contract, so
# they fell through it.
#
# This guard grades the DEPENDENCY, not the installed copy: an installed file
# proves someone ran something once; a repo source proves the deploy can do it
# again.
set -u
ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
AGENTS="$ROOT/config/launchagents"

[ -d "$AGENTS" ] || {
  echo "launch-script-installed: FAIL — $AGENTS does not exist; guard cannot see what it grades" >&2
  exit 1
}

graded=0
fails=0

for plist in "$AGENTS"/*.plist; do
  [ -f "$plist" ] || continue
  launcher=$(grep -oE '/[^<]*-launch\.sh' "$plist" 2>/dev/null | head -1 || true)
  [ -n "$launcher" ] || continue
  graded=$((graded + 1))
  name=$(basename "$launcher")
  crate="${name%-launch.sh}"
  src="$ROOT/platform/services/$crate/$name"
  if [ ! -f "$src" ]; then
    echo "launch-script-installed: FAIL — $(basename "$plist") execs $name," >&2
    echo "  but no repo source exists at platform/services/$crate/$name." >&2
    echo "  A deploy cannot install what the repo does not carry; the service" >&2
    echo "  works only until someone cleans ~/.chorus/bin." >&2
    fails=$((fails + 1))
  fi
done

# A guard that graded nothing must not report PASS (#3734).
if [ "$graded" -eq 0 ]; then
  echo "launch-script-installed: FAIL — graded 0 plists; either none declare a *-launch.sh (say so deliberately) or the guard is looking in the wrong place" >&2
  exit 1
fi

[ "$fails" -eq 0 ] || exit 1
echo "launch-script-installed: PASS — $graded plist launch-script dependency/ies have a repo source"
