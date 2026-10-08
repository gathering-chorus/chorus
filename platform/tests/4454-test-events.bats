#!/usr/bin/env bats
# @test-type: unit
# @domain: tests — how a test run reports itself (#4454)
#
# #4454 — tests and fixtures log on the run's trace, so a red names its cause.
# A case that fails because its fixture never came up must read differently in
# Loki from a case that fails on a wrong value. These pin the two helpers a test
# writes through and the fixture and reporter that use them.
#
# Covers: platform/tests/lib/test-events.bash
# Covers: platform/tests/lib/test-events.cjs
# Covers: proving/flows/lib/own-clearing.cjs
# Covers: proving/flows/lib/case-events-reporter.cjs
# Covers: platform/tests/lib/jest-case-events-reporter.cjs
# Covers: playwright.config.cjs
# Covers: platform/scripts/shim-wrapper.sh

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  EV="$BATS_TEST_TMPDIR/events.tsv"
  export CHORUS_TEST_EVENTS="$EV" CHORUS_TRACE_ID="trace-4454"
}

@test "test_event writes one forwardable line on the run's trace" {
  load lib/test-events.bash
  test_event test.fixture.ready fixture=stub port=1234 "message=a value with	a tab"
  run cut -f1,2 "$EV"
  [ "$output" = "test.fixture.ready	tests" ]
  run grep -c "trace=trace-4454" "$EV"
  [ "$output" = "1" ]
  # the tab inside a value did not split the event
  run awk -F'\t' '{print NF}' "$EV"
  [ "$output" = "7" ]
}

@test "NEGATIVE PROOF: outside werk-test (no CHORUS_TEST_EVENTS) nothing is written" {
  load lib/test-events.bash
  unset CHORUS_TEST_EVENTS
  test_event test.fixture.ready fixture=stub
  [ ! -e "$EV" ]
}

# A fake playwright `test` object: ownClearing registers beforeAll/afterAll on it.
fixture_down() {
  node -e '
    const { ownClearing } = require(process.argv[1]);
    const hooks = {};
    const t = { beforeAll: (f) => { hooks.before = f; }, afterAll: (f) => { hooks.after = f; } };
    // the Clearing process dies at once: a fixture that never comes up
    ownClearing(t, { env: { NODE_OPTIONS: "--require /nonexistent-4454" } });
    hooks.before().then(() => process.exit(0), () => hooks.after().then(() => process.exit(3)));
  ' "$ROOT/proving/flows/lib/own-clearing.cjs"
}

wrong_value() {
  node -e '
    const R = require(process.argv[1]);
    const r = new R();
    const test = { location: { file: process.argv[2] }, title: "shows the room", titlePath: () => ["", "", "shows the room"] };
    r.onTestBegin(test);
    r.onTestEnd(test, { status: "failed", duration: 42, error: { message: "expected \"Jeff\" got \"jeff\"" } });
  ' "$ROOT/proving/flows/lib/case-events-reporter.cjs" "$ROOT/proving/flows/clearing-page-3857.spec.cjs"
}

@test "a fixture that never comes up logs test.fixture.failed with its reason" {
  run fixture_down
  [ "$status" -eq 3 ]
  run grep -c "^test.fixture.failed" "$EV"
  [ "$output" = "1" ]
  run grep -E "^test.fixture.failed.*reason=(own Clearing exited|clearing is not built)" "$EV"
  [ "$status" -eq 0 ]
}

@test "a wrong value logs test.case.failed with the assertion and its time" {
  run wrong_value
  [ "$status" -eq 0 ]
  run grep -E '^test.case.failed.*elapsed_ms=42.*reason=expected "Jeff" got "jeff"' "$EV"
  [ "$status" -eq 0 ]
  run grep -c "^test.case.started" "$EV"
  [ "$output" = "1" ]
}

@test "NEGATIVE PROOF: the two reds read differently — only the fixture-down one has a fixture event" {
  run fixture_down
  cp "$EV" "$BATS_TEST_TMPDIR/down.tsv"; rm -f "$EV"
  run wrong_value
  run grep -c "^test.fixture.failed" "$BATS_TEST_TMPDIR/down.tsv"
  [ "$output" = "1" ]
  run grep -c "^test.fixture" "$EV"
  [ "$output" = "0" ]
  run grep -c "^test.case.failed" "$EV"
  [ "$output" = "1" ]
}

@test "the jest reporter logs each case starting" {
  run node -e '
    const R = require(process.argv[1]);
    new R().onTestCaseStart({ path: process.argv[2] }, { fullName: "room renders", title: "renders", ancestorTitles: ["room"] });
  ' "$ROOT/platform/tests/lib/jest-case-events-reporter.cjs" "$ROOT/directing/clearing/tests/x.test.ts"
  [ "$status" -eq 0 ]
  run grep -E "^test.case.started	tests	file=directing/clearing/tests/x.test.ts	case=room renders" "$EV"
  [ "$status" -eq 0 ]
}

# chorus-log is shim-wrapper.sh under another name; a fake shim on PATH shows
# what the wrapper hands it.
fake_shim_world() {
  mkdir -p "$BATS_TEST_TMPDIR/bin"
  printf '#!/usr/bin/env bash\nprintf "%%s|" "$@" > "%s/argv"; cat > "%s/stdin"\n' "$BATS_TEST_TMPDIR" "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/bin/chorus-hook-shim"
  chmod +x "$BATS_TEST_TMPDIR/bin/chorus-hook-shim"
  ln -sf "$ROOT/platform/scripts/shim-wrapper.sh" "$BATS_TEST_TMPDIR/bin/chorus-log"
}

@test "chorus-log --batch hands the shim the batch and its lines, untouched" {
  fake_shim_world
  # with a trace set: the single-event path would append trace_id=… to the argv
  run env PATH="$BATS_TEST_TMPDIR/bin:$PATH" CHORUS_TRACE_ID=t-4454 bash -c "printf 'test.case.passed\tnightly\tcase=a\n' | '$BATS_TEST_TMPDIR/bin/chorus-log' --batch"
  [ "$status" -eq 0 ]
  [ "$(cat "$BATS_TEST_TMPDIR/argv")" = "chorus-log|--batch|" ]
  [ "$(cat "$BATS_TEST_TMPDIR/stdin")" = "$(printf 'test.case.passed\tnightly\tcase=a')" ]
}

# NEGATIVE PROOF: a single event still goes the single-event way — the batch
# branch must not swallow ordinary calls.
@test "NEGATIVE PROOF: a single chorus-log event is not sent as a batch" {
  fake_shim_world
  run env PATH="$BATS_TEST_TMPDIR/bin:$PATH" bash -c "'$BATS_TEST_TMPDIR/bin/chorus-log' test.case.passed nightly case=a </dev/null"
  [ "$status" -eq 0 ]
  run cat "$BATS_TEST_TMPDIR/argv"
  [[ "$output" == chorus-log\|test.case.passed\|nightly\|* ]]  || return 1
  [[ "$output" != *--batch* ]] || return 1
}
