#!/usr/bin/env bats
# @test-type: integration — reads a live service, skip-if-absent in CI
load test_helper
# operating-model.bats — Tests for Operating Model page (#1759)
# What Jeff sees: a view of every domain, its services, ownership, gates,
# and migration readiness — so he can decide what to migrate next.

# #3606 — repointed, then FIXED.
#
# The original path `platform/roles/wren/artifacts/operating-model.html` is wrong
# twice over: roles live at `roles/`, not `platform/roles/`, and no copy of this
# artifact exists anywhere under the chorus repo. The only copy on disk is the
# deployed one in the app:
#     ~/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs/operating-model.html
#
# My first pass at this set ARTIFACT to the empty string and left every assertion
# reading CHORUS_ROOT joined with ARTIFACT — which expands to the repo root with a
# trailing slash, i.e. a DIRECTORY, never a file. That turned one honest failure
# into ten, and it landed on main. The lesson is narrow and worth keeping:
# blanking a variable is not repointing it. If the thing under test moved, move
# the assertions to where it went; an empty path is a new bug wearing the old
# one's comment.
#
# PAGE is the single source of the location now, so there is no way for the
# assertions and the existence check to disagree again.
DEPLOYED="public/gathering-docs/operating-model.html"
CHORUS_ROOT="${CHORUS_ROOT}"
APP_ROOT="${HOME}/CascadeProjects/jeff-bridwell-personal-site"
PAGE="$APP_ROOT/$DEPLOYED"

# Integration tier (header line 2): the app repo is a sibling checkout that CI
# does not have. Skip cleanly when it is absent rather than reporting content
# failures that are really an absent-repo failure — those two states are not the
# same and must not produce the same red.
setup() {
  [ -f "$PAGE" ] || skip "operating-model page not present at $PAGE (app repo absent — integration tier)"
}

# --- AC 1: Operating model visible as a page Jeff can read ---

@test "operating model artifact exists" {
  [ -f "$PAGE" ]
}

@test "page shows all framework domains and sub-products" {
  for entity in Cards Demo Hooks Clearing Seeds Protocol Observability Infrastructure Convergence Framework Photos Stories Music; do
    grep -q "$entity" "$PAGE"
  done
}

@test "page shows services for each domain" {
  grep -q "Fuseki" "$PAGE"
  grep -q "Gathering App" "$PAGE"
  grep -q "Chorus API" "$PAGE"
  grep -q "NiFi" "$PAGE"
}

@test "page shows ownership for every domain" {
  grep -q "Kade" "$PAGE"
  grep -q "Wren" "$PAGE"
  grep -q "Silas" "$PAGE"
}

@test "page shows gate coverage" {
  grep -q "Build Gate" "$PAGE"
  grep -q "Design Gate" "$PAGE"
  grep -q "Direction Gate" "$PAGE"
  grep -q "Proving Gate" "$PAGE"
}

# --- AC 2: Gaps visible ---

@test "page flags unprotected domains" {
  grep -q "No gate protection" "$PAGE"
  grep -qi "unprotected\|no gate" "$PAGE"
}

@test "completeness checks shown per domain — pass and fail markers" {
  grep -q "check-pass" "$PAGE"
  grep -q "check-fail" "$PAGE"
}

# --- AC 3: Model drives migration ---

@test "page has migration sequence table" {
  grep -qi "migration" "$PAGE"
  grep -q "READY" "$PAGE"
  grep -q "GAPS" "$PAGE"
}

@test "migration sequence orders entities by readiness" {
  # Cards (ready) should appear before Borg (design) in the migration table
  cards_line=$(grep -n "Cards" "$PAGE" | grep -i "ready\|READY" | head -1 | cut -d: -f1)
  borg_line=$(grep -n "Borg" "$PAGE" | grep -i "design\|DESIGN" | head -1 | cut -d: -f1)
  [ -n "$cards_line" ] && [ -n "$borg_line" ]
  [ "$cards_line" -lt "$borg_line" ]
}

# --- AC 4: Known gaps documented ---

@test "page lists known model gaps" {
  grep -qi "known gap\|gap.*model" "$PAGE"
  grep -q "crawler" "$PAGE"
  grep -q "API.*endpoint\|endpoint.*mapped" "$PAGE"
}

# --- Deployed ---

@test "deployed copy exists in gathering-docs" {
  [ -f "$APP_ROOT/$DEPLOYED" ]
}
