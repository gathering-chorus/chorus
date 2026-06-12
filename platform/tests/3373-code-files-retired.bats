#!/usr/bin/env bats
# #3373 retirement gate — /api/chorus/domain/:domain/code-files is RETIRED
# (deprecated by #2060, zero non-test consumers at deletion). Structural
# memory: this gate fails if the route or its handler reappears.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"

@test "code-files route is not registered in server.ts" {
  run grep -c "app.get('/api/chorus/domain/:domain/code-files'" "$REPO/platform/api/src/server.ts"
  [ "$output" = "0" ]
}

@test "chorus-code-files handler does not exist" {
  [ ! -f "$REPO/platform/api/src/handlers/chorus-code-files.ts" ]
}

@test "no non-test source references fetchChorusCodeFiles" {
  run bash -c "grep -rl 'fetchChorusCodeFiles' '$REPO/platform/api/src' 2>/dev/null | wc -l | tr -d ' '"
  [ "$output" = "0" ]
}
