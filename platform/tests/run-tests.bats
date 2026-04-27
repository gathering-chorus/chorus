#!/usr/bin/env bats
# run-tests.bats — Phase 1 contract surface for platform/scripts/run-tests (#2118).
# Wave 1: flag parsing + exit codes + skeleton JSON. No real test execution yet
# (RUN_TESTS_FAKE=1 short-circuits to canned outputs).

RUN_TESTS="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/run-tests"

setup() {
  export RUN_TESTS_FAKE=1
}

@test "exists and is executable" {
  [ -x "$RUN_TESTS" ]
}

@test "missing --domain exits 3 (invocation error)" {
  run "$RUN_TESTS" --category=hermetic --budget=10
  [ "$status" -eq 3 ]
}

@test "missing --category exits 3" {
  run "$RUN_TESTS" --domain=cards-service --budget=10
  [ "$status" -eq 3 ]
}

@test "missing --budget exits 3" {
  run "$RUN_TESTS" --domain=cards-service --category=hermetic
  [ "$status" -eq 3 ]
}

@test "fake-pass mode exits 0" {
  RUN_TESTS_FAKE=pass run "$RUN_TESTS" --domain=cards-service --category=hermetic --budget=10
  [ "$status" -eq 0 ]
}

@test "fake-fail mode exits 1" {
  RUN_TESTS_FAKE=fail run "$RUN_TESTS" --domain=cards-service --category=hermetic --budget=10
  [ "$status" -eq 1 ]
}

@test "fake-budget mode exits 2 (budget exceeded)" {
  RUN_TESTS_FAKE=budget run "$RUN_TESTS" --domain=cards-service --category=hermetic --budget=1
  [ "$status" -eq 2 ]
}

@test "stdout is structured JSON with required keys" {
  RUN_TESTS_FAKE=pass run "$RUN_TESTS" --domain=cards-service --category=hermetic --budget=10
  echo "$output" | python3 -c "
import json,sys
d=json.load(sys.stdin)
required=['pass','fail','runtime_ms','partial','failing_tests','tests_unrun','hermeticity_tags']
missing=[k for k in required if k not in d]
assert not missing, f'missing keys: {missing}'
assert isinstance(d['failing_tests'],list)
assert isinstance(d['tests_unrun'],list)
"
}

@test "fake-budget sets partial:true and populates tests_unrun" {
  RUN_TESTS_FAKE=budget run "$RUN_TESTS" --domain=cards-service --category=hermetic --budget=1
  echo "$output" | python3 -c "
import json,sys
d=json.load(sys.stdin)
assert d['partial'] is True, 'partial must be true on budget exit'
assert len(d['tests_unrun']) > 0, 'tests_unrun must list at least one'
assert all('name' in t and 'file' in t for t in d['tests_unrun']), 'tests_unrun shape'
"
}

@test "fake-fail emits failing_tests with name/file/error structure" {
  RUN_TESTS_FAKE=fail run "$RUN_TESTS" --domain=cards-service --category=hermetic --budget=10
  echo "$output" | python3 -c "
import json,sys
d=json.load(sys.stdin)
assert d['fail'] > 0
assert len(d['failing_tests']) > 0
for t in d['failing_tests']:
    assert 'name' in t and 'file' in t and 'error' in t, f'failing_test shape: {t}'
"
}

@test "category accepts comma-separated union" {
  RUN_TESTS_FAKE=pass run "$RUN_TESTS" --domain=cards-service --category=hermetic,integration --budget=10
  [ "$status" -eq 0 ]
}

@test "bad category value exits 3" {
  RUN_TESTS_FAKE=pass run "$RUN_TESTS" --domain=cards-service --category=garbage --budget=10
  [ "$status" -eq 3 ]
}
