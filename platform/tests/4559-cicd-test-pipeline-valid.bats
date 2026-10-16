#!/usr/bin/env bats
# @test-type: fitness — a pipeline file dagu can load, no live service
# @domain: pipelines — werk v2's test pipeline lands ahead of its caller (#4559)
# What Jeff sees: platform/pipelines/cicd-test.yaml is on main and dagu loads
# it, so #4540's run finds its test pipeline by name instead of failing at test.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  WF="$ROOT/platform/pipelines/cicd-test.yaml"
  command -v dagu >/dev/null || { echo "dagu not on PATH: cannot measure" >&2; return 1; }
}

@test "dagu loads the test pipeline" {
  [ -f "$WF" ]
  run dagu validate "$WF"
  [ "$status" -eq 0 ]
}

@test "negative proof: a step depending on a step that does not exist is refused" {
  sed 's/depends: \[skill-werk-test-scope\]/depends: [skill-no-such-step]/' "$WF" > "$BATS_TEST_TMPDIR/broken.yaml"
  ! cmp -s "$WF" "$BATS_TEST_TMPDIR/broken.yaml"
  run dagu validate "$BATS_TEST_TMPDIR/broken.yaml"
  [ "$status" -ne 0 ]
}
