#!/usr/bin/env bats
# @test-type: unit — pure parsers, no store, no network
# #4154 B3 — the tagger's parsers became a library the walker imports.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  LIB="$ROOT/platform/scripts/testfiles.py"
  PY="import sys; sys.path.insert(0, '$ROOT/platform/scripts'); import testfiles as t"
  F="$BATS_TEST_TMPDIR"
}

@test "a .bats anywhere is a test file — the TEST_ROOTS gap (docs/, designing/, cargo tests/) is closed" {
  run python3 -c "$PY; print(t.is_test_file('docs/x.bats'), t.is_test_file('designing/x/tests/y.test.ts'), t.is_test_file('platform/services/werk-x/tests/z.rs', '#[test] fn a(){}'))"
  [ "$output" = "True True True" ]
}

@test "negative proof: a source dir other crates include is NOT a test file, and a plain .rs without #[test] is not" {
  run python3 -c "$PY; print(t.is_test_file('platform/services/shared/a.rs', '#[test] fn a(){}'), t.is_test_file('platform/services/x/src/lib.rs', 'fn a(){}'))"
  [ "$output" = "False False" ]
}

@test "case names: escaped bats quotes unescaped, jest template names skipped, regex .test() is not a case, #[ignore] skipped" {
  printf '@test "the \\"quoted\\" one" {\n  true\n}\n' > "$F/a.bats"
  printf "it('plain one', () => {});\nit(\`port \${PORT}\`, () => {});\n/Log in/.test('<button>Log in</button>');\n" > "$F/b.test.ts"
  printf '#[test]\nfn keeps() {}\n#[test]\n#[ignore]\nfn skipped() {}\n' > "$F/c.rs"
  run python3 -c "$PY; import json; print(json.dumps([t.case_names('$F/a.bats')[0], t.case_names('$F/b.test.ts')[0], t.case_names('$F/c.rs')[0]]))"
  [ "$output" = '[["the \"quoted\" one"], ["plain one"], ["keeps"]]' ]
}

@test "classify: the authored @test-type header wins; the heuristic reads a live curl as integration/needs-stack" {
  run python3 -c "$PY; print(t.declared('# @test-type: integration:api\n'), t.classify('x.bats', 'curl -s http://localhost:3340/x'))"
  [ "$output" = "('integration', 'api') ('integration', 'needs-stack', None)" ]
}

# #4173 — the stub this case guarded is DELETED, not stubbed. A negative proof
# that a retired file "refuses to run" only holds while the file exists; once it
# is gone the guard becomes vacuous. The successor lives in
# 4173-crawler-retirement.bats: the walkers are gone from the tree, and that
# check ships its own proof that it can go red.
