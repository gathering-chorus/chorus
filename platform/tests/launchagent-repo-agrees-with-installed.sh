#!/usr/bin/env bash
# #4085 — does the repo say the same thing as the machine about what runs when?
#
# Kade re-installed two nightly slots Jeff had asked me to remove, twice,
# because platform/scripts/com.chorus.nightly-suites.plist still declares 06:00
# and 13:30 while the installed unit correctly runs 03:00 only. He was fixing
# drift and undoing a fix in the same motion, and nothing could tell him which
# side was right.
#
# PRIOR ART, deliberately not duplicated: launchagent-reachability.bats (#3734)
# asks "is this script reachable?" — a different question. Its header already
# describes today's failure: "the repo's plist inventory is itself incomplete —
# its own missing denominator", naming com.chorus.nightly-suites.plist as
# installed-and-running yet absent from the declared set. It was written down
# and left open; this closes the agreement half.
#
# Two findings, and the second is why the first survived:
#
#   1. the repo copy was stale
#   2. plists live in FOUR repo directories — config/launchagents,
#      platform/launchagents, platform/launchd, platform/scripts — so "the repo
#      copy" is not even a single answer to look up
#
# Three distinguishable states, because the whole family of defects this week
# has been checks that collapse them into one:
#
#   AGREE      exactly one repo copy, and its schedule matches the installed one
#   DIVERGES   a repo copy exists and differs — a peer will sync the wrong way
#   AMBIGUOUS  more than one repo copy — there is no single truth to sync from
#   UNTRACKED  installed, in no repo file at all (#3734's missing denominator)
#
# Exits non-zero on DIVERGES or AMBIGUOUS. UNTRACKED is reported, not failed:
# some units are legitimately machine-local, and failing them would train
# everyone to ignore the check — which is how we got here.
#
# Host-specific by nature, per #3734's reasoning: it means nothing where
# nothing is installed, so it skips there rather than reporting a false clean.

set -uo pipefail

ROOT="${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}"
INSTALLED_DIR="${LAUNCHAGENT_DIR:-$HOME/Library/LaunchAgents}"
REPO_DIRS=(config/launchagents platform/launchagents platform/launchd platform/scripts)

if ! ls "$INSTALLED_DIR"/com.chorus.*.plist >/dev/null 2>&1; then
  echo "SKIP: no com.chorus units installed under $INSTALLED_DIR — the question is host-specific (#3734)."
  exit 0
fi

# The comparable content is the SCHEDULE, not the whole file: paths and env
# legitimately differ between a repo template and an installed unit, and a
# whole-file diff would cry wolf until someone switched the check off.
schedule_of() {
  {
    /usr/libexec/PlistBuddy -c "Print :StartCalendarInterval" "$1" 2>/dev/null
    /usr/libexec/PlistBuddy -c "Print :StartInterval" "$1" 2>/dev/null
  } | tr -d ' \n'
}

agree=0; diverge=0; ambiguous=0; untracked=0

for inst in "$INSTALLED_DIR"/com.chorus.*.plist; do
  [ -f "$inst" ] || continue
  label="$(basename "$inst")"; unit="${label%.plist}"

  copies=()
  for d in "${REPO_DIRS[@]}"; do
    [ -f "$ROOT/$d/$label" ] && copies+=("$ROOT/$d/$label")
  done

  case "${#copies[@]}" in
    0)
      printf '  UNTRACKED  %s (installed, in no repo file)\n' "$unit"
      untracked=$((untracked + 1))
      ;;
    1)
      if [ "$(schedule_of "$inst")" = "$(schedule_of "${copies[0]}")" ]; then
        agree=$((agree + 1))
      else
        printf '  DIVERGES   %s\n' "$unit"
        printf '               repo      %s = %s\n' "${copies[0]#"$ROOT/"}" "$(schedule_of "${copies[0]}")"
        printf '               installed %s\n' "$(schedule_of "$inst")"
        diverge=$((diverge + 1))
      fi
      ;;
    *)
      printf '  AMBIGUOUS  %s — %d repo copies, no single source to sync from\n' "$unit" "${#copies[@]}"
      for c in "${copies[@]}"; do printf '               %s = %s\n' "${c#"$ROOT/"}" "$(schedule_of "$c")"; done
      ambiguous=$((ambiguous + 1))
      ;;
  esac
done

echo
echo "agree=$agree diverges=$diverge ambiguous=$ambiguous untracked=$untracked"

if [ "$diverge" -gt 0 ] || [ "$ambiguous" -gt 0 ]; then
  echo "FAIL: the repo and the machine disagree about what runs when." >&2
  echo "Someone syncing from the repo will silently undo a correct installed schedule." >&2
  exit 1
fi
echo "PASS: every installed com.chorus unit has exactly one repo copy, and the schedules match."
