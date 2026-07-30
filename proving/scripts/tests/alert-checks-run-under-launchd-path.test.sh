#!/usr/bin/env bash
# #3713 — an alert check must work in the environment it RUNS in, not only in
# the one I test it from.
#
# The failure this pins: #3709 taught ci-main-red to fall back to `gh auth token`
# when GH_TOKEN is unset. I proved it from my interactive shell, where gh is on
# PATH, and it correctly reported main red. Under launchd it is dead: gh lives at
# /opt/homebrew/bin/gh, com.chorus.alert-runner.plist sets no PATH, so the runner
# inherits /usr/bin:/bin:/usr/sbin:/sbin where gh does not exist. The fallback
# fails silently behind `2>/dev/null || true` and the check reports
# unverifiable:GH_TOKEN-absent — the exact string the 00:00 nudge carried hours
# after #3709 landed.
#
# My #3709 test asserted `grep -q "gh auth token"` — that the STRING was in the
# file. That proves I typed it, nothing more. This test runs the thing.
set -uo pipefail
ALERTS="${1:-$(cd "$(dirname "$0")/../../domains/alerts" && pwd)}"
# The PATH launchd actually hands a LaunchAgent with no EnvironmentVariables.
LAUNCHD_PATH="/usr/bin:/bin:/usr/sbin:/sbin"
FAIL=0
pass(){ echo "  PASS: $1"; }
fail(){ echo "  FAIL: $1"; FAIL=1; }

echo "=== ci-main-red reports a real verdict under the launchd PATH ==="
out=$(env -i PATH="$LAUNCHD_PATH" HOME="$HOME" bash "$ALERTS/ci-main-red.check.sh" 2>&1); rc=$?
echo "  check said: '$out' (rc=$rc)"
case "$out" in
  *unverifiable*)
    fail "blind under the launchd PATH — this is the shipped-but-dead state (#3709)" ;;
  red:*|ok)
    pass "reports a real verdict (${out%%:*}) under the launchd PATH" ;;
  *)
    fail "unrecognised output under the launchd PATH: '$out'" ;;
esac

echo "=== the #3519 contract still holds: genuinely blind is still unverifiable ==="
# No env token AND a gh that cannot answer ⇒ unverifiable is CORRECT, not a false ok.
NOGH=$(mktemp -d); trap 'rm -rf "$NOGH"' EXIT   # #3713 review (Kade): trap, do not leak on early exit
printf '#!/usr/bin/env bash\nexit 1\n' > "$NOGH/gh"; chmod +x "$NOGH/gh"
out=$(env -i PATH="$NOGH:$LAUNCHD_PATH" HOME="$HOME" GH_TOKEN="" bash "$ALERTS/ci-main-red.check.sh" 2>&1); rc=$?
echo "  check said: '$out' (rc=$rc)"
case "$out" in
  ok) fail "reports plain ok while blind — a red main would never surface" ;;
  *unverifiable*) pass "still says unverifiable when it genuinely cannot see" ;;
  *) fail "neither verdict nor unverifiable: '$out'" ;;
esac

echo "=== no check hits a missing binary under the launchd PATH ==="
# Two earlier attempts at this were both worse than running the thing:
#   1. a HARDCODED binary list — could not see a tool nobody added to it (Kade's
#      review nit), i.e. a check blind to what it claims to check;
#   2. deriving command names by PARSING the script — picked up python identifiers
#      inside heredocs (import, print, try, d, msg) and cried wolf on all three
#      files. False positives are their own dishonesty.
# So: RUN each check under the launchd PATH and look for the shell telling us a
# command was missing. No list to maintain, no parser to fool. A check may
# legitimately fail for its own reasons here — we assert only that nothing it
# invokes was unresolvable.
for f in "$ALERTS"/*.check.sh; do
  [ -f "$f" ] || continue
  n=$(basename "$f")
  err=$(env -i PATH="$LAUNCHD_PATH" HOME="$HOME" bash "$f" 2>&1 >/dev/null)
  if printf '%s' "$err" | grep -qE "command not found|: not found|No such file or directory"; then
    # name WHAT was missing — a failure that does not say which binary is the
    # same undiagnosable alarm this whole card sequence has been about.
    detail=$(printf '%s' "$err" | grep -E "command not found|: not found" | head -2 | tr '\n' '; ')
    fail "$n hits a missing binary under launchd: ${detail:-$(printf '%s' "$err" | head -1)}"
  else
    pass "$n: no missing-binary error under the launchd PATH"
  fi
done

echo
[ "$FAIL" -eq 0 ] && { echo "PASS: checks run in the env they run in"; exit 0; }
echo "RED: at least one check only works from an interactive shell"; exit 1
