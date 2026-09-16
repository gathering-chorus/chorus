#!/usr/bin/env bats
# #4185 — repointed again: the parsers moved from the Python library (#4159)
# into the crawler crate (chorus-crawl, src/cases.rs). Same behaviour, same
# asserts, third home — the seams are the binary's own (--names-of, --covers-of,
# --check-shares, --classify), no store, no network.
# @test-type: unit — hermetic. Drives the tagger's --names-of seam with fixture
# files in BATS_TEST_TMPDIR. No store, no network, no runner.
#
# #4111 — the registry stored the SOURCE spelling of a case name while the
# runner emits the EVALUATED one, so ten registered rows could never join and
# read as never-ran every night. Two shapes, both measured 2026-09-06 04:18
# against real files:
#
#   source  it('escapes newlines to literal \\n')      runner  ...literal \n
#   source  @test "the \$\$ name differs"               runner  the $$ name differs
#   source  @test "...Command::new(\"osascript\")..."   registry TRUNCATED at the \"
#
# The truncation was the worst of the three: `[^"]+` stopped at the escaped
# quote, so the row held `lock: chorus-hooks contains no direct Command::new(\`
# — a string no runner will ever emit. The real bats TAP line for that case,
# read by running it, is the full name with plain double quotes.

# bash 3.2 (this Mac) never fires errexit on a failing `[[ ]]`, so a `[[` assert
# that is not the LAST line of a test can fail and the test still passes (#4185,
# measured 2026-09-16: `[[ "a" == *"b"* ]]; true` → ok). Every assert here is a
# simple command, which bash 3.2 does honour.
has()   { grep -qF -- "$1" <<<"${2-$output}"; }
lacks() { if grep -qF -- "$1" <<<"${2-$output}"; then echo "unexpected: $1" >&2; return 1; fi; }
eq()    { [ "$1" = "$2" ] || { echo "expected [$2] got [$1]" >&2; return 1; }; }

setup() {
  BIN="${CHORUS_CRAWL_BIN:-$BATS_TEST_DIRNAME/../services/chorus-crawl/target/release/chorus-crawl}"; [ -x "$BIN" ] || BIN="$BATS_TEST_DIRNAME/../services/chorus-crawl/target/debug/chorus-crawl"; [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
  TMP="$BATS_TEST_TMPDIR"
}
names_of() { "$BIN" --names-of "$1"; }

@test "a bats name keeps everything after an escaped quote" {
  f="$TMP/locks.bats"
  printf '%s\n' '@test "lock: no direct Command::new(\"osascript\") — route via inject" {' '  true' '}' > "$f"
  run names_of "$f"
  [ "$status" -eq 0 ]
  [ "$output" = 'lock: no direct Command::new("osascript") — route via inject' ]
}

@test "negative proof: the truncated spelling is not what gets registered" {
  # The exact row that sat in the registry until today. If the extractor ever
  # stops at the escaped quote again, this comes back and the test goes red.
  f="$TMP/locks.bats"
  printf '%s\n' '@test "lock: no direct Command::new(\"osascript\") — route via inject" {' '  true' '}' > "$f"
  run names_of "$f"
  [ "$output" != 'lock: no direct Command::new(\' ]
  lacks '\"'
}

@test "a bats name unescapes \$ the way bash prints it" {
  f="$TMP/iso.bats"
  printf '%s\n' '@test "NEGATIVE PROOF: the \$\$ name differs" {' '  true' '}' > "$f"
  run names_of "$f"
  [ "$output" = 'NEGATIVE PROOF: the $$ name differs' ]
}

@test "a jest name is the string's value, not its source" {
  f="$TMP/esc.test.ts"
  printf '%s\n' "it('escapes newlines to literal \\\\n', () => {});" > "$f"
  run names_of "$f"
  [ "$output" = 'escapes newlines to literal \n' ]
}

@test "negative proof: the double-escaped spelling is not registered" {
  f="$TMP/esc.test.ts"
  printf '%s\n' "it('escapes newlines to literal \\\\n', () => {});" > "$f"
  run names_of "$f"
  lacks '\\n'
}

@test "control: a name with no escapes is unchanged" {
  f="$TMP/plain.bats"
  printf '%s\n' '@test "health check reports all roles reachable" {' '  true' '}' > "$f"
  run names_of "$f"
  [ "$output" = "health check reports all roles reachable" ]
}

@test "control: an apostrophe inside a double-quoted jest name still survives" {
  f="$TMP/q.test.ts"
  printf '%s\n' "it(\"a name with 'inner' quotes\", () => {});" > "$f"
  run names_of "$f"
  [ "$output" = "a name with 'inner' quotes" ]
}
