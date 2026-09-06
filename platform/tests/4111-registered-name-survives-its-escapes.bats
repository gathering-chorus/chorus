#!/usr/bin/env bats
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

setup() {
  TAGGER="$BATS_TEST_DIRNAME/../scripts/tag-tests-domain.py"
  TMP="$BATS_TEST_TMPDIR"
}
names_of() { python3 "$TAGGER" --names-of "$1"; }

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
  [[ "$output" != 'lock: no direct Command::new(\' ]]
  [[ "$output" != *'\"'* ]]
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
  [[ "$output" != *'\\n'* ]]
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
