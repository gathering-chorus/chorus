#!/usr/bin/env bats
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

@test "live_core fixture matches hash stamped in role CLAUDE.md headers" {
  stamp=$(grep -oE 'protocol-core: sha256=[a-f0-9]+' "$CHORUS_ROOT/roles/kade/CLAUDE.md" | head -1 | cut -d= -f2)
  vector=$(python3 -c "
import json
v = json.load(open('$VECTORS'))
print([f for f in v['fixtures'] if f['name']=='live_core'][0]['expected_core_hash'])
")
  [ "$stamp" = "$vector" ]
}

@test "vector core_paths matches manifest protocol_core" {
  run python3 -c "
import json
v = json.load(open('$VECTORS'))
m = json.load(open('$CHORUS_ROOT/designing/claudemd/manifest.json'))
assert sorted(v['core_paths']) == sorted(m['protocol_core']), (v['core_paths'], m['protocol_core'])
print('ok')
"
  [ "$status" -eq 0 ]
}
