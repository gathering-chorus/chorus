#!/usr/bin/env bats
# @test-type: unit — hermetic: runs the generator against a COPY of claudemd.
#
# #2311 origin: the Y auto-bump regression. REWRITTEN by #3904 for the #3288
# ledger contract, which retired auto-bump-on-generate entirely:
#
#   - version-ledger.json is the ONE home; PROTOCOL_VERSION is a rendered cache
#   - plain `generate` NEVER writes version state (AC5: a version change cannot
#     sit floating in canonical)
#   - `bump` mode classifies deterministically: core fragment → MINOR,
#     other constituent → PATCH, CLAUDEMD_MAJOR=1 → MAJOR
#
# The old suite asserted the retired X.Y auto-bump against LIVE canonical
# (mutating real role CLAUDE.mds — a #3528 violation) and died parsing "6.0"
# as an integer the day versions went semver. This suite brings its own world:
# a tmp copy of designing/claudemd with tmp role dirs.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
  GEN="$REPO/platform/scripts/claudemd-gen.py"
  W="$(mktemp -d)"
  mkdir -p "$W/designing" "$W/roles/silas" "$W/roles/wren" "$W/roles/kade"
  cp -R "$REPO/designing/claudemd" "$W/designing/claudemd"
  CLAUDEMD="$W/designing/claudemd"
}

teardown() { rm -rf "$W"; }

gen()  { python3 "$GEN" "$CLAUDEMD/manifest.json" "$CLAUDEMD" generate "" "#3904-test" >/dev/null 2>&1; }
bump() { python3 "$GEN" "$CLAUDEMD/manifest.json" "$CLAUDEMD" bump "" "#3904-test" >/dev/null 2>&1; }

stamp_version() {
  grep -oE '<!-- chorus-prompt: [0-9]+\.[0-9]+\.[0-9]+ -->' "$W/roles/$1/CLAUDE.md" \
    | head -1 | sed -E 's/<!-- chorus-prompt: (.+) -->/\1/'
}

@test "baseline: PROTOCOL_VERSION matches the stamp in all 3 roles (semver)" {
  gen
  disk=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  echo "$disk" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'
  for r in silas wren kade; do
    [ "$(stamp_version $r)" = "$disk" ]
  done
}

@test "AC5 NEGATIVE PROOF: plain generate NEVER writes version state, even with a mutated core fragment" {
  gen
  before=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  printf '\n<!-- ybump test marker -->\n' >> "$CLAUDEMD/shared/chorus-prompt.md"
  gen
  [ "$(cat "$CLAUDEMD/PROTOCOL_VERSION")" = "$before" ]
}

@test "bump: core-fragment change classifies MINOR and restamps all 3 roles" {
  gen
  before=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  bx="${before%%.*}"; rest="${before#*.}"; by="${rest%%.*}"
  printf '\n<!-- ybump test marker -->\n' >> "$CLAUDEMD/shared/chorus-prompt.md"
  bump
  after=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  [ "$after" = "${bx}.$((by + 1)).0" ]
  for r in silas wren kade; do
    [ "$(stamp_version $r)" = "$after" ]
  done
}

@test "bump appends the ledger (append-only home, version is its head)" {
  gen
  n_before=$(python3 -c "import json;print(len(json.load(open('$CLAUDEMD/version-ledger.json'))['entries']))")
  printf '\n<!-- ledger marker -->\n' >> "$CLAUDEMD/shared/chorus-prompt.md"
  bump
  n_after=$(python3 -c "import json;print(len(json.load(open('$CLAUDEMD/version-ledger.json'))['entries']))")
  [ "$n_after" -eq $((n_before + 1)) ]
  head_v=$(python3 -c "import json;print(json.load(open('$CLAUDEMD/version-ledger.json'))['entries'][-1]['version'])")
  [ "$head_v" = "$(cat "$CLAUDEMD/PROTOCOL_VERSION")" ]
}

@test "monotonic: reverting the fragment and bumping again still climbs, never resets" {
  gen
  cp "$CLAUDEMD/shared/chorus-prompt.md" "$W/orig-fragment"
  printf '\n<!-- marker A -->\n' >> "$CLAUDEMD/shared/chorus-prompt.md"
  bump
  v1=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  cp "$W/orig-fragment" "$CLAUDEMD/shared/chorus-prompt.md"
  bump
  v2=$(cat "$CLAUDEMD/PROTOCOL_VERSION")
  [ "$v1" != "$v2" ]
  python3 - "$v1" "$v2" <<'PYEOF'
import sys
a=tuple(map(int,sys.argv[1].split('.'))); b=tuple(map(int,sys.argv[2].split('.')))
sys.exit(0 if b>a else 1)
PYEOF
}
