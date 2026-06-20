#!/usr/bin/env bats
# @test-type: integration — reads a live service, skip-if-absent in CI
load test_helper
# clearing-service-design.bats — Tests for Clearing service design page (#2287)
# What Jeff sees: a service design page like Pulse and Loom, showing the Clearing's
# domain dependencies, components, consumers/producers, and infrastructure.

ARTIFACT="product-manager/artifacts/clearing-service-design.html"
DEPLOYED="public/gathering-docs/clearing-service-design.html"
CHORUS_ROOT="${CHORUS_ROOT}"
APP_ROOT="${HOME}/CascadeProjects/jeff-bridwell-personal-site"

# --- AC 1: Service design HTML page exists following existing template ---

@test "clearing service design artifact exists" {
  [ -f "$CHORUS_ROOT/$ARTIFACT" ]
}

@test "page has standard service design sections" {
  grep -q "Experience\|promise" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Overview" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Infrastructure" "$CHORUS_ROOT/$ARTIFACT"
}

@test "page follows template structure — has component blocks" {
  count=$(grep -c 'class="component"' "$CHORUS_ROOT/$ARTIFACT")
  [ "$count" -ge 4 ]
}

# --- AC 2: Maps domain dependencies ---

@test "page maps 5+ supporting domains" {
  for domain in coordination nudge observability awareness spine loom; do
    grep -qi "$domain" "$CHORUS_ROOT/$ARTIFACT"
  done
}

# --- AC 3: Identifies consumers and producers ---

@test "page identifies Jeff as consumer" {
  grep -q "Jeff" "$CHORUS_ROOT/$ARTIFACT"
  grep -qi "consumer" "$CHORUS_ROOT/$ARTIFACT"
}

@test "page identifies all three roles" {
  grep -q "Wren" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Silas" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Kade" "$CHORUS_ROOT/$ARTIFACT"
}

@test "page identifies producers — Bridge, role-state, alerts" {
  grep -qi "Bridge" "$CHORUS_ROOT/$ARTIFACT"
  grep -qi "role.state\|role-state\|Role Tile" "$CHORUS_ROOT/$ARTIFACT"
  grep -qi "alert" "$CHORUS_ROOT/$ARTIFACT"
}

# --- AC 5: Rendered at gathering-docs ---

@test "deployed copy exists in gathering-docs" {
  [ -f "$APP_ROOT/$DEPLOYED" ]
}
