#!/usr/bin/env bash
# test-retirement-gate-negative.sh — #3677 commit-moment negative suite for the
# retirement gate (#3598): a deleted surface whose tests still reference it must
# BLOCK. The gate has run in pre-commit since #3598 without a fixture proving it
# can go red — the hollow-gate class the werk-code contract closes.
#
# HERMETIC: fixture root via CHORUS_ROOT + RETGATE_DELETED (the gate's own env
# seams); the real repo is never touched.

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GATE="$SCRIPT_DIR/retirement-gate.sh"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }
TMP=$(mktemp -d)
cleanup() { rm -rf "$TMP"; }

[ -f "$GATE" ] || { bad "retirement-gate.sh missing"; cleanup; exit 1; }

mkroot() {
  R="$TMP/root-$RANDOM"; mkdir -p "$R/platform/tests" "$R/platform/scripts"
  echo "$R"
}

# 1. NEGATIVE PROOF: deleting a script whose bats still names it BLOCKS.
R=$(mkroot)
printf '@test "runs it" {\n  bash platform/scripts/doomed-thing.sh\n}\n' > "$R/platform/tests/orphan.bats"
out=$(CHORUS_ROOT="$R" RETGATE_DELETED="platform/scripts/doomed-thing.sh" bash "$GATE" 2>&1); rc=$?
[ "$rc" -ne 0 ] && ok || bad "orphaned reference must BLOCK"
echo "$out" | grep -q 'orphan.bats' && ok || bad "refusal must name the orphaned test, got: $out"

# 2. A clean deletion (no test references it) passes.
R=$(mkroot)
printf '@test "unrelated" { true; }\n' > "$R/platform/tests/other.bats"
CHORUS_ROOT="$R" RETGATE_DELETED="platform/scripts/doomed-thing.sh" bash "$GATE" >/dev/null 2>&1 && ok || bad "clean deletion must pass"

# 3. An ABSENCE GUARD referencing the dead name is exempt (guards live on).
R=$(mkroot)
printf '# retirement-gate: absence-guard\n@test "stays dead" {\n  ! grep -r doomed-thing.sh platform/scripts\n}\n' > "$R/platform/tests/guard.bats"
CHORUS_ROOT="$R" RETGATE_DELETED="platform/scripts/doomed-thing.sh" bash "$GATE" >/dev/null 2>&1 && ok || bad "absence guard must be exempt"

# 4. Non-code deletions (docs, data) never trip the gate.
R=$(mkroot)
printf '@test "mentions" {\n  grep note platform/docs/note.md\n}\n' > "$R/platform/tests/doc.bats"
CHORUS_ROOT="$R" RETGATE_DELETED="platform/docs/note.md" bash "$GATE" >/dev/null 2>&1 && ok || bad "doc deletion must not trip the gate"

cleanup
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
