#!/usr/bin/env bats
# @test-type: integration — chorus-hooks' read_code_first gate (cargo), its place on the PreToolUse dispatch, and its contract row; no live services
# @domain: policies
#
# #5027 AC2 (Jeff 11-21 17:41: "a stop hook as the very first step of werk-code"): an Edit or
# Write to an existing werk file this session has not read in full is refused, naming the file
# and the rule; a file read in full (one Read, or ranges that together cover it) passes, and so
# does a new file. NEGATIVE: a partial read, a shell read, no read. The cargo tests carry the
# negatives (each asserts the refusal); this file runs them and proves the wiring.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
HK="$ROOT/platform/services/chorus-hooks"
MAIN="$HK/src/main.rs"

cargo_hooks() { env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$HK/target/$(id -un)" nice cargo test -q --manifest-path "$HK/Cargo.toml" "$@"; }

# prints the body of fn $2 in the Rust file $1
fn_body() { awk -v f="fn $2(" 'index($0,f){p=1} p{print} p&&/^}/{exit}' "$1"; }
# in the Write/Edit arm, read_code_first runs right after canonical_write_guard and before every other gate
first_after_canonical() {
  fn_body "$1" pre_tool_use_inner | awk '/canonical_write_guard::check/{c=NR} /read_code_first_gate\(state, input\)/{if(!r)r=NR} /bounded_surfaces/{if(!b)b=NR} END{exit !(c && r && r>c && r<b)}'
}
# PostToolUse records what a Read returned
records_reads() { fn_body "$1" post_tool_use | grep -q 'read_code_first::record_result(&input'; }

@test "#5027 the gate: a whole-file read passes, a partial or shell read is refused naming file and rule" {
  run cargo_hooks --lib read_code_first_hook
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -qE "test result: ok. [1-9][0-9]* passed" || { echo "$output" | tail -5; return 1; }
}

@test "#5027 the real dispatch refuses an unread werk file and lets a fully read one past" {
  run cargo_hooks --bin chorus-hooks read_code_first_dispatch
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -qE "test result: ok. [1-9][0-9]* passed" || { echo "$output" | tail -5; return 1; }
}

@test "#5027 it is the first gate after the canonical guard, and reads are recorded at PostToolUse" {
  first_after_canonical "$MAIN" || { fn_body "$MAIN" pre_tool_use_inner | grep -n "::check" | head; return 1; }
  records_reads "$MAIN" || { fn_body "$MAIN" post_tool_use | head -30; return 1; }
  grep -q "^read_code_first	edit	platform/tests/read-code-first-hook.bats	" "$ROOT/platform/config/werk-code-contract.tsv"
}

@test "#5027 NEGATIVE: a dispatch that never calls the gate, or calls it after another gate, is caught" {
  f="$BATS_TEST_TMPDIR/main.rs"
  printf 'async fn pre_tool_use_inner(s: &S) {\n    let r = hooks::canonical_write_guard::check(input);\n    let r = bounded_surfaces(x);\n}\nasync fn post_tool_use(s: S) {\n}\n' > "$f"
  if first_after_canonical "$f"; then echo "a dispatch without the gate passed"; return 1; fi
  printf 'async fn pre_tool_use_inner(s: &S) {\n    let r = hooks::canonical_write_guard::check(input);\n    let r = bounded_surfaces(x);\n    let r = read_code_first_gate(state, input).await;\n}\n' > "$f"
  if first_after_canonical "$f"; then echo "a gate after bounded_surfaces passed"; return 1; fi
  if records_reads "$f"; then echo "a PostToolUse that records nothing passed"; return 1; fi
}
