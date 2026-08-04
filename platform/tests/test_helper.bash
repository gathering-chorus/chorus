# #3528 — shared test-root derivation. The single source of CHORUS_ROOT for the bats
# suite. Tests live in platform/tests/; the repo root is two levels up. Deriving it
# RELATIVELY (never a hardcoded absolute path) is what makes the suite run on any
# checkout — Jeff's machine OR the CI runner. A hardcoded /Users/<name>/... is green
# locally and silent-red in CI; that "works on my machine" rot is what kept quality.yml
# red and dismissed for 10 days (RCA 2026-06-20). The hardcoded-path-guard.bats
# regression test enforces that this stays the only way roots are derived.
#
# Usage in a .bats file (top of file):  load test_helper
# then use "$CHORUS_ROOT/platform/scripts/foo" instead of the absolute path.

CHORUS_ROOT="$(cd "${BATS_TEST_DIRNAME:-$(dirname "${BASH_SOURCE[0]}")}/../.." && pwd)"
export CHORUS_ROOT

# #3615 — the membrane world. Every suite loading this helper gets its own
# spine + sessions registry by default, so a spawned shim/script can never
# write the live surfaces (~/.chorus/chorus.log, ~/.chorus/sessions). A suite
# that already sets its own seam wins (:- guards). A suite that must touch the
# live surfaces on purpose exports CHORUS_CONTEXT=prod itself — explicit,
# reviewable, never the silent default. BATS_TEST_TMPDIR isn't set at load
# time, so setup-time resolution happens via membrane_world (call it in
# setup()); the file-scope fallback below covers suites that never call it.
membrane_world() {
  local dir="${BATS_TEST_TMPDIR:-${BATS_FILE_TMPDIR:-${BATS_RUN_TMPDIR:-/tmp}}}"
  export CHORUS_LOG_FILE="${CHORUS_LOG_FILE:-$dir/membrane-spine.log}"
  export CHORUS_SESSIONS_DIR="${CHORUS_SESSIONS_DIR:-$dir/membrane-sessions}"
  mkdir -p "$CHORUS_SESSIONS_DIR"
}
# Load-time default: BATS_RUN_TMPDIR exists for the whole bats run. Suites
# with their own setup()/seams override; the point is that NO suite falls
# through to the live surfaces by accident.
if [ -n "${BATS_RUN_TMPDIR:-}" ] && [ -z "${CHORUS_CONTEXT:-}" ]; then
  export CHORUS_LOG_FILE="${CHORUS_LOG_FILE:-$BATS_RUN_TMPDIR/membrane-spine.log}"
  export CHORUS_SESSIONS_DIR="${CHORUS_SESSIONS_DIR:-$BATS_RUN_TMPDIR/membrane-sessions}"
  mkdir -p "$CHORUS_SESSIONS_DIR" 2>/dev/null || true
fi
