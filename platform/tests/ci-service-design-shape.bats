#!/usr/bin/env bats
# @test-type: unit — static source/shape guard, hermetic
load test_helper
# CI service design — shape regression
# Per Wren 2026-04-30: doc should lead with "Local layers (0-2) vs CI (3)"
# framing, not the additive "Three Layers" framing that obscures the post-#2600
# cost-stop cut.

DOC="${CHORUS_ROOT_FOR_TEST:-${CHORUS_ROOT}}/designing/docs/ci-pipeline-service-design.html"
[ -f "$DOC" ] || DOC="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../../designing/docs" && pwd)/ci-pipeline-service-design.html"

# #3710 — the two cases here used to grep exact sentences Wren asked for on
# 2026-04-30 ("Local layers (0-2) vs CI (3)", "Layer 0 — Substrate"). The doc has
# since been rewritten and no longer uses that wording, so both failed on prose
# that changed while the POINT it protects did not. Pinning an author's exact
# phrasing makes every later edit look like a regression; what actually matters
# is that the doc still separates the local pre-merge lane from hosted CI and
# still records why (the #2600 cost-stop). Assert that instead.

@test "doc separates the local pre-merge lane from hosted CI" {
  run grep -qiE "pre-merge|runs locally before push" "$DOC"
  [ "$status" -eq 0 ]
}

@test "doc still records the #2600 cost-stop that cut the hosted lane" {
  run grep -qE "2600|cost-stop" "$DOC"
  [ "$status" -eq 0 ]
}

@test "doc does not retain the obsolete Three Layers heading" {
  run grep -F "<h2>The Three Layers</h2>" "$DOC"
  [ "$status" -ne 0 ]
}
