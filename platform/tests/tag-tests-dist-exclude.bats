#!/usr/bin/env bats
# @test-type: unit
# The registrar's exclude regex was `/dist/`, which matched only a directory
# named exactly "dist". platform/pulse carries dist.prev/, dist.prev-3130/ and
# dist.prev-l2/ — stale build output whose compiled .js copies registered as
# 338 tests that nothing runs and nothing can run. They were a third of the
# nightly's "registered test(s) never ran" gap, and the gap read as our
# coverage rotting rather than as the ledger counting build artifacts.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SCRIPT="$ROOT/platform/scripts/tag-tests-domain.py"
}

excl_matches() { # $1 = path -> prints True/False
  python3 - "$SCRIPT" "$1" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
m = re.search(r"excl = re\.compile\(r'([^']+)'\)", src)
assert m, "exclude regex not found — the registrar changed shape"
print(bool(re.compile(m.group(1)).search(sys.argv[2])))
PY
}

@test "a sibling build directory is excluded" {
  [ "$(excl_matches 'platform/pulse/dist.prev/service.test.js')" = "True" ]
  [ "$(excl_matches 'platform/pulse/dist.prev-3130/store.test.js')" = "True" ]
  [ "$(excl_matches 'platform/pulse/dist.prev-l2/service.test.js')" = "True" ]
}

@test "the plain dist directory is still excluded" {
  [ "$(excl_matches 'platform/pulse/dist/service.test.js')" = "True" ]
}

@test "NEGATIVE PROOF: a real source test is NOT excluded" {
  # Without this the exclude could match everything and still look correct.
  [ "$(excl_matches 'platform/api/tests/owl-proxy-3644.test.ts')" = "False" ]
  [ "$(excl_matches 'platform/tests/3838-roles-model.bats')" = "False" ]
}

@test "NEGATIVE PROOF: a directory that merely starts with dist is NOT excluded" {
  # `/dist[^/]*/` would have swallowed this; the pattern is deliberately
  # narrower than that.
  [ "$(excl_matches 'platform/district/tests/a.test.ts')" = "False" ]
}
