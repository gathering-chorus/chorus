#!/usr/bin/env bats
# @test-type: contract — the garden-observability repo, read from disk; its own check scripts are the proof
# @domain: monitors — garden-observability, the second product Chorus builds outside itself (#4836)
#
# The garden ontology on public vocabularies: the 2024 soil tests as SOSA observations
# mapped to GloSIS. Every check runs a script that lives in the garden repo, so the repo
# proves itself with Chorus stopped. A missing repo, script or PDF folder is a FAIL,
# never a skip (#3734). Each negative test breaks a temp copy and the check must go red.
# Needs on the box: Apache Jena (riot, shacl) and pdftotext.
# SOIL_PDF_DIR (default ~/garden-soil-pdfs) holds the seven UMass PDFs; they carry a
# name and address, so they stay out of both repos. An account without them goes red.

load test_helper

# Before the land, the card's work is on branch wren/4836 in its own garden worktree;
# after Jeff's go merges it, the garden main checkout carries it. Both sit beside chorus.
PARENT="$(dirname "$CHORUS_ROOT")"
if [ "$(basename "$PARENT")" = "chorus-werk" ]; then PROJECTS="$(dirname "$PARENT")"; else PROJECTS="$PARENT"; fi
GARDEN_MAIN="$PROJECTS/garden-observability"
GARDEN_WERK="${CHORUS_WERK_BASE:-$PROJECTS/chorus-werk}/wren-4836-garden"
if [ -z "${GARDEN_REPO:-}" ]; then
  if [ -d "$GARDEN_WERK" ]; then GARDEN_REPO="$GARDEN_WERK"; else GARDEN_REPO="$GARDEN_MAIN"; fi
fi
export SOIL_PDF_DIR="${SOIL_PDF_DIR:-$HOME/garden-soil-pdfs}"

# A scratch copy of designing/ the negative tests may break.
copy_designing() {
  [ -x "$GARDEN_REPO/designing/tests/check-soil.sh" ] || { echo "missing: designing/tests/check-soil.sh"; return 1; }
  W="$BATS_TEST_TMPDIR/g"; mkdir -p "$W"; cp -R "$GARDEN_REPO/designing" "$W/"
}

@test "AC1+AC2+AC6: the soil seed loads as SOSA, matches the PDFs, maps to GloSIS, carries no personal details" {
  [ -x "$GARDEN_REPO/designing/tests/check-soil.sh" ] || { echo "missing: designing/tests/check-soil.sh"; false; }
  run "$GARDEN_REPO/designing/tests/check-soil.sh"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"ok   all 133 values match the lab PDFs"* ]] || return 1
  [[ "$output" == *"ok   GloSIS: 14 mapped, 5 gaps listed"* ]] || return 1
  [[ "$output" == *"ok   exactly 6 rows refused"* ]] || return 1
}

@test "AC1 negative: one value off from its PDF fails the check" {
  copy_designing
  sed -i '' 's/^S240318-226,Back Yard,zone-back,50,soil-cec,13.6$/S240318-226,Back Yard,zone-back,50,soil-cec,31.6/' "$W/designing/data/seed/soil-2024.csv"
  grep -q 'soil-cec,31.6$' "$W/designing/data/seed/soil-2024.csv"
  python3 "$W/designing/data/seed/build-soil.py" "$W/designing/data/seed/soil-2024.csv" > "$W/designing/data/seed/soil-2024.ttl"
  run "$W/designing/tests/check-soil.sh"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" == *"mismatch S240318-226 soil-cec: csv 31.6 pdf 13.6"* ]] || return 1
}

@test "AC1 negative: no PDF folder fails, it does not skip" {
  copy_designing
  SOIL_PDF_DIR="$BATS_TEST_TMPDIR/none" run "$W/designing/tests/check-soil.sh"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" == *"FAIL SOIL_PDF_DIR is not set or holds no S240318 PDFs"* ]] || return 1
}

@test "AC1 negative: a hand-edited .ttl that drifts from the CSV fails" {
  copy_designing
  sed -i '' '1,/hasSimpleResult "6.5"/s/hasSimpleResult "6.5"/hasSimpleResult "6.6"/' "$W/designing/data/seed/soil-2024.ttl"
  run "$W/designing/tests/check-soil.sh"
  [ "$status" -ne 0 ]
  [[ "$output" == *"FAIL soil-2024.ttl is stale"* ]] || return 1
}

@test "AC2 negative: a property with no GloSIS match and no stated gap fails" {
  copy_designing
  sed -i '' 's/ ; skos:closeMatch glosis_lh:PH \././' "$W/designing/ontology/soil.ttl"
  run "$W/designing/tests/check-soil.sh"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" == *"FAIL soil seed has shape violations"* ]] || return 1
}

@test "AC6 negative: an email from a report in the soil data fails" {
  copy_designing
  echo "S240318-226,someone@example.com,zone-back,50,soil-ph,5" >> "$W/designing/data/seed/soil-2024.csv"
  run "$W/designing/tests/check-soil.sh"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" == *"FAIL personal details found in the soil data"* ]] || return 1
}

@test "AC3: the plants read as Darwin Core occurrences, each on its bed IRI, 63 with an NCBI taxon" {
  [ -x "$GARDEN_REPO/designing/tests/check-dwc.sh" ] || { echo "missing: designing/tests/check-dwc.sh"; false; }
  run "$GARDEN_REPO/designing/tests/check-dwc.sh"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"ok   64 plants as dwc:Occurrence"* ]] || return 1
  [[ "$output" == *"ok   every dwc:locationID is the plant's bed/zone/cell/room IRI"* ]] || return 1
}

@test "AC3 negative: a view that loses the bed fails the locationID check" {
  copy_designing
  sed -i '' 's/BIND(STR(?place) AS ?loc)/BIND("somewhere" AS ?loc)/' "$W/designing/data/views/dwc-occurrences.rq"
  grep -q 'BIND("somewhere" AS ?loc)' "$W/designing/data/views/dwc-occurrences.rq" || return 1
  run "$W/designing/tests/check-dwc.sh"
  echo "$output"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"FAIL 64 plants have a missing or wrong dwc:locationID"* ]] || return 1
}

@test "AC3 negative: an NCBI ID hand-edited in the .ttl fails as stale" {
  copy_designing
  sed -i '' 's/NCBITaxon_4081>/NCBITaxon_4082>/' "$W/designing/data/seed/taxa-2025.ttl"
  run "$W/designing/tests/check-dwc.sh"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"FAIL taxa-2025.ttl is stale"* ]] || return 1
}
