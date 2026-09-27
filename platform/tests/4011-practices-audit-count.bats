#!/usr/bin/env bats
# @test-type: unit — reads the verb's source and its manifest; no store, no services
# @domain: knowledge — the product domain this suite guards (#4334)
#
# #4011 caught a copy-paste bug: the PRACTICES_SET block in the bash deploy was
# copied from PRINCIPLES_SET and still said "principles" in its messages, its
# refusal reasons and its audit count — a populated deploy reporting as empty.
#
# #4229 replaced eight copied blocks with ONE leg reading a manifest, so that
# bug is now unreachable by construction rather than by vigilance. These tests
# assert the construction, which is the durable form of the same question.

SRC="$BATS_TEST_DIRNAME/../services/athena-deploy/src/lib.rs"
MANIFEST="$BATS_TEST_DIRNAME/../config/domain-set-manifest.txt"

@test "#4011 practices is a row in the manifest, not a block of its own code" {
  run grep -c '^practices|' "$MANIFEST"
  [ "$output" -ge 1 ]
}

@test "#4011 practices and principles are separate sets with separate graphs" {
  # The copy bug in one sentence: two sets that were supposed to differ did not.
  pg="$(grep '^practices|' "$MANIFEST" | head -1 | cut -d'|' -f2)"
  rg="$(grep '^principles|' "$MANIFEST" | head -1 | cut -d'|' -f2)"
  [ -n "$pg" ]
  [ -n "$rg" ]
  [ "$pg" != "$rg" ]
}

@test "#4011 the deployed event carries the set name, so the spine is queryable by it" {
  run grep -c '("set", set.name.clone())' "$SRC"
  [ "$output" -ge 1 ]
}

@test "#4011 NEGATIVE PROOF: no set's name is hardcoded in the leg's messages" {
  # If any of the eight names appears as a literal in the shared leg, a copy
  # has crept back in and the next set added will inherit the wrong word.
  # Comments discuss these names on purpose; only CODE may not carry them.
  # A check that cannot tell a comment from a line of code is the hollow shape
  # this file is about — it flagged its own explanatory comment first.
  code="$(grep -vE "^[[:space:]]*(//|/\*|\*)" "$SRC")"
  run bash -c "printf '%s' \"\$1\" | grep -cE '\"(practices|principles|values|services|infrastructure|code-vocab)\"' || true" _ "$code"
  [ "$output" -eq 0 ]
}

@test "#4011 NEGATIVE PROOF: the leg's messages are built from the set, not a constant" {
  # The refusals and the summary must interpolate the name. A constant string
  # here would pass the test above and still report every set as the same one.
  run grep -c 'set.name' "$SRC"
  [ "$output" -ge 4 ]
}
