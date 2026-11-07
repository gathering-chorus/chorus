#!/usr/bin/env bats
# @test-type: unit — the architecture review's dependency-cruiser rules on a planted tree
# @domain: cicd
# @card: #4889 · owner: silas
# Jeff 2026-11-04 19:04 ("fine do that"): dependency-cruiser in the architecture review.
# Kade 19:47: "a planted boundary break that goes red". The rules: platform/quality/depcruise/boundaries.cjs
# The tool comes from the root package.json (devDependency dependency-cruiser, pinned in
# package-lock.json): 17.4.3, the last major that runs on our node 20 (18 needs node 22).
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CFG="$ROOT/platform/quality/depcruise/boundaries.cjs"

plant() {  # a tiny tree with two domain packages and the sdk
  T="$BATS_TEST_TMPDIR/t"; mkdir -p "$T/directing/clearing/src" "$T/platform/api/src" "$T/platform/chorus-sdk/time"
  echo 'export const b = 1;' > "$T/platform/api/src/b.ts"
  echo 'export const now = 1;' > "$T/platform/chorus-sdk/time/boston.ts"
  echo "$1" > "$T/directing/clearing/src/a.ts"
}
cruise() { (cd "$T" && "$ROOT/node_modules/.bin/depcruise" --config "$CFG" --output-type err directing platform 2>&1); }

# the version package.json and package-lock.json pin for dependency-cruiser, or empty
pinned() {
  python3 - "$1" <<'PY2'
import json, sys
d = json.load(open(sys.argv[1]))
if "packages" in d:   # package-lock.json
    print((d["packages"].get("node_modules/dependency-cruiser") or {}).get("version", ""))
else:                 # package.json
    print({**d.get("dependencies", {}), **d.get("devDependencies", {})}.get("dependency-cruiser", ""))
PY2
}

@test "package.json and package-lock.json pin dependency-cruiser 17.4.3, and that is what runs" {
  [ "$(pinned "$ROOT/package.json")" = "17.4.3" ] || { echo "package.json pins '$(pinned "$ROOT/package.json")'"; return 1; }
  [ "$(pinned "$ROOT/package-lock.json")" = "17.4.3" ] || { echo "package-lock.json pins '$(pinned "$ROOT/package-lock.json")'"; return 1; }
  run "$ROOT/node_modules/.bin/depcruise" --version
  [ "$output" = "17.4.3" ] || { echo "installed depcruise is '$output'"; return 1; }
}

@test "negative proof: a range or another major is not the pin" {
  printf '{"devDependencies":{"dependency-cruiser":"^18.0.0"}}' > "$BATS_TEST_TMPDIR/package.json"
  [ "$(pinned "$BATS_TEST_TMPDIR/package.json")" = "^18.0.0" ] || return 1
  printf '{"packages":{"node_modules/dependency-cruiser":{"version":"18.0.0"}}}' > "$BATS_TEST_TMPDIR/package-lock.json"
  [ "$(pinned "$BATS_TEST_TMPDIR/package-lock.json")" = "18.0.0" ] || return 1
  printf '{"devDependencies":{}}' > "$BATS_TEST_TMPDIR/empty.json"
  [ -z "$(pinned "$BATS_TEST_TMPDIR/empty.json")" ] || return 1
}

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
