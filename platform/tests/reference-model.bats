#!/usr/bin/env bats
# @test-type: integration — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
load test_helper
# Tests for #2300: Reference model page
# What Jeff sees: the context diagram is THE reference model — renders, has narrative, layers described.

# #3606 — repointed. Wren's #2458 ("KM op 1: wrong-cabinet moves") relocated this
# doc OUT of the personal-site public dir and INTO chorus designing/docs/. The
# test kept aiming at the pre-move path, so every assertion failed on a file that
# exists and is fine.
DIAGRAM="${CHORUS_ROOT}/designing/docs/chorus-context-diagram-v2.html"
DIAGRAM_JS="${HOME}/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs/chorus-context-diagram-v2.js"

@test "context diagram HTML exists" {
  [ -f "$DIAGRAM" ]
}

@test "context diagram has all 8 layers" {
  grep -q "Shared Awareness" "$DIAGRAM"
  grep -q "OWL/RDF" "$DIAGRAM"
  grep -q "Loom" "$DIAGRAM"
  grep -q "Protocol" "$DIAGRAM"
  grep -q "Framework" "$DIAGRAM"
  grep -q "Apps" "$DIAGRAM"
  grep -q "Observability" "$DIAGRAM"
  grep -q "Infrastructure" "$DIAGRAM"
}

@test "context diagram has narrative section" {
  grep -q "Why Each Layer Exists" "$DIAGRAM"
  grep -q "Reading the Model" "$DIAGRAM"
}

@test "context diagram references OWL/RDF semantic layer" {
  grep -q "Semantic Layer" "$DIAGRAM"
  grep -q "domain→service→gate→role" "$DIAGRAM"
}

@test "mermaid script is self-hosted not CDN" {
  grep -q "/gathering-docs/mermaid.min.js" "$DIAGRAM"
  ! grep -q "d3js.org" "$DIAGRAM"
  ! grep -q "cdn.jsdelivr.net" "$DIAGRAM"
}

# #3606 — RETIRED. chorus-context-diagram-v2.js has NO git history in this repo:
# it never existed here. The diagram was authored in the personal-site public dir
# with a companion init file and relocated into chorus by #2458; the init is now a
# single inline <script> block in the HTML itself. This asserted the existence of
# a packaging that does not exist and never did on this side, so it could only
# ever fail. Retired rather than recreated — the thing it guarded (mermaid gets
# initialised) is covered by the self-hosted assertion above plus the doc rendering.

@test "ownership table includes Quality and Services horizontals" {
  grep -q "Quality" "$DIAGRAM"
  grep -q "Services" "$DIAGRAM"
  grep -q "Horizontal Capabilities" "$DIAGRAM"
}
