#!/usr/bin/env bats
# @test-type: contract — reads the werk's model and the generated principles door; no store, no network.
# @domain: principles
# @card: #5021 · owner: kade
# What Jeff sees: the principles Domain row says where its code lives, has one description, and
# a principle row carries no empty status, fixed in the data and the generator, not baselined.
#
# AC2: repoTarget is no longer "loom/principles"; the row has one comment (the store's second,
# a v1 leftover, leaves when the row is redeployed whole); no principle row serves status, because
# the generated door serves status only where a shape declares it. NEGATIVE: today's snapshot
# shows all three faults (platform/tests/fixtures/5021/principles-2026-11-20.part-a{a..d}.nq).

# Shared setup, named so a change to it runs this suite: platform/tests/5021-helpers.bash
load 5021-helpers

setup() { build_5021; }

domain_row() {  # the Domain row's values for one predicate, in a dataset
  grep -F '<https://jeffbridwell.com/chorus#principles> <'"$2"'>' "$T/$1.nq" | grep -F '<urn:chorus:domains:domains>' | sed 's/^[^ ]* [^ ]* //; s/ <urn:chorus:domains:domains> \.$//'
}
RDFS_COMMENT=http://www.w3.org/2000/01/rdf-schema#comment
CHORUS_COMMENT=https://jeffbridwell.com/chorus#comment
REPO=https://jeffbridwell.com/chorus#repoTarget

@test "AC2: repoTarget names where principles' code lives, not loom/principles" {
  run domain_row after "$REPO"
  [ "$output" = '"shaping/domains/principles"' ] || { echo "$output"; return 1; }
  [ -d "$ROOT/shaping/domains/principles" ]
}

@test "AC2: the Domain row has exactly one comment" {
  n=$( { domain_row after "$RDFS_COMMENT"; domain_row after "$CHORUS_COMMENT"; } | grep -c .)
  [ "$n" -eq 1 ] || { domain_row after "$RDFS_COMMENT"; domain_row after "$CHORUS_COMMENT"; return 1; }
}

@test "AC2 NEGATIVE: today's row has the stale repoTarget and two comments" {
  run domain_row before "$REPO"
  [ "$output" = '"loom/principles"' ] || { echo "$output"; return 1; }
  n=$( { domain_row before "$RDFS_COMMENT"; domain_row before "$CHORUS_COMMENT"; } | grep -c .)
  [ "$n" -eq 2 ] || return 1
}

@test "AC2: the generated principles door serves no status (Principle's shape declares none)" {
  ! grep -q 'name: "status"' "$ROOT/shaping/domains/principles/api/src/model.rs" || return 1
  ! grep -q 'row.insert("status"' "$ROOT/shaping/domains/principles/api/src/store.rs" || return 1
  ! grep -q 'row.insert("status"' "$ROOT/designing/services/athena-gen/templates/store.rs" || return 1
  # and its own tests prove it on rows: a row serves status only when declared, a status write is off-model
  grep -q 'fn a_row_serves_status_only_when_its_shape_declares_one' "$ROOT/shaping/domains/principles/api/src/embedded_tests.rs"
  grep -q 'fn a_write_of_status_to_a_class_without_one_is_refused_off_model' "$ROOT/shaping/domains/principles/api/src/embedded_tests.rs"
}
