#!/usr/bin/env bats
# @test-type: unit
# #3288 — Werk version derives from ONE source: version-ledger.json.
#
# AC covered by this file (hermetic — every test builds its own fixture
# claudemd dir under $BATS_TEST_TMPDIR; no live-repo reads or writes):
#   AC1: version derives from the ledger (constituent hash → head lookup);
#        PROTOCOL_VERSION is a rendered cache the generator writes, and the
#        rendered header reads the same number by construction.
#   AC2: semver MAJOR.MINOR.PATCH with deterministic position: protocol_core
#        change → MINOR, non-core change → PATCH, MAJOR only via explicit
#        CLAUDEMD_MAJOR=1.
#   AC4: `check-version` is the coherence gate (CI), replacing the runtime
#        stamp-compare: fails when fragments changed without a bump, or when
#        PROTOCOL_VERSION disagrees with the ledger head.
#   AC5: a plain generate NEVER writes version state (ledger or
#        PROTOCOL_VERSION) — only `bump` does, so a version change cannot
#        sit floating in canonical from a session-start regen.

WERK_ROOT="$(cd "$BATS_TEST_DIRNAME/../../.." && pwd)"
GEN="$WERK_ROOT/platform/scripts/claudemd-gen.py"

setup() {
  # Mirror the repo layout: output paths in the manifest are ../../roles/...
  # relative to the claudemd dir, so nest it two levels under a repo root.
  REPO="$BATS_TEST_TMPDIR/repo"
  FIX="$REPO/designing/claudemd"
  mkdir -p "$FIX/shared" "$FIX/roles/testrole" "$REPO/roles/testrole"
  cat > "$FIX/shared/core.md" <<'EOF'
# Core Protocol
Werk v{{CHORUS_PROMPT_VERSION}}
EOF
  cat > "$FIX/roles/testrole/title.md" <<'EOF'
# Test Role
EOF
  cat > "$FIX/manifest.json" <<EOF
{
  "_build": "1",
  "protocol_core": ["shared/core.md"],
  "variables": {"testrole": {"ROLE_NAME": "Test"}},
  "roles": {
    "testrole": {
      "output": "../../roles/testrole/CLAUDE.md",
      "sections": ["roles/testrole/title.md", "shared/core.md"]
    }
  }
}
EOF
  # cwd-independent invocation; fixture output lands inside the tmpdir
  cd "$BATS_TEST_TMPDIR"
}

gen() { # gen <mode> [card]
  python3 "$GEN" "$FIX/manifest.json" "$FIX" "$1" "" "${2:-}"
}

seed_ledger() { # first bump seeds the ledger from the PROTOCOL_VERSION literal
  echo "1.4" > "$FIX/PROTOCOL_VERSION"
  gen bump "#seed" >/dev/null 2>&1
}

@test "AC1: bump seeds the ledger and PROTOCOL_VERSION becomes semver" {
  seed_ledger
  [ -f "$FIX/version-ledger.json" ]
  run cat "$FIX/PROTOCOL_VERSION"
  [[ "$output" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

@test "AC1: rendered header shows the ledger-head version" {
  seed_ledger
  gen generate >/dev/null 2>&1
  ver=$(cat "$FIX/PROTOCOL_VERSION")
  run grep "Werk v${ver}" "$REPO/roles/testrole/CLAUDE.md"
  [ "$status" -eq 0 ]
}

@test "AC2: non-core fragment change bumps PATCH" {
  seed_ledger
  before=$(cat "$FIX/PROTOCOL_VERSION")
  echo "extra role line" >> "$FIX/roles/testrole/title.md"
  gen bump "#test" >/dev/null 2>&1
  after=$(cat "$FIX/PROTOCOL_VERSION")
  IFS=. read -r bx by bz <<< "$before"
  IFS=. read -r ax ay az <<< "$after"
  [ "$ax" = "$bx" ]
  [ "$ay" = "$by" ]
  [ "$az" = "$((bz + 1))" ]
}

@test "AC2: protocol_core fragment change bumps MINOR" {
  seed_ledger
  before=$(cat "$FIX/PROTOCOL_VERSION")
  echo "core protocol change" >> "$FIX/shared/core.md"
  gen bump "#test" >/dev/null 2>&1
  after=$(cat "$FIX/PROTOCOL_VERSION")
  IFS=. read -r bx by bz <<< "$before"
  IFS=. read -r ax ay az <<< "$after"
  [ "$ax" = "$bx" ]
  [ "$ay" = "$((by + 1))" ]
  [ "$az" = "0" ]
}

@test "AC2: MAJOR only via explicit CLAUDEMD_MAJOR=1" {
  seed_ledger
  before=$(cat "$FIX/PROTOCOL_VERSION")
  echo "breaking change" >> "$FIX/shared/core.md"
  CLAUDEMD_MAJOR=1 gen bump "#test" >/dev/null 2>&1
  after=$(cat "$FIX/PROTOCOL_VERSION")
  IFS=. read -r bx _ _ <<< "$before"
  [ "$after" = "$((bx + 1)).0.0" ]
}

@test "AC4: check-version passes when committed state is coherent" {
  seed_ledger
  run gen check-version
  [ "$status" -eq 0 ]
}

@test "AC4: check-version fails when fragments changed without a bump" {
  seed_ledger
  echo "drifted" >> "$FIX/shared/core.md"
  run gen check-version
  [ "$status" -eq 1 ]
  [[ "$output" == *"without"*"bump"* ]]
}

@test "AC4: check-version fails when PROTOCOL_VERSION is hand-edited off the ledger" {
  seed_ledger
  echo "9.9.9" > "$FIX/PROTOCOL_VERSION"
  run gen check-version
  [ "$status" -eq 1 ]
  [[ "$output" == *"ledger head"* ]]
}

@test "AC5: plain generate never writes ledger or PROTOCOL_VERSION" {
  seed_ledger
  ledger_before=$(cat "$FIX/version-ledger.json")
  ver_before=$(cat "$FIX/PROTOCOL_VERSION")
  echo "changed after seed" >> "$FIX/shared/core.md"
  gen generate >/dev/null 2>&1
  [ "$(cat "$FIX/version-ledger.json")" = "$ledger_before" ]
  [ "$(cat "$FIX/PROTOCOL_VERSION")" = "$ver_before" ]
}

@test "AC2: same constituents twice → no second ledger entry (idempotent bump)" {
  seed_ledger
  entries_before=$(python3 -c "import json; print(len(json.load(open('$FIX/version-ledger.json'))['entries']))")
  gen bump "#test" >/dev/null 2>&1
  entries_after=$(python3 -c "import json; print(len(json.load(open('$FIX/version-ledger.json'))['entries']))")
  [ "$entries_after" = "$entries_before" ]
}
