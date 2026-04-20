#!/usr/bin/env bats
# #2311 — Boot-time protocol contract stamps in CLAUDE.md headers.
#
# AC covered by this file:
#   - Protocol contract: schema version (chorus-prompt/X.Y) + fragment-set
#     content hash stamped into CLAUDE.md header, machine-readable.
#   - All three roles declare identical chorus-prompt/X.Y and identical
#     protocol-core hash at any given moment.
#   - role-fragments hash differs across roles (each role loads different sections).
#
# These tests assert what Jeff sees when he opens any role's CLAUDE.md:
# three machine-readable stamp lines in the header. Before this card, none exist.

CHORUS_ROOT="/Users/jeffbridwell/CascadeProjects/chorus"
MANIFEST="$CHORUS_ROOT/designing/claudemd/manifest.json"
CLAUDEMD_DIR="$CHORUS_ROOT/designing/claudemd"
WREN="$CHORUS_ROOT/roles/wren/CLAUDE.md"
SILAS="$CHORUS_ROOT/roles/silas/CLAUDE.md"
KADE="$CHORUS_ROOT/roles/kade/CLAUDE.md"

setup() {
  # Regenerate with the real manifest so we test current output, not stale files
  python3 "$CHORUS_ROOT/platform/scripts/claudemd-gen.py" \
    "$MANIFEST" "$CLAUDEMD_DIR" generate "" "#2311-test" >/dev/null 2>&1 || true
}

extract_stamp() {
  local file="$1" key="$2"
  grep -oE "<!-- ${key}: [^ ]+ -->" "$file" | head -1 | sed -E "s/<!-- ${key}: (.+) -->/\\1/"
}

@test "manifest declares protocol_core fragment set" {
  run python3 -c "import json; m = json.load(open('$MANIFEST')); print(len(m.get('protocol_core', [])))"
  [ "$status" -eq 0 ]
  [ "$output" -ge 10 ]
}

@test "PROTOCOL_VERSION seed file exists and matches X.Y format" {
  [ -f "$CLAUDEMD_DIR/PROTOCOL_VERSION" ]
  run cat "$CLAUDEMD_DIR/PROTOCOL_VERSION"
  [[ "$output" =~ ^[0-9]+\.[0-9]+$ ]]
}

@test "each role CLAUDE.md carries chorus-prompt stamp" {
  for f in "$WREN" "$SILAS" "$KADE"; do
    run grep -E "<!-- chorus-prompt: [0-9]+\.[0-9]+ -->" "$f"
    [ "$status" -eq 0 ]
  done
}

@test "each role CLAUDE.md carries protocol-core sha256 stamp" {
  for f in "$WREN" "$SILAS" "$KADE"; do
    run grep -E "<!-- protocol-core: sha256=[a-f0-9]{64} -->" "$f"
    [ "$status" -eq 0 ]
  done
}

@test "each role CLAUDE.md carries role-fragments sha256 stamp" {
  for f in "$WREN" "$SILAS" "$KADE"; do
    run grep -E "<!-- role-fragments: sha256=[a-f0-9]{64} -->" "$f"
    [ "$status" -eq 0 ]
  done
}

@test "all three roles declare identical chorus-prompt version" {
  w=$(extract_stamp "$WREN" "chorus-prompt")
  s=$(extract_stamp "$SILAS" "chorus-prompt")
  k=$(extract_stamp "$KADE" "chorus-prompt")
  [ -n "$w" ]
  [ "$w" = "$s" ]
  [ "$s" = "$k" ]
}

@test "all three roles declare identical protocol-core hash (same core by construction)" {
  w=$(extract_stamp "$WREN" "protocol-core")
  s=$(extract_stamp "$SILAS" "protocol-core")
  k=$(extract_stamp "$KADE" "protocol-core")
  [ -n "$w" ]
  [ "$w" = "$s" ]
  [ "$s" = "$k" ]
}

@test "role-fragments hashes differ across roles (role sections diverge)" {
  w=$(extract_stamp "$WREN" "role-fragments")
  s=$(extract_stamp "$SILAS" "role-fragments")
  k=$(extract_stamp "$KADE" "role-fragments")
  [ -n "$w" ]
  [ -n "$s" ]
  [ -n "$k" ]
  [ "$w" != "$s" ]
  [ "$s" != "$k" ]
  [ "$w" != "$k" ]
}
