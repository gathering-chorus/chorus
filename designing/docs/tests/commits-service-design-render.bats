#!/usr/bin/env bats
# @test-type: ui
# Test: rendered HTML for the version-control service design contains the
# mermaid loader so that ```mermaid``` blocks render visually as diagrams,
# not as code text.
# AC: Jeff (2026-05-02): "i need it to follow the earlier service design template
# including as is and to be diagrams" — diagrams must be visible, not raw text.
# #2683 renamed commits-service-design → version-control-service-design; this
# test follows the rename and asserts current mermaid block anchors.

HTML="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}/designing/docs/version-control-service-design.html"

@test "rendered html contains mermaid loader script" {
  grep -q 'import mermaid from' "$HTML"
}

# #3598 — the WERK/SHARED subgraph assertions were retired: the diagram was
# restructured from labelled subgraphs to a flat mermaid `flowchart` (the doc
# legitimately changed, the render is fine). Assert the diagram still RENDERS
# rather than coupling to brittle subgraph label names.
@test "rendered html contains a mermaid diagram" {
  grep -q 'class="mermaid"' "$HTML"
  grep -qE 'flowchart|graph (TD|LR)' "$HTML"
}
