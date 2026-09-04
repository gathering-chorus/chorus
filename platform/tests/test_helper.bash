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

# --- throwaway-graph naming (2026-09-04) ---------------------------------
# A suite that writes to the shared store needs a graph name that is UNIQUE per
# run (two pipelines run the same file at once and tore each other's graph down)
# and IDENTICAL across every process of that run.
#
# `$$` satisfies the first and breaks the second: bats runs setup_file,
# each @test, and teardown_file in DIFFERENT processes, so the file body is
# re-sourced and $$ differs every time. teardown_file then drops a graph that
# was never created and every per-test graph is left behind — 77 of them in the
# live store on 2026-09-04, which reddened two suites the next night.
#
# BATS_RUN_TMPDIR is created once per bats run and exported to every process in
# it, so its basename is the run identity. Non-alphanumerics are stripped: the
# result goes into an IRI.
test_graph_name() {  # $1 = suite tag, e.g. 3540
  local run_id="${BATS_RUN_TMPDIR:-}"
  run_id="${run_id##*/}"
  run_id="$(printf '%s' "$run_id" | tr -c '[:alnum:]' '-')"
  [ -n "$run_id" ] || run_id="norun-$$"
  printf 'urn:chorus:ontology-test-bats-%s-%s' "$1" "$run_id"
}
