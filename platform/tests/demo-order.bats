#!/usr/bin/env bats
# @test-type: unit
# @card: #4875
# @domain: decisions
# Demo order: show, prove, vouch, then go. The automated announce goes to the owner only.
# Log evidence (Loki, platform-chorus, 2026-11-03 06:58:45): "#4873 · gates ✗ · AC 0/2 ·
# decide: …/go/approvals" was sent with two red gates and nothing shown.

LIB="${BATS_TEST_DIRNAME}/../services/werk-demo/src/lib.rs"

announce_block() {
  # The announce block: from `if !announce_silent(` to the end of the demo run.
  awk '/if !announce_silent\(/{p=1} p{print} p&&/Ok\(DemoOutcome/{exit}' "$LIB"
}

@test "AC1/AC4: the announce block exists (a renamed or deleted target fails loudly)" {
  run announce_block
  [ "$status" -eq 0 ]
  [ -n "$output" ]
  [[ "$output" == *"announce_silent"* ]] || return 1
}

@test "AC1/AC4: the automated announce posts no go row" {
  run announce_block
  [[ "$output" != *"post_go_approval"* ]] || return 1
}

@test "AC1/AC4: the automated announce carries no decide/sign link" {
  run announce_block
  [[ "$output" != *"go_ask("* ]] || return 1
}

@test "AC1: the automated announce sends the owner brief" {
  run announce_block
  [[ "$output" == *"owner_brief("* ]] || return 1
}

@test "negative proof: the same checks go red on the announce that sent #4873's go (main before #4875)" {
  # The real violation, not a fixture: the announce block at e5516e266 (main before this card).
  local repo; repo="$(git -C "$BATS_TEST_DIRNAME" rev-parse --show-toplevel)"
  local old
  old="$(git -C "$repo" show e5516e266:platform/services/werk-demo/src/lib.rs \
    | awk '/if !announce_silent\(/{p=1} p{print} p&&/Ok\(DemoOutcome/{exit}')"
  [ -n "$old" ]
  [[ "$old" == *"post_go_approval"* ]] || return 1
  [[ "$old" == *"go_ask("* ]] || return 1
  [[ "$old" != *"owner_brief("* ]] || return 1
}
