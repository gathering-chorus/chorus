#!/usr/bin/env bats
# @test-type: integration:api — signal:ui is fixture-data (the file greps product.html for the Flows chapter; the checks are against the served /products rows)
load test_helper
#
# #4094 — the diagrams a design carries come into the graph with the words. Jeff,
# 2026-09-03 11:35: "we are completely abandoning diagrams and other visual flows
# from the html when we migrate." What Jeff sees: a product whose HTML design had a
# mermaid diagram serves that diagram as chorus:diagram (mermaid source, first line
# a %% caption) and the product page draws it in a Flows chapter.
# Negative proofs (#3734): the "design had a diagram, row has none" check is shown to
# FAIL on a row with its diagrams removed; the per-value check FAILS on a caption
# with no body and on a body with no caption; the untouched rows pass the same checks.

setup() {
  OWL_URL="${OWL_URL:-http://localhost:3360}"
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}

live() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration (live owl-api serve) — RUN_INTEGRATION=true to run"
  curl -sf --max-time 5 "$OWL_URL/health" >/dev/null || skip "owl-api absent (#3528)"
}

# product → the HTML design its diagrams were carried from
DESIGN_OF="borg:borg-product-design spine:spine-product-design werk:werk-product-design convergence:convergence-value-stream-design athena:athena-product-design loom:loom-service-design clearing:clearing-service-design chorus:chorus-client-onboarding-design"

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

# exit 0 when every diagram value on the row (stdin) is real: a "%% caption" first line,
# then a mermaid body that starts with a diagram keyword and is longer than a stub
diagram_ok() {
  python3 -c '
import sys, json, re
r = json.load(sys.stdin); v = r.get("diagram", [])
vals = v if isinstance(v, list) else [v]
if not vals: print("no diagram values"); sys.exit(1)
for x in vals:
    ls = str(x).strip().split("\n")
    if not ls[0].startswith("%% ") or len(ls[0]) < 5: print("no caption line: " + ls[0][:40]); sys.exit(1)
    body = "\n".join(ls[1:]).strip()
    if not re.match(r"^(flowchart|graph|sequenceDiagram|classDiagram|stateDiagram|erDiagram|gantt|journey)\b", body): print("not a mermaid body: " + body[:40]); sys.exit(1)
    if len(body) < 30: print("stub body"); sys.exit(1)
'
}

# exit 0 when the row (stdin) serves at least as many diagrams as its design HTML carries
design_covered() { # $1 = design html path
  python3 -c '
import sys, json, re
r = json.load(sys.stdin); v = r.get("diagram", []); n = len(v if isinstance(v, list) else [v]) if v else 0
h = open(sys.argv[1]).read(); want = len(re.findall(r"<pre class=\"mermaid\">", h))
print(f"design {want} served {n}")
sys.exit(0 if n >= want else 1)
' "$1"
}

@test "AC2: the four products whose designs carry diagrams serve them (borg 2, spine 2, werk 2, convergence 1)" {
  live
  for pair in borg:2 spine:2 werk:2 convergence:1; do
    p="${pair%%:*}"; n="${pair##*:}"
    row="$(product_row "$p")"; [ -n "$row" ]
    got="$(printf '%s' "$row" | python3 -c 'import sys,json; v=json.load(sys.stdin).get("diagram",[]); print(len(v) if isinstance(v,list) else (1 if v else 0))')"
    [ "$got" -eq "$n" ] || { echo "$p: served $got diagrams, design has $n"; false; }
    printf '%s' "$row" | diagram_ok
  done
}

@test "AC2: no product serves fewer diagrams than its design HTML carries" {
  live
  for pair in $DESIGN_OF; do
    p="${pair%%:*}"; doc="$ROOT/designing/docs/${pair##*:}.html"
    [ -f "$doc" ] || continue
    row="$(product_row "$p")"; [ -n "$row" ]
    run bash -c "$(declare -f design_covered); design_covered '$doc'" <<< "$row"
    [ "$status" -eq 0 ] || { echo "$p: $output"; false; }
  done
}

@test "AC3 negative proof (#3734): the design-covered check FAILS on a row with its diagrams removed" {
  live
  row="$(product_row spine)"; [ -n "$row" ]
  doc="$ROOT/designing/docs/spine-product-design.html"
  printf '%s' "$row" | python3 -c 'import sys,json; r=json.load(sys.stdin); r.pop("diagram",None); print(json.dumps(r))' > "$BATS_TEST_TMPDIR/row-no-diagram.json"
  run bash -c "$(declare -f design_covered); design_covered '$doc' < '$BATS_TEST_TMPDIR/row-no-diagram.json'"
  [ "$status" -eq 1 ]
  run bash -c "$(declare -f design_covered); design_covered '$doc'" <<< "$row"
  [ "$status" -eq 0 ]
}

@test "AC3 negative proof (#3734): a caption with no body, and a body with no caption, both FAIL the per-value check" {
  run bash -c "$(declare -f diagram_ok); printf '%s' '{\"diagram\":[\"%% Structure\"]}' | diagram_ok"
  [ "$status" -eq 1 ]
  run bash -c "$(declare -f diagram_ok); printf '%s' '{\"diagram\":[\"flowchart TD\\n  A[one] --> B[two] --> C[three]\"]}' | diagram_ok"
  [ "$status" -eq 1 ]
  run bash -c "$(declare -f diagram_ok); printf '%s' '{\"diagram\":[\"%% Structure\\nflowchart TD\\n  A[one] --> B[two] --> C[three]\"]}' | diagram_ok"
  [ "$status" -eq 0 ]
}

@test "AC2: diagram values carry no card numbers or role names (the product page is for outsiders)" {
  live
  for p in borg spine werk convergence; do
    row="$(product_row "$p")"; [ -n "$row" ]
    run bash -c "printf '%s' '$(printf '%s' "$row" | python3 -c 'import sys,json; print("\\n".join(json.load(sys.stdin).get("diagram",[])).replace(chr(39),chr(8217)))')' | python3 -c 'import sys,re; t=sys.stdin.read(); b=re.findall(r\"#\\d{3,4}|\\b(?:Wren|Silas|Kade|Jeff)\\b\", t); print(b); sys.exit(1 if b else 0)'"
    [ "$status" -eq 0 ] || { echo "$p: $output"; false; }
  done
}

@test "AC2: the seed file carries the seven diagrams, each with a caption line (the pipeline deploys the file, no hand write)" {
  n="$(grep -c 'chorus:diagram """%% ' "$ROOT/designing/data/product-instances.ttl")"
  [ "$n" -eq 7 ] || { echo "seed carries $n captioned diagrams, expected 7"; false; }
}

@test "AC2: the product page has a Flows chapter drawn with the vendored mermaid, and says so when a source will not render" {
  page="$ROOT/platform/api/public/athena/product.html"
  grep -q "\['diagram',       'Flows'\]" "$page"
  grep -q 'js/vendor/mermaid.min.js' "$page"
  grep -q 'mermaid.render(' "$page"
  grep -q 'diagram source did not render' "$page"
  [ -f "$ROOT/platform/api/public/js/vendor/mermaid.min.js" ]
}

@test "AC1: the model declares chorus:diagram on the Product, Service and Domain shapes" {
  ttl="$ROOT/roles/silas/ontology/chorus.ttl"
  grep -q '^chorus:diagram  *a owl:DatatypeProperty' "$ttl"
  for shape in ProductShape DomainShape ServiceShape; do
    awk "/^chorus:$shape a sh:NodeShape/,/ \\.\$/" "$ttl" | grep -q 'sh:path chorus:diagram' || { echo "$shape lacks chorus:diagram"; false; }
  done
}
