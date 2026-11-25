#!/usr/bin/env bats
# @test-type: integration — playwright lists the suite from this tree; no browser is opened, no server, no build
# @domain: tests
# @card: #5040 AC4 · owner: wren
#
# The through-the-door suite (proving/flows/athena-domain-public-door.spec.cjs) must run on any
# card that changes an Athena page, or the next change can break Jeff's phone view while every
# test is green again (2026-11-25 08:06: "the page is way too broken to do an interview", 13 of
# 13 green). werk-test plays, in a card's test step, every registered browser spec plus the
# card's own (card_ui_files, #5034 AC5, held by 5034-athena-domain-suites-run.bats). A spec is
# registered by the crawl from its "@test-type: e2e:ui" header, and the scope gate ties it to a
# page file by the file's path in the spec's text. So three things make this suite run:
#   - playwright finds it from the tree root and it holds tests for both public mounts;
#   - its header is the browser-lane header the registry reads;
#   - it names every Athena page file it opens.
# NEGATIVE: the same checks fail on a spec with the header cut, and on one that names no page.
#
# Covers: proving/flows/athena-domain-public-door.spec.cjs
# Covers: proving/flows/lib/public-door-5040.cjs

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SPEC="proving/flows/athena-domain-public-door.spec.cjs"
PAGES="domain.html domains-view.html facets.js model-fold.js rows.js athena-flow.js"

# is this spec text a registered-lane browser spec that names the page file $2?
names_page() { grep -q "platform/api/public/athena/$2" "$1"; }
browser_lane() { head -3 "$1" | grep -q '^// @test-type: e2e:ui'; }

@test "#5040 playwright finds the suite from the tree root, with tests at both public mounts" {
  run sh -c "cd '$ROOT' && npx playwright test --list '$SPEC' 2>&1"
  echo "$output"
  test "$status" -eq 0
  echo "$output" | grep -q "under /chorus (the public apex)"
  echo "$output" | grep -q "at the root (the door's own host)"
  n="$(echo "$output" | grep -c "athena-domain-public-door.spec.cjs")"
  test "$n" -ge 12
}

@test "#5040 the suite carries the browser-lane header and names every Athena page file it opens" {
  browser_lane "$ROOT/$SPEC"
  for p in $PAGES; do
    names_page "$ROOT/$SPEC" "$p" || { echo "the suite does not name platform/api/public/athena/$p"; return 1; }
    test -f "$ROOT/platform/api/public/athena/$p" || { echo "named page file is gone: $p"; return 1; }
  done
}

@test "#5040 NEGATIVE: a spec with the header cut, or naming no page, fails the same checks" {
  T="$(mktemp -d)"
  sed '1d' "$ROOT/$SPEC" > "$T/no-header.cjs"
  test "$(wc -l < "$T/no-header.cjs")" -lt "$(wc -l < "$ROOT/$SPEC")"
  run browser_lane "$T/no-header.cjs"
  test "$status" -ne 0
  grep -v 'platform/api/public/athena/' "$ROOT/$SPEC" > "$T/no-pages.cjs"
  run names_page "$T/no-pages.cjs" domain.html
  test "$status" -ne 0
  # and a path playwright cannot find lists nothing
  run sh -c "cd '$ROOT' && npx playwright test --list proving/flows/no-such-door-5040.spec.cjs 2>&1"
  test "$status" -ne 0
  rm -rf "$T"
}
