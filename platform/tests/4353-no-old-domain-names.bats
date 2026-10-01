#!/usr/bin/env bats
# @test-type: fitness — greps tracked source for the retired Domain names; no store.
# @domain: domains — the Domain rows #4353 moved to their bare names
# @card: #4353
# @owner: wren
#
# #4353 step 4 moved 39 old-named Domain rows (cards-service, loom-principles,
# photos-domain …) onto their bare names. A source file that names one again would
# put it back in the store on the next deploy or harvest. History (backups, the
# retirement ledger, design docs, the one-shot #2516 migration) may still name them.
# It checks the IRI forms, the ones that reach the store; a bare 'cards-service'
# string is also a query-file name or a test example elsewhere.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
MAP="$ROOT/designing/schemas/4353-domain-renames.tsv"

old_names_in() {  # $1 = directory to scan; prints file:line for each hit
  local p
  p=$(grep -v '^legacy' "$MAP" | cut -f1 | paste -sd'|' -)
  [ -n "$p" ] || { echo "empty map"; return 2; }
  grep -rnE "(chorus:|chorus#)($p)([^-A-Za-z0-9_]|\$)" "$1" \
    --include='*.ttl' --include='*.ts' --include='*.rs' --include='*.js' --include='*.sh' \
    --exclude-dir=node_modules --exclude-dir=target --exclude-dir=backups --exclude-dir=recovery \
    --exclude-dir=migrations --exclude-dir=docs \
    --exclude='migrate-aliases-to-graph*' --exclude='witness-3025.mjs' --exclude='4353-*' || true
}

@test "no tracked source names a retired Domain row" {
  [ -r "$MAP" ]
  run old_names_in "$ROOT/platform"
  [ -z "$output" ] || { echo "$output"; false; }
  run old_names_in "$ROOT/roles"
  [ -z "$output" ] || { echo "$output"; false; }
  run old_names_in "$ROOT/designing/data"
  [ -z "$output" ] || { echo "$output"; false; }
}

@test "NEGATIVE PROOF: a file that names an old row is caught" {
  d="$(mktemp -d)"
  printf 'chorus:gathering chorus:hasDomain chorus:photos-domain .\n' > "$d/x.ttl"
  printf "const d = 'https://jeffbridwell.com/chorus#loom-principles';\n" > "$d/y.ts"
  printf 'chorus:photos chorus:consumes chorus:security .\n' > "$d/z.ttl"
  run old_names_in "$d"
  rm -rf "$d"
  [[ "$output" == *"x.ttl"* ]] || false
  [[ "$output" == *"y.ts"* ]] || false
  [[ "$output" != *"z.ttl"* ]] || false
}
