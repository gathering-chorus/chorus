#!/usr/bin/env bats
# @test-type: unit — hermetic: imports the tagger's pure functions, no store
#
# #3924 — the AUTHORED @test-type header beats the path/content heuristic.
# The header was enforced at commit (#3442) and then thrown away at ingest;
# classify() re-guessed every layer. These pin the new contract:
#   authored layer[:concern] wins on BOTH axes it declares (Wren trap 1),
#   prose that merely MENTIONS the header does not declare one (Wren trap 2),
#   and absence still falls to the heuristic, flagged inferred.

run_py() {
  python3 - "$BATS_TEST_DIRNAME/../scripts/tag-tests-domain.py" <<'PY'
import sys, importlib.util
spec = importlib.util.spec_from_file_location("tagger", sys.argv[1])
m = importlib.util.module_from_spec(spec)
import types
# stub network-touching config so import is hermetic
spec.loader.exec_module(m)

def check(name, cond):
    if not cond:
        print(f"FAIL {name}"); sys.exit(1)
    print(f"ok {name}")

# authored beats heuristic on layer even when content SCREAMS integration
c = "// @test-type: unit — hermetic despite the curl below\ncurl http://localhost:3030/x"
check("declared layer wins", m.declared(c) == ("unit", None))

# authored concern beats classify()'s own concern regex (trap 1)
c2 = "// @test-type: unit:api\n#  gitleaks mention would heuristically say security"
check("declared concern wins", m.declared(c2) == ("unit", "api"))

# prose MENTION is not a declaration (trap 2 — the June gate-test-type case)
c3 = "// the @test-type: header is required by the gate\n// @test-type: headers matter"
check("prose mention stays undeclared", m.declared(c3) is None)

# junk layer -> undeclared -> heuristic+inferred, never a fabricated row
check("junk layer refused", m.declared("// @test-type: banana") is None)

# justification form parses
check("justified form", m.declared("# @test-type: e2e:ui — playwright flow") == ("e2e", "ui"))

# no header at all -> heuristic path
check("absence falls to heuristic", m.declared("plain file, no header") is None)
PY
}

@test "declared() contract — authored wins, prose does not declare, junk refused" {
  run run_py
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" != *FAIL* ]]
}

@test "discovery reaches proving/ and .spec.cjs — the zero-browser-tests hole (#3872)" {
  cd "$BATS_TEST_DIRNAME/../.."
  run python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('t','platform/scripts/tag-tests-domain.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
files = m.discover()
assert any(f.startswith('proving/') for f in files), 'proving/ invisible again'
assert any(f.endswith('.spec.cjs') for f in files), '.spec.cjs invisible again'
# NEGATIVE PROOF anchor: the exact spec that went green without running
assert 'proving/flows/clearing-base-path-3872.spec.cjs' in files
print('ok')
"
  [ "$status" -eq 0 ]
  [[ "$output" == *ok* ]]
}
