#!/usr/bin/env bash
# test-git-queue-force-with-lease.sh — #2877 AC1 + AC2 verification.
#
# AC1 (Test 1): push --branch <ref> --force-with-lease lands a rewritten
#   branch on origin without falling back to env-var override. Pre-#2877
#   the parser only checked $1 for --force-with-lease, so with --branch
#   first the flag was silently dropped; push hit non-FF.
#
# AC2 (Test 2): lease semantics preserved. A concurrent peer push to
#   origin/<branch> between our last fetch and our push must cause
#   --force-with-lease to refuse, NOT silently overwrite. Pre-fix the
#   lease check ran AFTER pull --rebase's implicit fetch, so the lease
#   always saw a "fresh" view and approved every force.
#
# Both scenarios mirror the real #2844 path: branch tracks origin/main
# (chorus-werk repoint default), local rewrites diverge from origin/<branch>,
# the explicit --branch push target is what triggers the bug class.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GIT_QUEUE="${SCRIPT_DIR}/../../../platform/scripts/git-queue.sh"
PASS=0
FAIL=0

# #2856 canonical contract: emit results line on EXIT
trap 'echo "=== Results: $PASS passed, $FAIL failed ==="' EXIT

setup_origin_clone() {
  local origin_dir="$1"
  local clone_dir="$2"
  git init -q --bare "$origin_dir"
  git clone -q "$origin_dir" "$clone_dir"
  git -C "$clone_dir" config user.email "test@test.com"
  git -C "$clone_dir" config user.name "Test"
  echo "seed" > "$clone_dir/file.txt"
  git -C "$clone_dir" add file.txt
  git -C "$clone_dir" commit -q -m "seed"
  local default_branch
  default_branch=$(git -C "$clone_dir" symbolic-ref --short HEAD)
  if [ "$default_branch" != "main" ]; then
    git -C "$clone_dir" branch -m "$default_branch" main
  fi
  git -C "$clone_dir" push -q -u origin main
  # Stub branch-check so check_branch accepts kade/9999 in the fixture.
  mkdir -p "$clone_dir/platform/scripts"
  cat > "$clone_dir/platform/scripts/branch-check.sh" <<'STUB'
branch_check_match() { return 0; }
STUB
}

# --- Test 1 (AC1): push --branch X --force-with-lease lands rewritten branch ---
echo "Test 1: push --branch <ref> --force-with-lease lands rewritten branch"
ORIGIN_1=$(mktemp -d)
CLONE_1=$(mktemp -d)
setup_origin_clone "$ORIGIN_1" "$CLONE_1"
git -C "$CLONE_1" checkout -q -b kade/9999
echo "wave1" > "$CLONE_1/wave.txt"
git -C "$CLONE_1" add wave.txt && git -C "$CLONE_1" commit -q -m "wave 1"
echo "wave2" >> "$CLONE_1/wave.txt"
git -C "$CLONE_1" add wave.txt && git -C "$CLONE_1" commit -q -m "wave 2"
git -C "$CLONE_1" push -q origin kade/9999
git -C "$CLONE_1" branch --set-upstream-to=origin/main kade/9999
git -C "$CLONE_1" reset -q --hard main
echo "wave1" > "$CLONE_1/wave.txt"
echo "wave2" >> "$CLONE_1/wave.txt"
echo "squash-marker" > "$CLONE_1/squash.txt"
git -C "$CLONE_1" add wave.txt squash.txt
git -C "$CLONE_1" commit -q -m "squashed"
LOCAL_TIP_1=$(git -C "$CLONE_1" rev-parse HEAD)
set +e
push_out_1=$(cd "$CLONE_1" && DEPLOY_ROLE=kade _GIT_QUEUE_PUSH=1 bash "$GIT_QUEUE" push --force-branch --branch kade/9999 --force-with-lease 2>&1)
push_code_1=$?
set -e
ORIGIN_TIP_1=$(git -C "$ORIGIN_1" rev-parse refs/heads/kade/9999 2>/dev/null || echo "missing")
if [ "$push_code_1" = "0" ] && [ "$ORIGIN_TIP_1" = "$LOCAL_TIP_1" ]; then
  echo "  PASS: rewritten branch landed on origin via --force-with-lease"
  PASS=$((PASS+1))
else
  echo "  FAIL: push did not land (exit=$push_code_1, origin=$ORIGIN_TIP_1, local=$LOCAL_TIP_1)"
  echo "  output: $push_out_1"
  FAIL=$((FAIL+1))
fi
rm -rf "$ORIGIN_1" "$CLONE_1"

# --- Test 2 (AC2): lease refuses when origin diverged since last fetch ---
echo ""
echo "Test 2: --force-with-lease refuses when origin diverged since last fetch"
ORIGIN_2=$(mktemp -d)
CLONE_A=$(mktemp -d)
CLONE_B=$(mktemp -d)
setup_origin_clone "$ORIGIN_2" "$CLONE_A"
git -C "$CLONE_A" checkout -q -b kade/9999
echo "wave1" > "$CLONE_A/wave.txt"
git -C "$CLONE_A" add wave.txt && git -C "$CLONE_A" commit -q -m "wave 1"
git -C "$CLONE_A" push -q origin kade/9999
git -C "$CLONE_A" branch --set-upstream-to=origin/main kade/9999
git -C "$CLONE_A" reset -q --hard main
echo "squashed" > "$CLONE_A/squashed.txt"
git -C "$CLONE_A" add squashed.txt
git -C "$CLONE_A" commit -q -m "squashed"
# Concurrent peer push to origin/kade/9999 — Clone A's view is now stale
git clone -q "$ORIGIN_2" "$CLONE_B"
git -C "$CLONE_B" config user.email "b@test.com"
git -C "$CLONE_B" config user.name "B"
git -C "$CLONE_B" checkout -q kade/9999
echo "peer" > "$CLONE_B/peer.txt"
git -C "$CLONE_B" add peer.txt
git -C "$CLONE_B" commit -q -m "peer concurrent"
git -C "$CLONE_B" push -q origin kade/9999
ORIGIN_BEFORE_2=$(git -C "$ORIGIN_2" rev-parse refs/heads/kade/9999)
set +e
push_out_2=$(cd "$CLONE_A" && DEPLOY_ROLE=kade _GIT_QUEUE_PUSH=1 bash "$GIT_QUEUE" push --force-branch --branch kade/9999 --force-with-lease 2>&1)
push_code_2=$?
set -e
ORIGIN_AFTER_2=$(git -C "$ORIGIN_2" rev-parse refs/heads/kade/9999)
if [ "$push_code_2" != "0" ] && [ "$ORIGIN_BEFORE_2" = "$ORIGIN_AFTER_2" ]; then
  echo "  PASS: stale-lease push refused; origin preserved peer commit"
  PASS=$((PASS+1))
else
  echo "  FAIL: stale-lease push was NOT refused"
  echo "    exit=$push_code_2, origin_before=$ORIGIN_BEFORE_2, origin_after=$ORIGIN_AFTER_2"
  echo "    output: $push_out_2"
  FAIL=$((FAIL+1))
fi
rm -rf "$ORIGIN_2" "$CLONE_A" "$CLONE_B"

if [ "$FAIL" -gt 0 ]; then exit 1; fi
exit 0
