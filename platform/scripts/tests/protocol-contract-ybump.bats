#!/usr/bin/env bats
# #2311 — Y auto-bump regression.
#
# The bug Jeff caught in demo: hash flipped but chorus-prompt stayed at 2.0
# because the plain `generate` path never persisted _protocol_core_hash, so
# the auto-bump's "stored != current" check was always False on first compare.
# Silas's fix adds the persist block at the plain-gen checksum-save site.
#
# This test pins that behavior: across multiple regens with core mutation,
# Y must increment monotonically and the stamped chorus-prompt version in
# each role's CLAUDE.md must match PROTOCOL_VERSION on disk.

CHORUS_ROOT="/Users/jeffbridwell/CascadeProjects/chorus"
CLAUDEMD="$CHORUS_ROOT/designing/claudemd"
GEN="$CHORUS_ROOT/platform/scripts/claudemd-gen.py"

# Snapshot mutable state so a failed test doesn't leave the repo drifted.
setup() {
  PV_BACKUP=$(mktemp); cp "$CLAUDEMD/PROTOCOL_VERSION" "$PV_BACKUP"
  CK_BACKUP=$(mktemp); cp "$CLAUDEMD/.checksums.json"  "$CK_BACKUP"
  export PV_BACKUP CK_BACKUP
}

teardown() {
  [ -n "$PV_BACKUP" ] && cp "$PV_BACKUP" "$CLAUDEMD/PROTOCOL_VERSION" && rm "$PV_BACKUP"
  [ -n "$CK_BACKUP" ] && cp "$CK_BACKUP" "$CLAUDEMD/.checksums.json"  && rm "$CK_BACKUP"
  # Re-regen so role CLAUDE.md stamps match the restored state.
  python3 "$GEN" "$CLAUDEMD/manifest.json" "$CLAUDEMD" generate "" "#2311-ybump-teardown" >/dev/null 2>&1 || true
}

regen() {
  python3 "$GEN" "$CLAUDEMD/manifest.json" "$CLAUDEMD" generate "" "#2311-ybump" >/dev/null 2>&1
}

stamp_version() {
  grep -oE '<!-- chorus-prompt: [0-9]+\.[0-9]+ -->' "$CHORUS_ROOT/roles/$1/CLAUDE.md" \
    | head -1 | sed -E 's/<!-- chorus-prompt: (.+) -->/\1/'
}

@test "baseline: PROTOCOL_VERSION on disk matches stamp in all 3 roles" {
  regen
  disk=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  for r in silas wren kade; do
    [ "$(stamp_version $r)" = "$disk" ]
  done
}

@test "mutating a core fragment auto-bumps Y and restamps all 3 roles" {
  # Seed: establish baseline persisted _protocol_core_hash
  regen
  before=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  before_x="${before%.*}"
  before_y="${before#*.}"

  # Mutate a protocol-core fragment
  target="$CLAUDEMD/shared/chorus-prompt.md"
  backup=$(mktemp); cp "$target" "$backup"
  printf '\n<!-- ybump test marker -->\n' >> "$target"

  regen

  after=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  after_x="${after%.*}"
  after_y="${after#*.}"

  # X unchanged, Y bumped by exactly 1
  [ "$after_x" = "$before_x" ]
  [ "$after_y" -eq $((before_y + 1)) ]

  # All 3 role stamps now at the new version
  for r in silas wren kade; do
    [ "$(stamp_version $r)" = "$after" ]
  done

  mv "$backup" "$target"
}

@test "restoring the fragment still bumps Y (monotonic, not reset)" {
  regen
  v0=$(cat "$CLAUDEMD/PROTOCOL_VERSION")

  target="$CLAUDEMD/shared/chorus-prompt.md"
  backup=$(mktemp); cp "$target" "$backup"

  printf '\n<!-- ybump test marker -->\n' >> "$target"
  regen
  v1=$(cat "$CLAUDEMD/PROTOCOL_VERSION")

  mv "$backup" "$target"
  regen
  v2=$(cat "$CLAUDEMD/PROTOCOL_VERSION")

  y0="${v0#*.}"; y1="${v1#*.}"; y2="${v2#*.}"
  [ "$y1" -eq $((y0 + 1)) ]
  [ "$y2" -eq $((y1 + 1)) ]
  # Specifically: v2 is NOT equal to v0, even though fragments match v0.
  [ "$v2" != "$v0" ]
}
