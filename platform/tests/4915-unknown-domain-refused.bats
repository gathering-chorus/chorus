#!/usr/bin/env bats
# @test-type: integration — the built werk-test binary on a scratch git werk; the Domain list from a file door.
# @domain: tests
# @card: #4915
# #4915 round 2 (Jeff's go 11-07 08:47): werk-test refuses a changed test whose
# @domain names a domain the store does not list, before anything runs. On 11-07,
# 19 such files had landed and #4919 landed 3 more (`werk`).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  BIN="$ROOT/platform/services/werk-test/target/debug/werk-test"
  [ -x "$BIN" ] || (cd "$ROOT/platform/services/werk-test" && cargo build -q)
  T="$(mktemp -d "${BATS_TMPDIR:-/tmp}/wt4915.XXXXXX")"
  mkdir -p "$T/door/v1/domains" "$T/werk/platform/tests"
  printf '{"data":[{"name":"cicd"},{"name":"tests"}]}' > "$T/door/v1/domains/domains"
  git -C "$T/werk" init -q -b main
  git -C "$T/werk" -c user.email=t@t -c user.name=t commit -q --allow-empty -m base
  git -C "$T/werk" update-ref refs/remotes/origin/main HEAD
  # a stand-in werk-demo: werk-test records its verdict through it, and a fixture
  # run must never write a result for a real card
  mkdir -p "$T/bin" && printf '#!/bin/sh\nexit 0\n' > "$T/bin/werk-demo" && chmod +x "$T/bin/werk-demo"
}
teardown() { rm -rf "$T"; }

change() {
  printf '#!/usr/bin/env bats\n# @domain: %s\n@test "x" { true; }\n' "$1" > "$T/werk/platform/tests/zz.bats"
  git -C "$T/werk" add -A && git -C "$T/werk" -c user.email=t@t -c user.name=t commit -q -m change
}
run_wt() {
  run env PATH="$T/bin:$PATH" WERK_TEST_TREE="$T/werk" ATHENA_MAKE_URL="file://$T/door" WERK_TEST_OWN_BUILD=1 \
    CHORUS_TRACE_ID= "$BIN" 4915 kade --select-only
}

@test "negative proof: a test declaring an unlisted domain is refused and named" {
  change nowhere
  run_wt
  [ "$status" -eq 2 ]
  [[ "$output" == *"domain-check: REFUSED — platform/tests/zz.bats declares @domain: nowhere"* ]] || return 1
}

@test "a listed domain passes the check" {
  change cicd
  run_wt
  [[ "$output" != *"domain-check: REFUSED"* ]] || return 1
  [[ "$output" == *"select-only: platform/tests/zz.bats"* ]] || return 1  # it got past the check
}

@test "the store unreadable is said, never a silent pass" {
  change nowhere
  run env PATH="$T/bin:$PATH" WERK_TEST_TREE="$T/werk" ATHENA_MAKE_URL="file://$T/no-door" WERK_TEST_OWN_BUILD=1 "$BIN" 4915 kade --select-only
  [[ "$output" == *"domain-check: UNMEASURED"* ]] || return 1
}
