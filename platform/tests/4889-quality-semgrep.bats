#!/usr/bin/env bats
# @test-type: unit — the quality review's Semgrep rules against their own fixtures
# @domain: proving
# @card: #4889 · owner: silas
# Jeff 2026-11-04 19:04 ("fine do that"): Semgrep in the quality review. Each rule must
# fire on the shape it names and stay quiet on the fixed shape (#3734 negative proof).
# The rules under test: platform/quality/semgrep/quality.yml
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
R="$ROOT/platform/quality/semgrep"
F="$ROOT/platform/tests/fixtures/4889-quality-semgrep"

scan() {
  SSL_CERT_FILE=/etc/ssl/cert.pem semgrep scan --config "$R/quality.yml" "$@" --json --metrics=off --disable-version-check 2>/dev/null \
    | python3 -c 'import json,sys; d=json.load(sys.stdin); print(" ".join(sorted(r["check_id"].split(".")[-1]+"@"+r["path"].rsplit("/",1)[-1] for r in d["results"])))'
}

@test "each quality rule fires on its planted shape" {
  out="$(scan "$F/swallow.ts" "$F/neg.bats.sample")"
  [ "$out" = "bats-negation-cannot-fail@neg.bats.sample empty-catch-swallows-failure@swallow.ts" ] || { echo "$out"; return 1; }
}

@test "negative proof: the fixed shape draws no finding" {
  out="$(scan "$F/clean.ts")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}
