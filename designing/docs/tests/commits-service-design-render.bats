#!/usr/bin/env bats
# Test: rendered HTML for commits-service-design.md contains the mermaid loader
# so that ```mermaid``` blocks render visually as diagrams, not as code text.
# AC: Jeff (2026-05-02): "i need it to follow the earlier service design template
# including as is and to be diagrams" — diagrams must be visible, not raw text.

HTML="/Users/jeffbridwell/CascadeProjects/chorus/designing/docs/commits-service-design.html"

@test "rendered html contains mermaid loader script" {
  grep -q 'import mermaid from' "$HTML"
}

@test "rendered html contains v3 As-Is mermaid block" {
  grep -q 'v2.5 today' "$HTML"
}

@test "rendered html contains v3 To-Be mermaid block" {
  grep -q 'chorus_commit service' "$HTML"
}
