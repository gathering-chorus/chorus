#!/usr/bin/env bats
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

@test "rendered html contains the werk-lifecycle mermaid block" {
  grep -q 'subgraph WERK' "$HTML"
}

@test "rendered html contains the shared-infrastructure mermaid block" {
  grep -q 'subgraph SHARED' "$HTML"
}
