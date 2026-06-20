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
