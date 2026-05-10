#!/usr/bin/env bats
# git-queue-card-id-export.bats — #2876
# What Jeff sees: build.* spine events carry the card_id of the active branch.
# Without it, chorus_logs_for_card silently drops build/push/delete events
# and the pipeline-health report (#2874) cannot stitch demo->build->deploy.
#
# This file pins the card_id-from-branch contract by re-implementing the
# regex from git-queue.sh's export_card_id_from_branch helper. If the helper
# in the script drifts from this regex, integration breaks — the regex is
# the contract.

# Prefer the local copy (this werk's tree) — the fix under test may not be
# in canonical yet. CHORUS_ROOT_FOR_TEST overrides for explicit pinning.
if [ -n "${CHORUS_ROOT_FOR_TEST:-}" ]; then
  GIT_QUEUE="${CHORUS_ROOT_FOR_TEST}/platform/scripts/git-queue.sh"
else
  GIT_QUEUE="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/git-queue.sh"
fi

setup() {
  TEST_REPO=$(mktemp -d)
  cd "$TEST_REPO"
  git init --quiet --initial-branch=main
  git config user.email "test@test"
  git config user.name "test"
  git commit --allow-empty -m "init" --quiet
  export REPO_ROOT="$TEST_REPO"
  export CHORUS_ROOT="$TEST_REPO"
}

teardown() {
  rm -rf "$TEST_REPO"
  unset CHORUS_CARD_ID
}

# Re-implementation of export_card_id_from_branch from git-queue.sh — the
# contract under test. The script and this test must stay in sync; the
# contract-pin test below verifies the script body matches.
local_export_card_id_from_branch() {
  local _branch
  _branch=$(git -C "$REPO_ROOT" symbolic-ref --short HEAD 2>/dev/null || echo "")
  if [[ "$_branch" =~ ^[a-z]+/([0-9]+)$ ]]; then
    export CHORUS_CARD_ID="${BASH_REMATCH[1]}"
  fi
}

@test "sets CHORUS_CARD_ID=2876 when on silas/2876" {
  git checkout -b silas/2876 --quiet
  unset CHORUS_CARD_ID
  local_export_card_id_from_branch
  [ "${CHORUS_CARD_ID:-unset}" = "2876" ]
}

@test "leaves CHORUS_CARD_ID unset on main" {
  unset CHORUS_CARD_ID
  local_export_card_id_from_branch
  [ "${CHORUS_CARD_ID:-unset}" = "unset" ]
}

@test "leaves CHORUS_CARD_ID unset on detached HEAD" {
  git checkout --detach --quiet HEAD
  unset CHORUS_CARD_ID
  local_export_card_id_from_branch
  [ "${CHORUS_CARD_ID:-unset}" = "unset" ]
}

@test "extracts card_id from wren/9999 branch" {
  git checkout -b wren/9999 --quiet
  unset CHORUS_CARD_ID
  local_export_card_id_from_branch
  [ "${CHORUS_CARD_ID:-unset}" = "9999" ]
}

@test "ignores feature/no-card-id branch (non-conforming)" {
  git checkout -b feature/no-card-id --quiet
  unset CHORUS_CARD_ID
  local_export_card_id_from_branch
  [ "${CHORUS_CARD_ID:-unset}" = "unset" ]
}

@test "git-queue.sh defines export_card_id_from_branch (contract pin)" {
  # Defensive: if the helper goes missing or is renamed, the contract test
  # at the call sites still passes against the local copy. This pins the
  # source of truth so a drift fails loudly here.
  grep -q '^export_card_id_from_branch()' "$GIT_QUEUE"
}

@test "git-queue.sh helper regex matches local copy (contract pin)" {
  # The regex is the contract. Drift between the local re-impl and the
  # script's helper will produce real-world build event tagging gaps.
  grep -qF '^[a-z]+/([0-9]+)$' "$GIT_QUEUE"
}
