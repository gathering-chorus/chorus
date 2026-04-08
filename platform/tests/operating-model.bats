#!/usr/bin/env bats
# operating-model.bats — Tests for Operating Model page (#1759)
# What Jeff sees: a view of every domain, its services, ownership, gates,
# and migration readiness — so he can decide what to migrate next.

ARTIFACT="platform/roles/wren/artifacts/operating-model.html"
DEPLOYED="public/gathering-docs/operating-model.html"
CHORUS_ROOT="/Users/jeffbridwell/CascadeProjects/chorus"
APP_ROOT="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site"

# --- AC 1: Operating model visible as a page Jeff can read ---

@test "operating model artifact exists" {
  [ -f "$CHORUS_ROOT/$ARTIFACT" ]
}

@test "page shows all framework domains and sub-products" {
  for entity in Cards Demo Hooks Clearing Seeds Protocol Observability Infrastructure Convergence Framework Photos Stories Music; do
    grep -q "$entity" "$CHORUS_ROOT/$ARTIFACT"
  done
}

@test "page shows services for each domain" {
  grep -q "Fuseki" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Gathering App" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Chorus API" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "NiFi" "$CHORUS_ROOT/$ARTIFACT"
}

@test "page shows ownership for every domain" {
  grep -q "Kade" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Wren" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Silas" "$CHORUS_ROOT/$ARTIFACT"
}

@test "page shows gate coverage" {
  grep -q "Build Gate" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Design Gate" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Direction Gate" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "Proving Gate" "$CHORUS_ROOT/$ARTIFACT"
}

# --- AC 2: Gaps visible ---

@test "page flags unprotected domains" {
  grep -q "No gate protection" "$CHORUS_ROOT/$ARTIFACT"
  grep -qi "unprotected\|no gate" "$CHORUS_ROOT/$ARTIFACT"
}

@test "completeness checks shown per domain — pass and fail markers" {
  grep -q "check-pass" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "check-fail" "$CHORUS_ROOT/$ARTIFACT"
}

# --- AC 3: Model drives migration ---

@test "page has migration sequence table" {
  grep -qi "migration" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "READY" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "GAPS" "$CHORUS_ROOT/$ARTIFACT"
}

@test "migration sequence orders entities by readiness" {
  # Cards (ready) should appear before Borg (design) in the migration table
  cards_line=$(grep -n "Cards" "$CHORUS_ROOT/$ARTIFACT" | grep -i "ready\|READY" | head -1 | cut -d: -f1)
  borg_line=$(grep -n "Borg" "$CHORUS_ROOT/$ARTIFACT" | grep -i "design\|DESIGN" | head -1 | cut -d: -f1)
  [ -n "$cards_line" ] && [ -n "$borg_line" ]
  [ "$cards_line" -lt "$borg_line" ]
}

# --- AC 4: Known gaps documented ---

@test "page lists known model gaps" {
  grep -qi "known gap\|gap.*model" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "crawler" "$CHORUS_ROOT/$ARTIFACT"
  grep -q "API.*endpoint\|endpoint.*mapped" "$CHORUS_ROOT/$ARTIFACT"
}

# --- Deployed ---

@test "deployed copy exists in gathering-docs" {
  [ -f "$APP_ROOT/$DEPLOYED" ]
}
