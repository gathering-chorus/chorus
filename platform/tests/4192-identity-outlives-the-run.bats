#!/usr/bin/env bats
# @test-type: integration
# #4192 — the crawler identity lives 600 s; a full pass does not. The run
# re-mints inside itself. Proven live against a werk VARIANT (#4180 pattern).

has() { grep -qF -- "$1" <<<"${2-$output}"; }

setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration — RUN_INTEGRATION=true against a werk variant"
  OWL_URL="${OWL_URL:-}"
  case "$OWL_URL" in ""|*:3360*) skip "refuses to write to the canonical store — point OWL_URL at a werk variant" ;; esac
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  BIN="${CHORUS_CRAWL_BIN:-$REPO/platform/services/chorus-crawl/target/release/chorus-crawl}"
  [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
  MINT="$REPO/platform/scripts/chorus-identity-token"
  FX="$BATS_TEST_TMPDIR/repo"; mkdir -p "$FX/platform/tests"
  TAG="fx4192$$"
  printf '@test "%s one" {\n  true\n}\n' "$TAG" > "$FX/platform/tests/$TAG.bats"
  git -C "$FX" init -q && git -C "$FX" add -A && git -C "$FX" -c user.name=t -c user.email=t@t commit -q -m fixture
  # an already-EXPIRED token in the shape the minter produces (exp in 2020)
  EXPIRED="eyJhbGciOiJub25lIn0.$(printf '{"sub":"crawler","iat":1600000000,"exp":1600000600}' | base64 | tr '+/' '-_' | tr -d '=\n').sig"
  export CHORUS_ROOT="$FX" CHORUS_OWL_API="$OWL_URL" CHORUS_ROLE=crawler
}
teardown() {
  TC="$("$MINT" crawler 2>/dev/null)"
  for c in /tests/tests /code/files; do
    curl -s --max-time 30 -H "Authorization: Bearer $TC" "$OWL_URL$c?limit=20000" | python3 -c "
import sys,json
for r in json.load(sys.stdin).get('data',[]):
    if '$TAG' in r.get('filePath',''): print(r['name'])" | while read -r n; do
      curl -s -o /dev/null -X DELETE -H "Authorization: Bearer $TC" --max-time 10 "$OWL_URL$c/$n" || true
    done
  done
}

@test "a run handed an expired identity re-mints inside itself and writes" {
  export CHORUS_IDENTITY_TOKEN="$EXPIRED" CHORUS_IDENTITY_MINT="$MINT"
  run "$BIN"; echo "$output"
  [ "$status" -eq 0 ]
  has "wrote=2 failed=0"
  has "mints=1"
}

# NEGATIVE PROOF (#3734): a run that cannot re-mint goes RED at once — it never
# keeps writing with a token it knows is dead, and never silently 401s.
@test "NEGATIVE PROOF: a run that cannot re-mint refuses to write and says why" {
  export CHORUS_IDENTITY_TOKEN="$EXPIRED" CHORUS_IDENTITY_MINT=/usr/bin/false
  run "$BIN"; echo "$output"
  [ "$status" -eq 2 ]
  has "re-mint failed"
  has "refusing"
}
