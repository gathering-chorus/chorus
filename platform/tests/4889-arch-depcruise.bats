#!/usr/bin/env bats
# @test-type: unit — the architecture review's dependency-cruiser rules on a planted tree
# @domain: proving
# @card: #4889 · owner: silas
# Jeff 2026-11-04 19:04 ("fine do that"): dependency-cruiser in the architecture review.
# Kade 19:47: "a planted boundary break that goes red". The rules: platform/quality/depcruise/boundaries.cjs
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CFG="$ROOT/platform/quality/depcruise/boundaries.cjs"

plant() {  # a tiny tree with two domain packages and the sdk
  T="$BATS_TEST_TMPDIR/t"; mkdir -p "$T/directing/clearing/src" "$T/platform/api/src" "$T/platform/chorus-sdk/time"
  echo 'export const b = 1;' > "$T/platform/api/src/b.ts"
  echo 'export const now = 1;' > "$T/platform/chorus-sdk/time/boston.ts"
  echo "$1" > "$T/directing/clearing/src/a.ts"
}
cruise() { (cd "$T" && "$ROOT/node_modules/.bin/depcruise" --config "$CFG" --output-type err directing platform 2>&1); }

@test "the real tree holds its boundaries" {
  run bash -c "cd '$ROOT' && node_modules/.bin/depcruise --config '$CFG' --output-type err directing platform/api platform/pulse platform/workflow-engine platform/agent-adapters platform/chorus-sdk"
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
}

@test "negative proof: a domain package reaching into another's files goes red" {
  plant "import { b } from '../../../platform/api/src/b'; export const a = b;"
  run cruise
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"error no-reach-into-another-package: directing/clearing/src/a.ts → platform/api/src/b.ts"* ]] || { echo "$output"; return 1; }
}

@test "the shared sdk by path is allowed, so the red above is about the boundary" {
  plant "import { now } from '../../../platform/chorus-sdk/time/boston'; export const a = now;"
  run cruise
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: a cycle goes red" {
  plant "import { c } from './c'; export const a = c;"
  echo "import { a } from './a'; export const c = a;" > "$T/directing/clearing/src/c.ts"
  run cruise
  [[ "$output" == *"error no-circular"* ]] || { echo "$output"; return 1; }
}
