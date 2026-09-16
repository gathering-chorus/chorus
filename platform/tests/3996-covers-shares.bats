#!/usr/bin/env bats
# #4185 — repointed again: the parsers moved from the Python library (#4159)
# into the crawler crate (chorus-crawl, src/cases.rs). Same behaviour, same
# asserts, third home — the seams are the binary's own (--names-of, --covers-of,
# --check-shares, --classify), no store, no network.
# @test-type: unit — hermetic: covers_for runs offline (TESTS_COVERS_OFFLINE),
# share fixtures are authored JSON; no store, no network
# #3996 — covers-inference precision + the share gate, proven both directions
# (#3734): the over-share state must FAIL, the healthy state must pass, and the
# rules must be deterministic (same path → same domain, twice).

# bash 3.2 (this Mac) never fires errexit on a failing `[[ ]]`, so a `[[` assert
# that is not the LAST line of a test can fail and the test still passes (#4185,
# measured 2026-09-16: `[[ "a" == *"b"* ]]; true` → ok). Every assert here is a
# simple command, which bash 3.2 does honour.
has()   { grep -qF -- "$1" <<<"${2-$output}"; }
lacks() { if grep -qF -- "$1" <<<"${2-$output}"; then echo "unexpected: $1" >&2; return 1; fi; }
eq()    { [ "$1" = "$2" ] || { echo "expected [$2] got [$1]" >&2; return 1; }; }

setup() {
  BIN="${CHORUS_CRAWL_BIN:-$BATS_TEST_DIRNAME/../services/chorus-crawl/target/release/chorus-crawl}"; [ -x "$BIN" ] || BIN="$BATS_TEST_DIRNAME/../services/chorus-crawl/target/debug/chorus-crawl"; [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
}

# --covers-of reads the file if it exists (a security CONCERN re-homes covers to
# security, #3922 lane); these paths are not on disk, so the path rules answer.
cov() { "$BIN" --covers-of "$1"; }

@test "the former services-bucket trees map to their real domains" {
  [ "$(cov directing/products/cards/tests/card-lifecycle-flow.test.ts)" = "cards" ]
  [ "$(cov directing/clearing/tests/router.test.ts)" = "messages" ]
  [ "$(cov platform/services/athena-make/tests/reconcile.rs)" = "domains" ]
  [ "$(cov platform/services/chorus-oidc/tests/token.rs)" = "identity" ]
}

@test "basename keywords beat package prefixes (api test about alerts covers alerts)" {
  [ "$(cov platform/api/tests/eventloop-alert.test.ts)" = "alerts-monitors" ]
  [ "$(cov platform/api/tests/search-meta.test.ts)" = "search" ]
  # no keyword in the name → the package prefix still answers
  [ "$(cov platform/api/tests/server-unit.test.ts)" = "services" ]
}

@test "a security-concern file covers security, whatever its path (the #3922 lane keeps its rows)" {
  f="$BATS_TEST_TMPDIR/account.test.ts"
  printf '%s\n' "// @test-type: integration:security" "it('refuses', () => {});" > "$f"
  [ "$(cov "$f")" = "security" ]
  # control: the same path with no security concern falls to the path rules
  g="$BATS_TEST_TMPDIR/router.test.ts"
  printf '%s\n' "it('routes', () => {});" > "$g"
  [ "$(cov "$g")" != "security" ]
}

@test "deterministic: same path answers the same domain twice (re-ingest stability)" {
  a=$(cov platform/pulse/src/delivery-worker.test.ts)
  b=$(cov platform/pulse/src/delivery-worker.test.ts)
  [ "$a" = "$b" ]
  [ -n "$a" ]
}

@test "negative proof: an over-share corpus REFUSES to land" {
  cat > "$BATS_TEST_TMPDIR/over.json" <<'EOF'
{"services": 500, "cards": 100, "messages": 100}
EOF
  run "$BIN" --check-shares "$BATS_TEST_TMPDIR/over.json"
  [ "$status" -ne 0 ]
  has "covers-share gate RED"
  has "services holds 500/700"
}

@test "healthy shares pass the gate (the check separates its two states)" {
  cat > "$BATS_TEST_TMPDIR/ok.json" <<'EOF'
{"services": 200, "cards": 180, "messages": 170, "builds": 150, "cicd": 140}
EOF
  run "$BIN" --check-shares "$BATS_TEST_TMPDIR/ok.json"
  [ "$status" -eq 0 ]
  has "shares ok"
}

@test "threshold is config: a tighter env cap flips the same fixture red" {
  cat > "$BATS_TEST_TMPDIR/ok.json" <<'EOF'
{"services": 200, "cards": 180, "messages": 170, "builds": 150, "cicd": 140}
EOF
  MAX_DOMAIN_SHARE=0.10 run "$BIN" --check-shares "$BATS_TEST_TMPDIR/ok.json"
  [ "$status" -ne 0 ]
}
