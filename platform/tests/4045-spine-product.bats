#!/usr/bin/env bats
# @test-type: integration:api
load test_helper
#
# #4045 — the spine is a modeled Product. What Jeff sees: /products serves nine
# rows with spine among them, its fields real (no stubs), its hasDomain edge to
# the events domain readable from the served collection row, and the chorus
# product's hasDomain edges back (memory, search). Negative proof (#3734): a
# product that does not exist must NOT 200, and the same edge assertion is shown
# to FAIL against a product that does not carry the edge — so a fold that
# returned every domain for every product could not pass this file.

setup() {
  OWL_URL="${OWL_URL:-http://localhost:3360}"
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run"
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
}

# Print the served collection row for one product as compact JSON ("" if absent).
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

# Does the product's served hasDomain fold carry this domain? exit 0 yes / 1 no.
has_domain() {
  product_row "$1" | python3 -c '
import sys, json
row = sys.stdin.read().strip()
if not row: sys.exit(2)
v = json.loads(row).get("hasDomain")
v = v if isinstance(v, list) else ([v] if v else [])
sys.exit(0 if sys.argv[1] in v else 1)
' "$2"
}

@test "AC1: spine is served as a Product with every narrative field populated" {
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' "$OWL_URL/products/spine"
  [ "$output" = "200" ]
  row="$(product_row spine)"
  [ -n "$row" ]
  for f in audience valueProposition vision status; do
    printf '%s' "$row" | python3 -c "import sys,json; v=json.load(sys.stdin).get('$f',''); sys.exit(0 if v and v.strip() else 1)"
  done
  printf '%s' "$row" | python3 -c "import sys,json; v=json.load(sys.stdin).get('ownedBy',''); sys.exit(0 if 'wren' in str(v) else 1)"
}

@test "AC1: nine products are served and spine is one of them" {
  n="$(curl -sf --max-time 10 "$OWL_URL/products" | python3 -c '
import sys, json
d = json.load(sys.stdin)
rows = d if isinstance(d, list) else d.get("items", d.get("rows", d.get("data", [])))
print(len(rows))')"
  [ "$n" -ge 9 ]
  [ -n "$(product_row spine)" ]
}

@test "AC2: spine hasDomain events, readable from the served row" {
  has_domain spine events
}

@test "AC3: the chorus product carries hasDomain edges again (memory, search)" {
  has_domain chorus memory
  has_domain chorus search
}

@test "AC5 negative: a product that does not exist does not 200" {
  run curl -s --max-time 10 -o /dev/null -w '%{http_code}' "$OWL_URL/products/no-such-product-4045"
  [ "$output" = "404" ]
}

@test "AC5 negative proof (#3734): the edge check FAILS for a product without that edge" {
  # pulse composes knowledge only; if has_domain said yes here, the fold is lying.
  run has_domain pulse events
  [ "$status" -eq 1 ]
  # and an unknown product is a distinct failure (2), not a silent pass
  run has_domain no-such-product-4045 events
  [ "$status" -eq 2 ]
}

# ── #4045 second half — Jeff: "i expected all doc sections to be attributes in
# the class, not stored in html." The shape declares nine section properties;
# spine's row serves them non-empty; the schema the page renders from lists them.
SECTIONS="promise structure model pagesAndFlow apiSurface asIs toBe notInScope references"

@test "AC4: the served Product schema declares every design-doc section as a property" {
  props="$(curl -sf --max-time 10 "$OWL_URL/products/openapi.json" | python3 -c '
import sys, json
print(" ".join(sorted(json.load(sys.stdin)["components"]["schemas"]["Product"]["properties"].keys())))')"
  for k in $SECTIONS; do
    case " $props " in *" $k "*) ;; *) echo "schema lacks $k"; return 1 ;; esac
  done
}

@test "AC4: spine serves every section non-empty, from the model not the html" {
  row="$(product_row spine)"
  [ -n "$row" ]
  for k in $SECTIONS; do
    printf '%s' "$row" | python3 -c "import sys,json; v=json.load(sys.stdin).get('$k',''); sys.exit(0 if isinstance(v,str) and len(v.strip())>20 else 1)" \
      || { echo "spine.$k empty"; return 1; }
  done
}

@test "AC4 negative proof (#3734): a property the shape does not declare is NOT in the schema" {
  props="$(curl -sf --max-time 10 "$OWL_URL/products/openapi.json" | python3 -c '
import sys, json
print(" ".join(json.load(sys.stdin)["components"]["schemas"]["Product"]["properties"].keys()))')"
  case " $props " in *" section1 "*|*" designHtml "*) return 1 ;; esac
  # and the section check is shown to FAIL on a row that lacks a section (#4080: every
  # product is filled now, so the fixture is a served row with one section removed)
  run bash -c "$(declare -f product_row); OWL_URL=$OWL_URL product_row spine | python3 -c '
import sys,json; r=json.load(sys.stdin); r.pop(\"toBe\",None)
v=r.get(\"toBe\",\"\"); sys.exit(0 if isinstance(v,str) and len(v.strip())>20 else 1)'"
  [ "$status" -eq 1 ]
}
