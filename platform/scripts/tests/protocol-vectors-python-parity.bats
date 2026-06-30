#!/usr/bin/env bats
# @test-type: unit
# #2311 — Python side of the cross-language parity contract.
#
# The Rust hook's sibling test asserts identical digests. Any canonicalization
# drift in EITHER language fails this contract and forces a joint re-agreement.
#
# Fixture sources: designing/claudemd/.protocol_test_vectors.json

CHORUS_ROOT="/Users/jeffbridwell/CascadeProjects/chorus"
VECTORS="$CHORUS_ROOT/designing/claudemd/.protocol_test_vectors.json"

@test "vector file exists and has version 1" {
  [ -f "$VECTORS" ]
  run python3 -c "import json; print(json.load(open('$VECTORS'))['version'])"
  [ "$output" = "1" ]
}

@test "empty-set fixture matches canonical sha256 of empty input" {
  run python3 -c "
import hashlib, json
v = json.load(open('$VECTORS'))
empty = [f for f in v['fixtures'] if f['name']=='empty'][0]
assert empty['expected_core_hash'] == hashlib.sha256().hexdigest(), empty['expected_core_hash']
print('ok')
"
  [ "$status" -eq 0 ]
}

@test "known-content fixture recomputes to the stored hash" {
  run python3 -c "
import hashlib, json
v = json.load(open('$VECTORS'))
known = [f for f in v['fixtures'] if f['name']=='known_content'][0]
h = hashlib.sha256()
for rel, content in sorted(known['files'].items()):
    h.update(rel.encode('utf-8'))
    h.update(b'\0')
    h.update(hashlib.sha256(content.encode('utf-8')).hexdigest().encode('ascii'))
    h.update(b'\0')
assert h.hexdigest() == known['expected_core_hash'], (h.hexdigest(), known['expected_core_hash'])
print('ok')
"
  [ "$status" -eq 0 ]
}

# NOTE: the "live_core fixture" test was retired (#3598) — the live_core fixture
# was removed from .protocol_test_vectors.json (only empty/known_content remain),
# so the test IndexError'd every run. Retired with its fixture, not left as rot.

# NOTE: the "vector core_paths matches manifest protocol_core" test was retired
# (#3598, confirmed by Wren as claudemd owner). #3254 deliberately removed the
# core_paths pin from .protocol_test_vectors.json — pinning a live-content path
# set cried wolf on every CLAUDE.md regen. Whether the live doc matches its
# fragments is the RUNTIME guard's job, not this unit test's. Re-adding core_paths
# would re-introduce the exact churn #3254 killed. Retired with its field.
