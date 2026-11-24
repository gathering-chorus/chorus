#!/usr/bin/env bats
# @test-type: integration — read_code_first takes its rule from the policies row, and refuses when the row cannot be read (cargo); no live services
# @domain: policies
#
# #5027 AC3: the hook reads the rule from the policy row read-the-code-first, never a copy in
# code; with the row absent it refuses and says the policy could not be read, never passes.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
HK="$ROOT/platform/services/chorus-hooks"
SRC="$HK/src/hooks/read_code_first.rs"
PLAN="$ROOT/designing/domains/policies/policy-checks-5027.json"

cargo_hooks() { env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$HK/target/$(id -un)" nice cargo test -q --manifest-path "$HK/Cargo.toml" "$@"; }

# the hook's code holds no sentence of the row's rule: the words live in the row only
no_copy() {
  python3 - "$1" "$2" <<'PY'
import json, re, sys
code = open(sys.argv[1]).read()
rule = json.load(open(sys.argv[2]))[0]["comment"]
for sentence in re.split(r"(?<=\.)\s+", rule):
    probe = sentence.strip()[:60]
    assert len(probe) < 25 or probe not in code, f"the hook copies the rule: {probe!r}"
PY
}

@test "#5027 the refusal carries the row's words; an absent or empty row refuses and says so" {
  run cargo_hooks --lib read_code_first_reads_policy
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -q "3 passed" || { echo "$output" | tail -5; return 1; }
}

@test "#5027 the hook's code holds no copy of the rule" {
  no_copy "$SRC" "$PLAN"
}

@test "#5027 NEGATIVE: a hook that pastes the rule into its code is caught" {
  f="$BATS_TEST_TMPDIR/rcf.rs"
  { cat "$SRC"; python3 -c "import json;print('const RULE: &str = \"' + json.load(open('$PLAN'))[0]['comment'][:80] + '\";')"; } > "$f"
  if no_copy "$f" "$PLAN" 2>/dev/null; then echo "a pasted rule passed"; return 1; fi
}
