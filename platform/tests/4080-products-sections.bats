#!/usr/bin/env bats
# @test-type: integration:api
load test_helper
#
# #4080 — every product carries its nine design sections as attributes, filled
# from its existing HTML design (Jeff, 2026-09-03: "focus on products … reference
# existing earlier html designs as u fill gaps"). What Jeff sees: /products serves
# nine rows and each serves promise / structure / model / pagesAndFlow /
# apiSurface / asIs / toBe / notInScope / references non-empty, sourced from the
# model not the html; every references section names its source design doc; werk's
# hasDesignDoc resolves to the real product design, not the retired stub.
# Negative proof (#3734): the per-section check is shown to FAIL on a served row
# with one section removed, and on a section that is present but too short to be
# real — so a check that passed on whitespace could not pass this file.

setup() {
  OWL_URL="${OWL_URL:-http://localhost:3360}"
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run"
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
}

PRODUCTS="athena borg chorus clearing convergence loom pulse spine werk"
SECTIONS="promise structure model pagesAndFlow apiSurface asIs toBe notInScope references"

product_row() {
  curl -sf --max-time 10 "$OWL_URL/products" | python3 -c '
import sys, json
d = json.load(sys.stdin)
rows = d if isinstance(d, list) else d.get("items", d.get("rows", d.get("data", [])))
for r in rows:
    if r.get("name") == sys.argv[1]:
        print(json.dumps(r)); break
' "$1"
}

# exit 0 when the row on stdin carries section $1 as a real string (>20 chars), else 1
section_ok() {
  python3 -c "import sys,json; v=json.load(sys.stdin).get('$1',''); sys.exit(0 if isinstance(v,str) and len(v.strip())>20 else 1)"
}

@test "AC1: all nine products serve all nine design sections non-empty" {
  for p in $PRODUCTS; do
    row="$(product_row "$p")"
    [ -n "$row" ] || { echo "$p: no served row"; return 1; }
    for k in $SECTIONS; do
      printf '%s' "$row" | section_ok "$k" || { echo "$p.$k empty"; return 1; }
    done
  done
}

@test "AC1: each product's references section names its source design doc" {
  for p in $PRODUCTS; do
    printf '%s' "$(product_row "$p")" | python3 -c '
import sys, json
r = json.load(sys.stdin); refs = r.get("references", "")
ok = "designing/docs/" in refs and ".html" in refs
sys.exit(0 if ok else 1)' || { echo "$p.references cites no designing/docs/*.html"; return 1; }
  done
}

@test "AC1: werk hasDesignDoc points at the real product design, not the retired stub" {
  # The edge is what this card changes. Whether /documents SERVES the row is the
  # seeder's graph placement (documents seed into urn:chorus:instances, the API
  # serves urn:chorus:domains:documents) — a pre-existing gap owned by #4062, so
  # the variant serves 0 documents today and that is not asserted here.
  printf '%s' "$(product_row werk)" | python3 -c '
import sys, json
v = json.load(sys.stdin).get("hasDesignDoc", "")
v = v if isinstance(v, list) else [v]
sys.exit(0 if any("werk-product-design" in str(x) for x in v) and not any("werk-subproduct-design" in str(x) for x in v) else 1)'
}

@test "AC2: the sections come from the seed file the pipeline deploys, not a hand write" {
  # the served text for one product/section is byte-for-byte in the day-authored seed
  seed="$CHORUS_ROOT/designing/data/product-instances.ttl"
  [ -f "$seed" ]
  grep -q '^product:designing/data/product-instances.ttl' "$CHORUS_ROOT/platform/config/instance-seed-manifest.txt"
  for p in $PRODUCTS; do
    first="$(printf '%s' "$(product_row "$p")" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("promise","").splitlines()[0][:60])')"
    [ -n "$first" ]
    grep -qF -- "$first" "$seed" || { echo "$p promise not in seed: $first"; return 1; }
  done
}

@test "AC4 negative proof (#3734): the section check FAILS on a row missing a section" {
  row="$(product_row spine)"
  [ -n "$row" ]
  # go through a file: the row text carries apostrophes, which a shell-quoted inline copy breaks on
  printf '%s' "$row" | python3 -c 'import sys,json; r=json.load(sys.stdin); r.pop("toBe",None); print(json.dumps(r))' > "$BATS_TEST_TMPDIR/row-missing-tobe.json"
  run bash -c "$(declare -f section_ok); section_ok toBe < '$BATS_TEST_TMPDIR/row-missing-tobe.json'"
  [ "$status" -eq 1 ]
  # and the untouched row passes the same check, so the red above is the removal, not the harness
  run bash -c "$(declare -f section_ok); section_ok toBe" <<< "$row"
  [ "$status" -eq 0 ]
}

@test "AC4 negative proof (#3734): a present-but-hollow section (whitespace, 3 words) FAILS" {
  run bash -c "$(declare -f section_ok); printf '%s' '{\"promise\":\"   \"}' | section_ok promise"
  [ "$status" -eq 1 ]
  run bash -c "$(declare -f section_ok); printf '%s' '{\"promise\":\"to be done\"}' | section_ok promise"
  [ "$status" -eq 1 ]
  run bash -c "$(declare -f section_ok); printf '%s' '{\"promise\":\"$(printf 'x%.0s' $(seq 1 40))\"}' | section_ok promise"
  [ "$status" -eq 0 ]
}
