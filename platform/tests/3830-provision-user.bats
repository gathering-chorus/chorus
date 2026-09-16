#!/usr/bin/env bats
# @test-type: contract
# #3830 — chorus-provision. Every test here is a REFUSAL, because the card is
# about the states a half-provisioned user can be left in, and each refusal is
# one of them made unreachable.
#
# Measured baseline this card exists for (2026-09-15 12:47): 13 principals,
# three of them — crawler-index, reindex-worker, embed-worker — naming WebIDs
# whose profile card answers 401. Principal rows typed by hand into a TTL while
# seed-css.sh's AGENTS list decided which pods actually got made. Two lists,
# nothing reconciling them.

BIN="${BATS_TEST_DIRNAME}/../services/chorus-provision/target/release/chorus-provision"

setup() {
  [ -x "$BIN" ] || skip "chorus-provision not built"
  T="$(mktemp -d)"
  export FUSEKI_URL="http://127.0.0.1:59998"   # nothing listens
  export CSS_URL="http://127.0.0.1:59999"      # nothing listens
  export CHORUS_API="http://127.0.0.1:59997"
}
teardown() { rm -rf "$T"; }

@test "usage is exit 2, not a silent success" {
  run "$BIN"
  [ "$status" -eq 2 ]
  [[ "$output" == *"create"* ]]
}

@test "NEGATIVE PROOF — an empty read is REFUSED, never reported as a clean census" {
  # The census reads the register through SPARQL. With the store unreachable it
  # reads zero principals. Zero principals and a healthy system are identical
  # from the inside, so the only honest answer is a refusal.
  #
  # This is not hypothetical: the first version of the JSON reader matched the
  # needle `"value":"` while Jena pretty-prints `"value" : "`, and it read 0 of
  # 13 bindings from a perfectly good response. This refusal is what surfaced it.
  run "$BIN" census
  [ "$status" -eq 2 ]
  [[ "$output" == *"refusing to report a clean census over an empty read"* ]]
}

@test "NEGATIVE PROOF — a role store that cannot be read is UNMEASURED, not no-role" {
  # A failed read is not a FALSE. Read the other way, an unreachable roles
  # domain refuses every provision and calls it a policy decision.
  run "$BIN" create somebody-new
  [[ "$output" == *"UNMEASURED, not as no-role"* ]]
}

@test "the register being unreachable REFUSES before anything is written" {
  run "$BIN" create somebody-new
  [ "$status" -eq 2 ]
  [[ "$output" == *"nothing was written"* ]]
}

@test "the refusal names the register as the only source of a webId" {
  # The whole card in one sentence: if the server did not give us an
  # identifier, there is nothing honest to write.
  run "$BIN" create somebody-new
  [[ "$output" == *"only source of a webId"* ]]
}

@test "NEGATIVE PROOF — there is no template fallback anywhere in the source" {
  # seed-css.sh:75 is the line this binary exists to delete:
  #     [ -n "$WEBID" ] || WEBID="$ISSUER_URL/$AGENT/profile/card#me"
  # It cannot fail, so it always yields a plausible WebID with nothing behind
  # it. A grep is the right shape of check: the defect is the EXISTENCE of a
  # construction path, and a behavioural test cannot prove absence.
  src="${BATS_TEST_DIRNAME}/../services/chorus-provision/src/main.rs"
  run grep -nE '^\s*(let|.*=)\s*format!\("\{issuer\}/\{name\}/profile/card' "$src"
  [ "$status" -ne 0 ]
}

@test "the plan subcommand writes nothing and says so" {
  run "$BIN" plan somebody-new
  [ "$status" -eq 0 ]
  [[ "$output" == *"REFUSE"* ]]
  [[ "$output" == *"all three, or none"* ]]
}

@test "plan is a separate verb, not a dry-run flag on create" {
  # A boolean that switches between writing and not writing is one wrong
  # default away from provisioning during a test run. This card exists because
  # identities got made by accident.
  src="${BATS_TEST_DIRNAME}/../services/chorus-provision/src/main.rs"
  run grep -cE '\-\-dry-run|dry_run' "$src"
  [ "$output" = "0" ]
}
