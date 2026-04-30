#!/usr/bin/env bats
# CI service design — shape regression
# Per Wren 2026-04-30: doc should lead with "Local layers (0-2) vs CI (3)"
# framing, not the additive "Three Layers" framing that obscures the post-#2600
# cost-stop cut.

DOC="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}/designing/docs/ci-pipeline-service-design.html"
[ -f "$DOC" ] || DOC="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../../designing/docs" && pwd)/ci-pipeline-service-design.html"

@test "doc leads with Local-vs-CI framing, not Three Layers" {
  run grep -F "Local layers (0-2) vs CI (3)" "$DOC"
  [ "$status" -eq 0 ]
}

@test "doc names Layer 0 substrate explicitly" {
  run grep -E "Layer 0.*Substrate|Layer 0 — Substrate" "$DOC"
  [ "$status" -eq 0 ]
}

@test "doc does not retain the obsolete Three Layers heading" {
  run grep -F "<h2>The Three Layers</h2>" "$DOC"
  [ "$status" -ne 0 ]
}
