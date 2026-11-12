#!/usr/bin/env bats
# @test-type: unit — runs werk-demo's policy-gate tests (a fake policy-validate in a temp home); no store, no model, no network.
# @domain: cicd
# @card: #4959
# @owner: silas
#
# #4959 AC4 — every demo review runs the policies for its gate: code, arch and quality
# inside run-gate (after the breaks split, so a policy failure is never a note), data
# and product inside validate (deterministic, they refuse the demo). The cases live in
# platform/services/werk-demo/tests/policy_gates_4959.rs; this file runs them and
# fails if any is missing, so a deleted case cannot pass by omission.

ROOT="$BATS_TEST_DIRNAME/../.."
CRATE="$ROOT/platform/services/werk-demo"

@test "werk-demo's policy-gate cases all run and pass" {
  run cargo test --manifest-path "$CRATE/Cargo.toml" --test policy_gates_4959 -- --test-threads=1
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
  # the negative proofs must be among what ran, not filtered or deleted
  for t in negative_proof_exit_1_fails_and_names_each_policy_over_its_baseline \
           negative_proof_exit_2_or_a_crash_is_an_error_never_a_pass \
           negative_proof_a_policy_failure_survives_a_passing_review \
           negative_proof_no_checker_on_canonical_or_in_the_werk_is_an_error_never_a_pass \
           negative_proof_validate_refuses_when_a_product_policy_is_over_its_baseline \
           negative_proof_validate_refuses_when_a_data_policy_cannot_measure \
           negative_proof_every_failure_is_labelled_so_none_leaks_into_the_ledger \
           the_canonical_checker_wins_over_the_werks_copy \
           the_checker_runs_in_the_cards_werk; do
    echo "$output" | grep -q "test $t ... ok" || { echo "missing or not ok: $t"; return 1; }
  done
}

@test "the review folds policies in after the breaks split, and validate runs data and product" {
  src="$CRATE/src/lib.rs"
  breaks=$(grep -n 'apply_breaks(&result, &findings, classify_breaks' "$src" | head -1 | cut -d: -f1)
  folds=$(grep -n 'with_policies(&result, &findings, policy_review(' "$src" | head -1 | cut -d: -f1)
  [ -n "$breaks" ] && [ -n "$folds" ] || return 1
  [ "$folds" -gt "$breaks" ] || return 1
  grep -q 'deterministic_gates(home, Some(&werk))' "$src" || return 1
}
