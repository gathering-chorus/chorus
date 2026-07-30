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
NOGH=$(mktemp -d)
printf '#!/usr/bin/env bash\nexit 1\n' > "$NOGH/gh"; chmod +x "$NOGH/gh"
out=$(env -i PATH="$NOGH:$LAUNCHD_PATH" HOME="$HOME" GH_TOKEN="" bash "$ALERTS/ci-main-red.check.sh" 2>&1); rc=$?
echo "  check said: '$out' (rc=$rc)"
case "$out" in
  ok) fail "reports plain ok while blind — a red main would never surface" ;;
  *unverifiable*) pass "still says unverifiable when it genuinely cannot see" ;;
  *) fail "neither verdict nor unverifiable: '$out'" ;;
esac
rm -rf "$NOGH"

echo "=== no other check assumes a non-system binary is on PATH ==="
# Enumerate rather than assume: every external command each check invokes, checked
# for existence on the launchd PATH. A Homebrew tool here is the same latent bug.
for f in "$ALERTS"/*.check.sh; do
  [ -f "$f" ] || continue
  n=$(basename "$f")
  missing=""
  for cmd in $(grep -oE '\b(gh|jq|python3|curl|sqlite3|rg|node|npm|docker|launchctl|osascript)\b' "$f" | sort -u); do
    env -i PATH="$LAUNCHD_PATH" sh -c "command -v $cmd >/dev/null 2>&1" || missing="$missing $cmd"
  done
  # a check may legitimately resolve a tool by absolute path — only flag bare use
  for m in $missing; do
    grep -qE "(/opt/homebrew/bin/$m|/usr/local/bin/$m|command -v $m)" "$f" \
      && missing="${missing/ $m/}"
  done
  if [ -n "${missing// /}" ]; then
    fail "$n invokes${missing} which is absent on the launchd PATH and not resolved absolutely"
  else
    pass "$n: every external command it uses is reachable under launchd"
  fi
done

echo
[ "$FAIL" -eq 0 ] && { echo "PASS: checks run in the env they run in"; exit 0; }
echo "RED: at least one check only works from an interactive shell"; exit 1
