#!/usr/bin/env bats
# @test-type: unit — hermetic: covers_for runs offline (TESTS_COVERS_OFFLINE),
# share fixtures are authored JSON; no store, no network
# #3996 — covers-inference precision + the share gate, proven both directions
# (#3734): the over-share state must FAIL, the healthy state must pass, and the
# rules must be deterministic (same path → same domain, twice).

INGEST="$BATS_TEST_DIRNAME/../scripts/tag-tests-domain.py"

setup() { export TESTS_COVERS_OFFLINE=1; }

cov() { python3 "$INGEST" --covers-of "$1"; }

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
  run python3 "$INGEST" --check-shares "$BATS_TEST_TMPDIR/over.json"
  [ "$status" -ne 0 ]
  [[ "$output" == *"covers-share gate RED"* ]]
  [[ "$output" == *"services holds 500/700"* ]]
}

@test "healthy shares pass the gate (the check separates its two states)" {
  cat > "$BATS_TEST_TMPDIR/ok.json" <<'EOF'
{"services": 200, "cards": 180, "messages": 170, "builds": 150, "cicd": 140}
EOF
  run python3 "$INGEST" --check-shares "$BATS_TEST_TMPDIR/ok.json"
  [ "$status" -eq 0 ]
  [[ "$output" == *"shares ok"* ]]
}

@test "threshold is config: a tighter env cap flips the same fixture red" {
  cat > "$BATS_TEST_TMPDIR/ok.json" <<'EOF'
{"services": 200, "cards": 180, "messages": 170, "builds": 150, "cicd": 140}
EOF
  MAX_DOMAIN_SHARE=0.10 run python3 "$INGEST" --check-shares "$BATS_TEST_TMPDIR/ok.json"
  [ "$status" -ne 0 ]
}
