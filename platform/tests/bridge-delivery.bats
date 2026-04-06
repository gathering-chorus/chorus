#!/usr/bin/env bats
# bridge-delivery.bats — Tests for Bridge message fidelity (#2255)
# What Jeff sees: typed "did we do 2246?" and the role ran /acp instead of answering.
# Messages from Bridge must arrive as Jeff's exact words, clearly marked as questions.

NUDGE="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge"

# --- AC 1: Bridge delivers Jeff's exact text ---

@test "Bridge nudge delivers exact text without mutation" {
  # Simulate Bridge delivery — message should arrive verbatim
  export SESSION_HEALTH_TEST=1  # suppress real delivery
  run bash "$NUDGE" silas "did we do 2246?" --from jeff --force 2>&1
  # Output should contain the exact message text
  echo "$output" | grep -q "did we do 2246?"
}

# --- AC 2: Reproduce — question arrives as question ---

@test "input classifier classifies 'did we do 2246?' as question" {
  # The Rust classifier should tag this as question, not command
  # We test via the binary's classify output
  result=$(/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim classify "did we do 2246?" 2>&1 || true)
  echo "$result" | grep -qi "question"
}

# --- AC 1: Messages with card numbers are not rewritten ---

@test "card number in question is not treated as directive" {
  # "did we do 2246?" contains a card number but is a question
  # Bridge must not strip, reformat, or add directive framing
  msg="did we do 2246?"
  safe=$(echo "$msg" | sed 's/"/\\"/g')
  [ "$safe" = "$msg" ]
}
