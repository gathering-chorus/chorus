#!/usr/bin/env bats
# @test-type: integration — read_code_first's changed-definition rule (cargo) and its lookup recording on the dispatch; no live services
# @domain: policies
#
# #5027 AC4 (Jeff 11-21 17:47: "this is where lsp and ast also get hooked in"): an edit that
# changes a definition line (a function's name or parameters, a type's name or header) is
# refused unless the session asked cclsp or ast-grep about that symbol; body edits are not
# checked. NEGATIVE: a signature change or rename with no lookup is refused, naming the symbol.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
HK="$ROOT/platform/services/chorus-hooks"
MAIN="$HK/src/main.rs"

cargo_hooks() { env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$HK/target/$(id -un)" nice cargo test -q --manifest-path "$HK/Cargo.toml" "$@"; }
fn_body() { awk -v f="fn $2(" 'index($0,f){p=1} p{print} p&&/^}/{exit}' "$1"; }
# every PreToolUse call is offered to the lookup recorder before any gate can return
records_lookups() {
  fn_body "$1" pre_tool_use_inner | awk '/read_code_first::record_lookup\(input/{r=NR} /return \(/{if(!ret)ret=NR} END{exit !(r && r<ret)}'
}

@test "#5027 a changed definition needs a lookup; body edits, additions and reflows do not" {
  run cargo_hooks --lib read_code_first_lsp_ast
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -q "4 passed" || { echo "$output" | tail -5; return 1; }
}

@test "#5027 the dispatch records lookups before any gate returns" {
  records_lookups "$MAIN" || { fn_body "$MAIN" pre_tool_use_inner | grep -n "record_lookup\|return (" | head; return 1; }
}

@test "#5027 NEGATIVE: a dispatch that records lookups only after a gate can return is caught" {
  f="$BATS_TEST_TMPDIR/main.rs"
  printf 'async fn pre_tool_use_inner(s: &S) {\n    if x { return ("a".into(), r); }\n    hooks::read_code_first::record_lookup(input, &l);\n}\n' > "$f"
  if records_lookups "$f"; then echo "a late recorder passed"; return 1; fi
  printf 'async fn pre_tool_use_inner(s: &S) {\n    if x { return ("a".into(), r); }\n}\n' > "$f"
  if records_lookups "$f"; then echo "no recorder passed"; return 1; fi
}
