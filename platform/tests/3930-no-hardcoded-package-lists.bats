#!/usr/bin/env bats
# @test-type: unit — hermetic source guard: greps this repo's own scripts
#
# #3930 — the retirement gate for the hand-maintained TS package lists.
#
# Five copies of "which TypeScript packages exist" once lived in five files,
# and they drifted: one package landed with no tests asked of it, run-all-tests
# ran SOME tests under an ALL banner. The consumers now DISCOVER packages
# (tracked jest.config.js / package.json), so a hardcoded package path in one
# of them is the disease returning — this suite goes RED the commit it does.

setup() {
  # This werk's own tree — never $CHORUS_ROOT, which points at canonical and
  # would grade code this diff didn't touch (the #3701 measured-canonical trap).
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}

# The literals whose reappearance means a hand list came back. Any one of
# these dirs named as data in a runner is a curated list, because discovery
# never needs to spell a package out.
LIST_SIGNATURE='directing/products/cards"|platform/chorus-sdk"|directing/clearing"'

@test "gate-code-tests derives PROJECTS, never lists them" {
  run grep -E "$LIST_SIGNATURE" "$REPO/platform/scripts/gate-code-tests.sh"
  [ "$status" -ne 0 ]
  grep -q 'ls-files.*jest.config.js' "$REPO/platform/scripts/gate-code-tests.sh"
}

@test "run-all-tests discovers TS suites, never lists them" {
  run grep -E "$LIST_SIGNATURE" "$REPO/platform/scripts/run-all-tests.sh"
  [ "$status" -ne 0 ]
  grep -q 'ls-files.*package.json' "$REPO/platform/scripts/run-all-tests.sh"
}

@test "NEGATIVE PROOF: the signature catches a reintroduced list" {
  # #3734 — show the guard can go red: a fixture carrying the old list form
  # must match. If this fails, the grep is hollow and guards nothing.
  fixture='PROJECTS=("platform/api" "platform/chorus-sdk")'
  run bash -c "grep -E '$LIST_SIGNATURE' <<< '$fixture'"
  [ "$status" -eq 0 ]
}

@test "discovery and the old hand list agree on today's packages" {
  # The retirement must not silently SHRINK coverage: every package the old
  # gate-code list named still has a tracked jest.config.js on disk.
  for p in platform/api platform/chorus-sdk platform/pulse \
           platform/workflow-engine directing/clearing directing/products/cards; do
    [ -f "$REPO/$p/jest.config.js" ] || {
      echo "package $p lost its jest.config.js — discovery would drop it" >&2
      return 1
    }
  done
}
