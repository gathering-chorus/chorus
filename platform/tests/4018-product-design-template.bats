#!/usr/bin/env bats
# @test-type: unit — validates TTL with Jena's shacl CLI and reads the page source; no store, no network.
# @domain: products
# @card: #4018
# @owner: wren
#
# #4018 — Jeff 2026-10-10: "the product designs read like service designs"; "a
# product design must show page flow and value stream flow and downstream
# product and domain and service dependencies". The template's four new words
# sit on ProductShape; the product page reads in the template's order, draws the
# value stream and the dependencies from the graph, and shows the row's history.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPE="$ROOT/roles/wren/ontology/product-design-4018.ttl"
PAGE="$ROOT/platform/api/public/athena/product.html"
FOLD="$ROOT/platform/api/public/athena/history-fold.js"

setup() {
  T="$BATS_TEST_TMPDIR"
  cat > "$T/good.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:pulse a chorus:Product ;
    chorus:job "When I come back after an hour away, I want to see what everyone is doing, so I can step in only where needed." ;
    chorus:whyNow "Six times on the design day Jeff asked are you working." ;
    chorus:outcomes "Outcome | Today | Target" ;
    chorus:openBets "Observed state can replace every declared state." .
EOF
  cat > "$T/bad.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:pulse a chorus:Product ;
    chorus:job "one job" , "a second job" ;
    chorus:outcomes 6 .
EOF
}

# ProductShape's sh:targetClass lives in chorus.ttl with its required floor; this file only adds
# properties. Pin the target here so the four new rules are tested alone (without it every row
# "conforms" — the first run of this test proved that).
report() {
  { cat "$SHAPE"; printf '\nchorus:ProductShape a sh:NodeShape ; sh:targetClass chorus:Product .\n'; } > "$T/shapes.ttl"
  shacl validate --shapes "$T/shapes.ttl" --data "$1" 2>&1
}

@test "every new property in the template is described" {
  run grep -c 'sh:path' "$SHAPE"; paths="$output"
  run grep -c 'sh:description' "$SHAPE"
  [ "$output" -eq "$paths" ]
}

@test "a product written in the template conforms" {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  run report "$T/good.ttl"
  [[ "$output" == *"sh:conforms  true"* ]] || { echo "$output"; false; }
}

@test "negative proof: two jobs and a number for outcomes are refused" {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  run report "$T/bad.ttl"
  [[ "$output" == *"sh:conforms  false"* ]] || { echo "$output"; false; }
  [[ "$output" == *"job"* && "$output" == *"outcomes"* ]] || { echo "$output"; false; }
}

@test "the page reads in the template's order, engineering last" {
  order=$(grep -oE "^\s+\['(promise|audience|job|whyNow|valueProposition|pagesAndFlow|valueStream|dependencies|outcomes|notInScope|composition|apiSurface)'," "$PAGE" | tr -d " ['," | tr '\n' ' ')
  [ "$order" = "promise audience job whyNow valueProposition pagesAndFlow valueStream dependencies outcomes notInScope composition apiSurface " ] || { echo "got: $order"; false; }
}

@test "the value stream and dependency pictures are drawn from the graph, not typed" {
  grep -q "const vsSrc = () =>" "$PAGE"
  grep -q "stepLocal(e.atStep)" "$PAGE"
  grep -q "const depSrc = () =>" "$PAGE"
  grep -q "asArray(e.consumes)" "$PAGE"
  grep -q "asArray(r.consumes)" "$PAGE"   # used by: the reverse direction
}

@test "history reads this row's versions from the route the door serves" {
  grep -q "/versions?ofRow=" "$FOLD"
  # negative proof: the old route answered 404 and the page said "no prior versions kept yet"
  run grep -q "fetchJSON('/revisions')" "$FOLD"
  [ "$status" -ne 0 ]
}

@test "the template file is in the model set athena-deploy loads" {
  grep -q 'roles/wren/ontology/product-design-4018.ttl' "$ROOT/platform/services/athena-deploy/src/lib.rs"
}

# The post-land rewrite (roles/wren/notes/4018-postland.py, 4018-postland-designs.json,
# 4018-postland-jeff.sh) runs once against prod after the land; these hold it to the template.
NOTES="$ROOT/roles/wren/notes"

@test "post-land designs only write fields the template and ProductShape know" {
  run python3 - "$NOTES/4018-postland-designs.json" <<'PY'
import json, sys
known = {"promise", "audience", "job", "whyNow", "valueProposition", "pagesAndFlow", "outcomes", "notInScope", "openBets", "hasDomain"}
d = json.load(open(sys.argv[1]))
bad = {p: sorted(set(v) - known) for p, v in d.items() if not p.startswith("_") and set(v) - known}
missing = {p: sorted({"promise", "audience", "job", "whyNow", "valueProposition", "outcomes", "notInScope"} - set(v)) for p, v in d.items() if not p.startswith("_")}
missing = {p: m for p, m in missing.items() if m}
print(bad, missing); sys.exit(1 if bad or missing else 0)
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "the post-land rewrite writes nothing without --apply, and leaves consumes to Jeff's links" {
  python3 -m py_compile "$NOTES/4018-postland.py"
  grep -q 'APPLY = "--apply" in sys.argv' "$NOTES/4018-postland.py"
  grep -q '"consumes", "consumesEvent"' "$NOTES/4018-postland.py"
  bash -n "$NOTES/4018-postland-jeff.sh"
  run grep -c 'INS' "$NOTES/4018-postland-jeff.sh"
  [ "$output" -ge 3 ]
}
