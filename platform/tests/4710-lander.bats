#!/usr/bin/env bats
# @test-type: integration — the lander (chorus-go as chorus-lander) end to end in the werk: its GitHub half, its own login, its card filing, its store, the programs it may run, and its install
# @domain: security
# @card: #4710 · owner: wren
# What Jeff sees: only his Face ID go lands a card. AC3: the lander, in its own account, serves the
# go page, verifies his passkey go, posts jeff-go and merges as the GitHub App, and runs nothing
# jeffbridwell or a role can write. Each suite below carries its own negative proofs.
load test_helper
GO="$CHORUS_ROOT/platform/services/chorus-go"

cargo_suite() {
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$GO/target/$(id -un)" nice cargo test -q --manifest-path "$GO/Cargo.toml" --test "$1"
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  [[ "$output" == *"test result: ok"* ]] || { echo "$output" | tail -5; return 1; }
}

@test "AC3: it runs only /usr/bin programs and no env knob names one (exec_guard_4710)" { cargo_suite exec_guard_4710; }
@test "AC3: it merges as the App — status then squash at the signed sha; a moved head merges nothing (github_4710)" { cargo_suite github_4710; }
@test "AC3: a go merges before its step is answered; a refused merge leaves the step open (github_4710)" { cargo_suite github_4710; }
@test "AC3: its own door login, 0600 and bound to its account, never on argv (identity_4710)" { cargo_suite identity_4710; }
@test "AC3: an approved card is filed through the MCP door as the lander (mcp_4710)" { cargo_suite mcp_4710; }
@test "AC3: Jeff's key in the lander's public dir, unswappable; no key refuses (public_store_4710)" { cargo_suite public_store_4710; }
@test "AC3: a lander-owned decision row lands only with Jeff's signature (approvals_4642)" { cargo_suite approvals_4642; }
@test "AC3: a werk's git config cannot run a program as the lander (werk_head_4735)" { cargo_suite werk_head_4735; }

@test "AC3: the install script, against a fake root (4710-lander-install.bats)" {
  run bats "$CHORUS_ROOT/platform/tests/4710-lander-install.bats"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
