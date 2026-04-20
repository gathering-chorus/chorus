#!/usr/bin/env bats
# #2311 — Protocol contract regression: staged-state mutation tests.
#
# AC: "Regression test — stage all three role CLAUDE.md files to a known hash,
#      mutate one fragment, assert that the stale role(s) fail the contract
#      check and the unchanged role(s) pass."
#
# Approach: work at the hash level, not the Rust binary level. The contract
# check is "stamp in CLAUDE.md == computed live hash." We can assert that
# directly from bats without shelling into the Rust hook — which keeps this
# test portable (no cargo dependency) while still proving the invariant.

CHORUS_ROOT="/Users/jeffbridwell/CascadeProjects/chorus"
CLAUDEMD="$CHORUS_ROOT/designing/claudemd"

# Python one-shot hash over a list of fragment paths (matches
# claudemd-gen.py _hash_fragment_set and Rust protocol_contract::hash_entries).
hash_set() {
  python3 - "$@" <<'PY'
import hashlib, sys, os
claudemd = "/Users/jeffbridwell/CascadeProjects/chorus/designing/claudemd"
paths = sys.argv[1:]
h = hashlib.sha256()
for rel in sorted(paths):
    with open(os.path.join(claudemd, rel), "rb") as f:
        content = f.read()
    h.update(rel.encode("utf-8"))
    h.update(b"\0")
    h.update(hashlib.sha256(content).hexdigest().encode("ascii"))
    h.update(b"\0")
print(h.hexdigest())
PY
}

# Extract stamp value for a given key from a role's CLAUDE.md.
stamp() {
  local role="$1" key="$2"
  grep -oE "<!-- ${key}: [^ ]+ -->" "$CHORUS_ROOT/roles/${role}/CLAUDE.md" \
    | head -1 | sed -E "s/<!-- ${key}: (.+) -->/\\1/" | sed 's/^sha256=//'
}

# Manifest section list for a role (JSON array → newline-delimited paths).
role_sections() {
  local role="$1"
  python3 -c "
import json
m = json.load(open('$CLAUDEMD/manifest.json'))
print('\n'.join(m['roles']['$role']['sections']))
"
}

core_paths() {
  python3 -c "
import json
m = json.load(open('$CLAUDEMD/manifest.json'))
print('\n'.join(m['protocol_core']))
"
}

# Stage: regenerate CLAUDE.md for all 3 roles so stamps == current fragments.
setup() {
  python3 "$CHORUS_ROOT/platform/scripts/claudemd-gen.py" \
    "$CLAUDEMD/manifest.json" "$CLAUDEMD" generate "" "#2311-regression" >/dev/null 2>&1 || true
}

@test "baseline: all three roles pass contract (stamps == live hashes)" {
  live_core=$(hash_set $(core_paths))
  for role in silas wren kade; do
    [ "$(stamp $role protocol-core)" = "$live_core" ]
    live_frag=$(hash_set $(role_sections $role))
    [ "$(stamp $role role-fragments)" = "$live_frag" ]
  done
}

@test "mutating a role-specific fragment stales only that role" {
  target="$CLAUDEMD/roles/kade/state-files.md"
  backup=$(mktemp)
  cp "$target" "$backup"

  # Mutate kade's role-specific fragment
  printf '\n<!-- regression marker -->\n' >> "$target"

  # Recompute live hashes (without regenerating CLAUDE.md — that's the whole point:
  # the stamp in CLAUDE.md is frozen, the live fragments have drifted).
  live_core=$(hash_set $(core_paths))
  live_kade_frag=$(hash_set $(role_sections kade))
  live_silas_frag=$(hash_set $(role_sections silas))
  live_wren_frag=$(hash_set $(role_sections wren))

  # Protocol core untouched → all three protocol-core stamps still valid
  for role in silas wren kade; do
    [ "$(stamp $role protocol-core)" = "$live_core" ]
  done

  # Kade's role-fragments stamp no longer matches (STALE for kade only)
  [ "$(stamp kade role-fragments)" != "$live_kade_frag" ]

  # Silas + Wren role-fragments still match (unchanged roles pass)
  [ "$(stamp silas role-fragments)" = "$live_silas_frag" ]
  [ "$(stamp wren role-fragments)" = "$live_wren_frag" ]

  # Restore
  mv "$backup" "$target"
}

@test "mutating a core fragment version-mismatches all three roles" {
  target="$CLAUDEMD/shared/chorus-prompt.md"
  backup=$(mktemp)
  cp "$target" "$backup"

  # Mutate a protocol-core fragment
  printf '\n<!-- regression marker -->\n' >> "$target"

  live_core=$(hash_set $(core_paths))

  # Every role's stamped protocol-core hash is now stale (VERSION_MISMATCH)
  for role in silas wren kade; do
    [ "$(stamp $role protocol-core)" != "$live_core" ]
  done

  # Restore
  mv "$backup" "$target"
}

@test "restoring the fragment re-passes all checks" {
  # Post-mutation sanity: after setup() regenerates, everything is aligned again.
  live_core=$(hash_set $(core_paths))
  for role in silas wren kade; do
    [ "$(stamp $role protocol-core)" = "$live_core" ]
    live_frag=$(hash_set $(role_sections $role))
    [ "$(stamp $role role-fragments)" = "$live_frag" ]
  done
}
