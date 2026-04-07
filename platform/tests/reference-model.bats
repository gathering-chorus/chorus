#!/usr/bin/env bats
# Tests for #2300: Reference model page
# What Jeff sees: the context diagram is THE reference model — renders, has narrative, layers described.

DIAGRAM="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs/chorus-context-diagram-v2.html"
DIAGRAM_JS="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs/chorus-context-diagram-v2.js"

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

@test "mermaid init JS exists" {
  [ -f "$DIAGRAM_JS" ]
}

@test "ownership table includes Quality and Services horizontals" {
  grep -q "Quality" "$DIAGRAM"
  grep -q "Services" "$DIAGRAM"
  grep -q "Horizontal Capabilities" "$DIAGRAM"
}
