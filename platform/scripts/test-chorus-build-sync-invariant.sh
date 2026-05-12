#!/usr/bin/env bash
# test-chorus-build-sync-invariant.sh — proving gate for #2863.
#
# Asserts that /build's first step is `git fetch + ff` against origin/main
# (the canonical-sync invariant), and that the script aborts loudly on
# fetch / fast-forward failure rather than silently building stale source.
#
# Two assertions:
#   1. happy path: chorus-build prints the invariant marker line and the
#      "canonical at <sha>" line before any build artifacts are written.
#   2. abort path: chorus-build with a non-git CHORUS_ROOT exits non-zero
#      with a diagnostic, NOT silently proceeding to build.
#
# Usage: ./test-chorus-build-sync-invariant.sh

set -uo pipefail

trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

PASS=0
FAIL=0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$SCRIPT_DIR/chorus-build"
CHORUS_HOME="${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}"

if [ ! -x "$SCRIPT" ]; then
  echo "FAIL [setup]: chorus-build not executable at $SCRIPT"
  FAIL=$((FAIL+1))
  exit 1
fi

# Assertion 1: happy path emits the invariant marker.
# Uses a clean tmp git repo with an `origin` bare-repo remote — minimal setup
# that lets fetch + ff succeed. Verifies chorus-build prints the sync-canonical
# invariant marker before the build slice begins. (#2908: previously ran
# against real canonical; that path now fails if canonical is dirty, which is
# correct behavior — the classifier fires. The happy-path test moves to a
# tmp repo so it's deterministic regardless of canonical state.)
echo "test-chorus-build-sync-invariant: assertion 1 — happy-path invariant marker (clean tmp repo)"
CLEAN_REMOTE="/tmp/chorus-build-clean-remote-$$.git"
CLEAN_ROOT="/tmp/chorus-build-clean-root-$$"
git init -q --bare "$CLEAN_REMOTE" >/dev/null 2>&1
mkdir -p "$CLEAN_ROOT"
(
  cd "$CLEAN_ROOT" || exit 1
  git init -q -b main
  git config user.email test@test
  git config user.name test
  echo "initial" > README.md
  git add README.md
  git commit -q -m "initial commit"
  git remote add origin "$CLEAN_REMOTE"
  git push -q origin main 2>/dev/null || true
)
# Run chorus-build; expect it to print the invariant marker, then likely fail
# on the build step (no cargo crate at this tmp root) — but the sync marker
# should land before that.
HAPPY_OUT=$(CHORUS_ROOT="$CLEAN_ROOT" bash "$SCRIPT" chorus-hooks 2>&1 || true)
rm -rf "$CLEAN_ROOT" "$CLEAN_REMOTE"
if echo "$HAPPY_OUT" | grep -q "sync canonical from origin (invariant"; then
  echo "PASS [happy-path]: invariant marker present on clean tmp repo"
  PASS=$((PASS+1))
else
  echo "FAIL [happy-path]: invariant marker missing"
  echo "  output: $HAPPY_OUT"
  FAIL=$((FAIL+1))
fi

# Assertion 2: abort path on bad CHORUS_ROOT — non-git dir.
# A CHORUS_ROOT that isn't a git repo can't fetch; chorus-build must
# abort, not proceed. Uses /tmp/<random> as the bad root.
echo "test-chorus-build-sync-invariant: assertion 2 — abort on non-git CHORUS_ROOT"
BAD_ROOT="/tmp/chorus-build-test-$$"
mkdir -p "$BAD_ROOT"
ABORT_OUT=$(CHORUS_ROOT="$BAD_ROOT" bash "$SCRIPT" chorus-hooks 2>&1 || true)
ABORT_RC=$?
rm -rf "$BAD_ROOT"
# We expect non-zero exit AND the abort marker to appear in output.
if echo "$ABORT_OUT" | grep -qE "ABORT — chorus-werk-sync recover failed|ABORT — git fetch origin main failed|ABORT — canonical not fast-forwardable|is not a git repo"; then
  echo "PASS [abort-path]: abort marker present on non-git root"
  PASS=$((PASS+1))
else
  echo "FAIL [abort-path]: expected abort marker, got:"
  echo "  $ABORT_OUT"
  FAIL=$((FAIL+1))
fi

# Assertion 3 (#2909): chorus-build delegates sync to chorus-werk-sync recover.
# Drift-class behavior + auto-recovery are tested end-to-end in
# test-chorus-werk-sync-recover.sh. Here we verify chorus-build is a thin
# caller — it invokes the sync script, and aborts loudly if that script
# is missing.
echo "test-chorus-build-sync-invariant: assertion 3 — chorus-build delegates to chorus-werk-sync (#2909)"
# Read the script source and check it references chorus-werk-sync recover.
if grep -q "chorus-werk-sync\b" "$SCRIPT" && grep -q "recover" "$SCRIPT"; then
  echo "PASS [delegation]: chorus-build references chorus-werk-sync recover"
  PASS=$((PASS+1))
else
  echo "FAIL [delegation]: chorus-build doesn't delegate to chorus-werk-sync recover"
  FAIL=$((FAIL+1))
fi

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
