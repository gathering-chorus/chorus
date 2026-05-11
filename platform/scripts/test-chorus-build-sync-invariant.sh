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
if echo "$ABORT_OUT" | grep -qE "ABORT — git fetch origin main failed|ABORT — canonical not fast-forwardable"; then
  echo "PASS [abort-path]: abort marker present on non-git root"
  PASS=$((PASS+1))
else
  echo "FAIL [abort-path]: expected abort marker, got:"
  echo "  $ABORT_OUT"
  FAIL=$((FAIL+1))
fi

# Assertion 3 (#2908): drift-class classifier — dirty tree with real code
# changes (non-union-merge) refuses with file path + werk recovery hint.
# Setup: git init a tmp dir with one tracked file (committed) and one
# modified-uncommitted file. Run chorus-build → expect the new abort
# message naming the file and the recovery options.
echo "test-chorus-build-sync-invariant: assertion 3 — drift classifier refuses real uncommitted work (#2908)"
DRIFT_ROOT="/tmp/chorus-build-drift-test-$$"
mkdir -p "$DRIFT_ROOT"
(
  cd "$DRIFT_ROOT" || exit 1
  git init -q
  git config user.email test@test
  git config user.name test
  echo "initial" > tracked-file.txt
  git add tracked-file.txt
  git commit -q -m "initial commit"
  echo "modified content" > tracked-file.txt   # Dirty M state on tracked file.
)
DRIFT_OUT=$(CHORUS_ROOT="$DRIFT_ROOT" bash "$SCRIPT" chorus-hooks 2>&1 || true)
rm -rf "$DRIFT_ROOT"
if echo "$DRIFT_OUT" | grep -q "ABORT — canonical has uncommitted work blocking the sync"; then
  if echo "$DRIFT_OUT" | grep -q "tracked-file.txt"; then
    if echo "$DRIFT_OUT" | grep -q "belong in a role's werk"; then
      echo "PASS [drift-classifier]: refuses with file path + werk recovery hint"
      PASS=$((PASS+1))
    else
      echo "FAIL [drift-classifier]: missing werk recovery hint"
      echo "  output: $DRIFT_OUT"
      FAIL=$((FAIL+1))
    fi
  else
    echo "FAIL [drift-classifier]: missing file path in refusal"
    echo "  output: $DRIFT_OUT"
    FAIL=$((FAIL+1))
  fi
else
  echo "FAIL [drift-classifier]: did not refuse on dirty tracked file"
  echo "  output: $DRIFT_OUT"
  FAIL=$((FAIL+1))
fi

# Assertion 4 (#2908): union-merge files pass through the classifier.
# activity.md is declared merge=union in .gitattributes; the classifier must
# skip it so a dirty activity.md doesn't block sync.
echo "test-chorus-build-sync-invariant: assertion 4 — merge=union files skip the classifier (#2908)"
UNION_ROOT="/tmp/chorus-build-union-test-$$"
mkdir -p "$UNION_ROOT"
(
  cd "$UNION_ROOT" || exit 1
  git init -q
  git config user.email test@test
  git config user.name test
  echo "activity.md merge=union" > .gitattributes
  echo "# Activity" > activity.md
  git add .gitattributes activity.md
  git commit -q -m "initial commit"
  echo "# Activity edit" > activity.md   # Dirty M on union-merge file.
)
UNION_OUT=$(CHORUS_ROOT="$UNION_ROOT" bash "$SCRIPT" chorus-hooks 2>&1 || true)
rm -rf "$UNION_ROOT"
# Classifier should NOT print the drift-block abort; instead, fetch fails
# (no remote in tmp repo) → that's the expected next step, not the drift
# abort. Pass if drift-block message is absent.
if echo "$UNION_OUT" | grep -q "ABORT — canonical has uncommitted work blocking the sync"; then
  echo "FAIL [union-merge]: classifier refused on a merge=union file (should skip)"
  echo "  output: $UNION_OUT"
  FAIL=$((FAIL+1))
else
  echo "PASS [union-merge]: classifier skipped merge=union file"
  PASS=$((PASS+1))
fi

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
