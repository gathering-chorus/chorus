#!/usr/bin/env bats
# @test-type: unit — runs the migration's SPARQL with Jena `update` on a fixture dataset; no store, no network.
# @domain: domains — the Domain rows this migration moves (#4353)
# @card: #4353
# @owner: wren
#
# #4353 step 4 — the old-named Domain rows move onto their bare names. These
# describe what the store looks like after the move, on a small fixture that has
# one merge (cards-service → cards, cards already exists), one rename
# (photos-domain → photos, nothing there yet), and a triple naming two old rows.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$ROOT/platform/scripts/4353-domain-rename.sh"

setup() {
  command -v update >/dev/null || skip "Jena update not on PATH"
  T="$(mktemp -d)"
  printf 'legacy\ttarget\tkind\ncards-service\tcards\tmerge\nphotos-domain\tphotos\trename\ncode-domain\tcode\tmerge\ntime-domain\ttime\tmerge\n' > "$T/map.tsv"
  cat > "$T/before.trig" <<'EOF'
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<urn:chorus:ontology> {
  c:cards a c:Domain ; rdfs:label "Cards" ; c:ownedBy c:principal-wren .
  c:code a c:Domain ; rdfs:label "Code" .
  c:time a c:Domain ; rdfs:label "Time" .
}
<urn:chorus:domains:domains> {
  c:cards-service a c:Domain ; rdfs:label "Cards Service" ; c:ownedBy c:principal-kade ;
    c:consumes c:security ; c:hosts c:service-cards .
  c:photos-domain a c:Domain ; rdfs:label "Photos" ; c:ownedBy c:principal-jeff ; c:consumes c:security .
  c:code-domain a c:Domain ; c:consumes c:time-domain .
  c:time-domain a c:Domain ; rdfs:label "Time (old)" .
}
<urn:chorus:documents> { c:doc-1 c:hasDomain c:cards-service . }
<urn:chorus:domains:products> { c:gathering c:hasDomain c:photos-domain . }
EOF
}

teardown() { rm -rf "$T"; }

run_move() {  # $1 = the update to run (defaults to the script's own)
  local u="${1:-$T/update.ru}"
  [ -n "${1:-}" ] || RENAME_MAP="$T/map.tsv" bash "$SCRIPT" --print-sparql > "$u"
  update --data="$T/before.trig" --update="$u" --dump > "$T/after.trig"
  riot --syntax=trig --output=nq "$T/after.trig" > "$T/after.nq"
}

has()   { grep -qF -- "$1" "$T/after.nq"; }
count() { grep -cF -- "$1" "$T/after.nq" || true; }

# The checks the cases share. A dump that still names an old row, or gives a
# merged row two labels, fails here.
after_is_clean() {
  for old in cards-service photos-domain code-domain time-domain; do
    if has "chorus#$old>"; then echo "still names $old"; return 1; fi
  done
  [ "$(count '<https://jeffbridwell.com/chorus#cards> <http://www.w3.org/2000/01/rdf-schema#label>')" -eq 1 ] \
    || { echo "cards has more than one label"; return 1; }
}

@test "nothing names an old row afterwards, and a merged row keeps one label" {
  run_move
  run after_is_clean
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}

@test "merge: the existing row keeps its label and owner, and gains the old row's edges" {
  run_move
  has '<https://jeffbridwell.com/chorus#cards> <http://www.w3.org/2000/01/rdf-schema#label> "Cards"'
  if has '"Cards Service"'; then false; fi
  has '<https://jeffbridwell.com/chorus#cards> <https://jeffbridwell.com/chorus#ownedBy> <https://jeffbridwell.com/chorus#principal-wren>'
  if has 'principal-kade'; then false; fi
  has '<https://jeffbridwell.com/chorus#cards> <https://jeffbridwell.com/chorus#consumes> <https://jeffbridwell.com/chorus#security> <urn:chorus:domains:domains>'
  has '<https://jeffbridwell.com/chorus#cards> <https://jeffbridwell.com/chorus#hosts> <https://jeffbridwell.com/chorus#service-cards>'
}

@test "rename: the row moves whole to the bare name" {
  run_move
  has '<https://jeffbridwell.com/chorus#photos> <http://www.w3.org/2000/01/rdf-schema#label> "Photos" <urn:chorus:domains:domains>'
  has '<https://jeffbridwell.com/chorus#photos> <https://jeffbridwell.com/chorus#ownedBy> <https://jeffbridwell.com/chorus#principal-jeff>'
}

@test "what pointed at an old row points at the new one, in its own graph" {
  run_move
  has '<https://jeffbridwell.com/chorus#doc-1> <https://jeffbridwell.com/chorus#hasDomain> <https://jeffbridwell.com/chorus#cards> <urn:chorus:documents>'
  has '<https://jeffbridwell.com/chorus#gathering> <https://jeffbridwell.com/chorus#hasDomain> <https://jeffbridwell.com/chorus#photos> <urn:chorus:domains:products>'
  has '<https://jeffbridwell.com/chorus#code> <https://jeffbridwell.com/chorus#consumes> <https://jeffbridwell.com/chorus#time>'
}

@test "NEGATIVE PROOF: the checks fail on the store before the move" {
  riot --output=nq "$T/before.trig" > "$T/after.nq"
  run after_is_clean
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: a move without the target-wins rule gives the merged row two labels" {
  RENAME_MAP="$T/map.tsv" bash "$SCRIPT" --print-sparql | sed '/FILTER NOT EXISTS/d' > "$T/naive.ru"
  run_move "$T/naive.ru"
  run after_is_clean
  [ "$status" -ne 0 ]
  [[ "$output" == *"more than one label"* ]] || false
}
