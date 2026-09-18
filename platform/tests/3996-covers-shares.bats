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
# #4201 — the seam answers on stdout and explains on stderr, so a caller can
# compare the answer. `2>/dev/null` here keeps the reason out of the value.
cov() { "$BIN" --covers-of "$1" 2>/dev/null; }

@test "#4201: a path alone answers nothing — the rules read the FILE" {
  # These paths carry no file, so no route, class, import or card can be read
  # from them. The folder rule that used to map directing/products/cards/** to
  # cards, and a basename keyword like `alert` to alerts-monitors, is retired:
  # a name is not evidence. Unplaced falls to the tests domain and is listed.
  eq "$(cov directing/products/cards/tests/card-lifecycle-flow.test.ts)" "tests"
  eq "$(cov platform/api/tests/eventloop-alert.test.ts)" "tests"
  eq "$(cov platform/services/athena-make/tests/reconcile.rs)" "tests"
}

@test "#4201 NEGATIVE PROOF: the same basenames with real content DO place" {
  # The control for the test above: if the retired rules were merely renamed,
  # these would answer the same as the pathless case. They do not — the domain
  # comes out of what is written in the file.
  a="$BATS_TEST_TMPDIR/eventloop-alert.test.ts"
  printf '%s\n' "await request(app).get('/api/chorus/cards');" > "$a"
  eq "$(CHORUS_VALID_DOMAINS="cards,alerts-monitors,tests" "$BIN" --covers-of "$a" 2>/dev/null)" "cards"

  b="$BATS_TEST_TMPDIR/server-unit.test.ts"
  printf '%s\n' "it('boots', () => {});" > "$b"
  eq "$(CHORUS_VALID_DOMAINS="cards,services,tests" "$BIN" --covers-of "$b" 2>/dev/null)" "tests"
}

@test "a security-concern file covers security, whatever its path (the #3922 lane keeps its rows)" {
  f="$BATS_TEST_TMPDIR/account.test.ts"
  printf '%s\n' "// @test-type: integration:security" "it('refuses', () => {});" > "$f"
  eq "$(cov "$f")" "security"
  # control: the same path with no security concern falls to the file rules
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
