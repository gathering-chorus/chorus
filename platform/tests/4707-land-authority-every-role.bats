#!/usr/bin/env bats
# @test-type: unit
# @domain: tests
# @card: #4707
# @owner: wren
# #4707 AC3 — the go rule reaches every role, whatever its model. The protocol
# core fragments in designing/claudemd/manifest.json must be in every role's
# sections, or platform/scripts/claudemd-gen.py refuses the manifest; the
# generated roles/abby-normal/AGENTS.md and roles/borg-queen/AGENTS.md carry
# designing/claudemd/shared/land-authority.md like every CLAUDE.md. The bump is
# recorded in designing/claudemd/PROTOCOL_VERSION and designing/claudemd/version-ledger.json.

setup() {
  ROOT="$BATS_TEST_DIRNAME/../.."
  GEN="$ROOT/platform/scripts/claudemd-gen.py"
  CM="$ROOT/designing/claudemd"
}

drop_from_role() {
  python3 -c '
import json, sys
m = json.load(open(sys.argv[1]))
m["roles"][sys.argv[2]]["sections"].remove(sys.argv[3])
json.dump(m, open(sys.argv[4], "w"))' "$CM/manifest.json" "$1" "$2" "$BATS_TEST_TMPDIR/manifest.json"
}

@test "the go rule is a protocol-core fragment, and the manifest validates" {
  python3 -c '
import json, sys
m = json.load(open(sys.argv[1]))
assert "shared/land-authority.md" in m["protocol_core"]' "$CM/manifest.json" || return 1
  run python3 "$GEN" "$CM/manifest.json" "$CM" validate
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

# CLAUDE.md files are generated at deploy and not tracked; AGENTS.md is committed.
@test "the committed AGENTS.md of each non-Claude role carries the go rule" {
  for f in roles/abby-normal/AGENTS.md roles/borg-queen/AGENTS.md; do
    grep -q "^## Only Jeff's Go Lands a Card" "$ROOT/$f" || { echo "missing in $f"; return 1; }
  done
}

@test "negative proof: a role without a protocol-core fragment is refused, naming role and fragment" {
  drop_from_role abby-normal shared/land-authority.md
  run python3 "$GEN" "$BATS_TEST_TMPDIR/manifest.json" "$CM" validate
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"Role 'abby-normal': protocol_core fragment missing from sections: shared/land-authority.md"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: the same holds for a Claude role" {
  drop_from_role kade shared/land-authority.md
  run python3 "$GEN" "$BATS_TEST_TMPDIR/manifest.json" "$CM" validate
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"Role 'kade': protocol_core fragment missing"* ]] || { echo "$output"; return 1; }
}
