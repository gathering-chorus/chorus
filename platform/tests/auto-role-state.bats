#!/usr/bin/env bats
# auto-role-state.bats — verify card actions auto-declare role state (#1782)
# Bug: roles forget to call role-state manually. State goes stale.
# Fix: sdk.ts move-to-WIP calls role-state building, done calls role-state idle.

SDK_SRC="/Users/jeffbridwell/CascadeProjects/chorus/directing/products/cards/src/sdk.ts"

@test "sdk.ts calls autoRoleState building on WIP entry" {
  grep -q "autoRoleState('building'" "$SDK_SRC"
}

@test "sdk.ts calls autoRoleState idle on Done" {
  grep -q "autoRoleState('idle'" "$SDK_SRC"
}

@test "sdk.ts imports child_process for role-state calls" {
  grep -q 'child_process\|spawnSync\|execSync' "$SDK_SRC"
}
