#!/usr/bin/env bats
# @test-type: integration — read_code_first's shell leg (cargo, over real git werks in a tmp dir) and its place on the dispatch; no live services
# @domain: policies
# @card: #5033 AC4 · owner: wren (built by kade)
#
# Jeff 11-24 09:45: the read-the-code-first rule (#5027) also holds for changes made through
# the shell. After a shell command, a tracked werk file whose content it changed, and that the
# session had not read whole (or written) before the command ran, is named; the session's next
# Edit, Write or shell command is refused until the file is restored. NEGATIVE: sed -i, a
# heredoc, python open('w'), git apply, perl -pi and rm on an unread file are each seen; the
# next Edit and the next shell command refuse, naming the file and its restore command; after
# git restore both pass; the same sed -i on a file read whole is never refused. The cargo
# tests carry the negatives; this file runs them and proves the wiring.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
HK="$ROOT/platform/services/chorus-hooks"
MAIN="$HK/src/main.rs"

cargo_hooks() { env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$HK/target/$(id -un)" nice cargo test -q --manifest-path "$HK/Cargo.toml" "$@"; }
fn_body() { awk -v f="fn $2(" 'index($0,f){p=1} p{print} p&&/^}/{exit}' "$1"; }
# the Bash arm asks the debt gate before any other Bash guard
bash_arm_gated() {
  fn_body "$1" pre_tool_use_inner | awk '/"Bash" => \{/{a=NR} a && /read_code_first_gate\(state, input\)/{if(!g)g=NR} a && /sparql_guard::check/{if(!s)s=NR} END{exit !(a && g && s && g<s)}'
}
# an allowed shell command is snapshotted before it runs, and scanned after it ends
snapshots() { fn_body "$1" pre_tool_use | grep -q 'before_shell(&state, &input)' && fn_body "$1" before_shell | grep -q 'read_code_first::shell_pre(&i, &l)'; }
scans() { fn_body "$1" post_tool_use | grep -q 'after_shell(&state, &input)' && fn_body "$1" after_shell | grep -q 'read_code_first::shell_post(&i, &l)'; }
# the commit tools pass the debt gate (Wren 10:15 A)
commits_gated() { fn_body "$1" pre_tool_use_inner | grep -A2 '"mcp__chorus-api__werk-commit"' | grep -q 'read_code_first_gate(state, input)'; }

@test "#5033 AC4 the shell leg: every way of writing is seen, the next edit and shell refuse until restored" {
  run cargo_hooks --lib read_code_first_shell
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -q "test result: ok. 10 passed" || { echo "$output" | tail -5; return 1; }
}

@test "#5033 AC4 the dispatch: debt gate first on Bash, a snapshot before, a scan after" {
  bash_arm_gated "$MAIN" || { fn_body "$MAIN" pre_tool_use_inner | grep -n '"Bash" =>\|read_code_first_gate\|sparql_guard' | head; return 1; }
  snapshots "$MAIN" || { fn_body "$MAIN" pre_tool_use | head -30; return 1; }
  scans "$MAIN" || { fn_body "$MAIN" post_tool_use | head -40; return 1; }
  commits_gated "$MAIN" || { fn_body "$MAIN" pre_tool_use_inner | grep -n 'werk-commit' ; return 1; }
  grep -q "^read_code_first_shell	edit	platform/tests/read-code-first-shell.bats	" "$ROOT/platform/config/werk-code-contract.tsv"
}

@test "#5033 AC4 the real dispatch: a blind shell change refuses the next Edit, shell and werk-commit; the restore passes" {
  run cargo_hooks --bin chorus-hooks read_code_first_shell_dispatch
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -q "test result: ok. 1 passed" || { echo "$output" | tail -5; return 1; }
}

@test "#5033 AC4 NEGATIVE: a dispatch with no debt gate on Bash, no snapshot or no scan is caught" {
  f="$BATS_TEST_TMPDIR/main.rs"
  printf 'async fn pre_tool_use_inner(s: &S) {\n    match t {\n        "Bash" => {\n            let r = hooks::sparql_guard::check(input).await;\n        }\n    }\n}\nasync fn pre_tool_use(s: S) {\n}\nasync fn post_tool_use(s: S) {\n}\n' > "$f"
  if bash_arm_gated "$f"; then echo "a Bash arm without the gate passed"; return 1; fi
  if snapshots "$f"; then echo "no snapshot passed"; return 1; fi
  if scans "$f"; then echo "no scan passed"; return 1; fi
  if commits_gated "$f"; then echo "ungated commit tools passed"; return 1; fi
  printf 'async fn pre_tool_use_inner(s: &S) {\n    match t {\n        "Bash" => {\n            let r = hooks::sparql_guard::check(input).await;\n            let r = read_code_first_gate(state, input).await;\n        }\n    }\n}\n' > "$f"
  if bash_arm_gated "$f"; then echo "a gate after sparql_guard passed"; return 1; fi
}
