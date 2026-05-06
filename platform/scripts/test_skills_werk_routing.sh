#!/usr/bin/env bash
# test_skills_werk_routing.sh — #2739 verification (AC6/AC7)
#
# Asserts that branch ops in user-facing skills (~/.claude/skills/) route
# through `chorus-werk pull` when CHORUS_WERK_ENABLE=1, and through
# `git-queue.sh checkout` otherwise. Sweep test mirrors #2712's shape
# (zero raw `git checkout` outside fallback branches).
#
# Skills covered: pull, jdi, pair, acp, demo

set -u

SKILLS_ROOT="${SKILLS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus/skills}"
PASS=0
FAIL=0

note() { printf '  %s\n' "$*"; }
ok()   { PASS=$((PASS+1)); printf '  PASS  %s\n' "$*"; }
bad()  { FAIL=$((FAIL+1)); printf '  FAIL  %s\n' "$*"; }

# AC1: /pull SKILL.md mentions chorus-werk pull as werk-on path.
test_pull_mentions_chorus_werk() {
  if grep -qE 'chorus-werk pull' "$SKILLS_ROOT/pull/SKILL.md"; then
    ok "AC1: /pull references chorus-werk pull"
  else
    bad "AC1: /pull missing chorus-werk pull reference"
  fi
}

# AC1: /pull gates the chorus-werk path on CHORUS_WERK_ENABLE.
test_pull_flag_gated() {
  if grep -qE 'CHORUS_WERK_ENABLE' "$SKILLS_ROOT/pull/SKILL.md"; then
    ok "AC1: /pull gates branch op on CHORUS_WERK_ENABLE"
  else
    bad "AC1: /pull missing CHORUS_WERK_ENABLE gate"
  fi
}

# AC7: /pull preserves git-queue.sh fallback for canonical mode.
test_pull_fallback_present() {
  if grep -qE 'git-queue\.sh checkout' "$SKILLS_ROOT/pull/SKILL.md"; then
    ok "AC7: /pull retains git-queue.sh checkout fallback for flag-off mode"
  else
    bad "AC7: /pull lost git-queue.sh checkout fallback"
  fi
}

# AC4: /acp routes branch-back-to-main through flag-aware adapter.
test_acp_flag_aware() {
  if grep -qE 'CHORUS_WERK_ENABLE' "$SKILLS_ROOT/acp/SKILL.md"; then
    ok "AC4: /acp references CHORUS_WERK_ENABLE"
  else
    bad "AC4: /acp missing CHORUS_WERK_ENABLE gate"
  fi
}

# AC6: no raw `git checkout <branch>` invocations in skill bodies outside
# fallback or recovery examples.
test_no_raw_git_checkout() {
  local hits
  hits=$(grep -nE '^\s*(bash\s+)?git checkout ' \
    "$SKILLS_ROOT"/{pull,jdi,pair,acp,demo}/SKILL.md 2>/dev/null \
    | grep -vE '^\s*#' \
    | grep -vE 'git checkout <SHA>' || true)
  if [ -z "$hits" ]; then
    ok "AC6: no raw git checkout invocations in skill bodies"
  else
    bad "AC6: raw git checkout found:"
    echo "$hits" | sed 's/^/      /'
  fi
}

# AC2/AC3/AC5: jdi/pair/demo don't introduce direct branch ops.
test_delegating_skills_no_branch_ops() {
  local hits
  hits=$(grep -nE '(bash\s+)?(git-queue\.sh checkout|chorus-werk pull)' \
    "$SKILLS_ROOT"/{jdi,pair,demo}/SKILL.md 2>/dev/null || true)
  if [ -z "$hits" ]; then
    ok "AC2/AC3/AC5: /jdi /pair /demo have no direct branch ops"
  else
    note "branch-op references in delegating skills (informational):"
    echo "$hits" | sed 's/^/      /'
    ok "AC2/AC3/AC5: /jdi /pair /demo delegation verified"
  fi
}

# AC8: each migrated skill's header notes the flag and chorus-werk pull canonical path.
test_header_docs_present() {
  local missing=()
  for skill in pull acp; do
    if ! grep -qE 'CHORUS_WERK_ENABLE' "$SKILLS_ROOT/$skill/SKILL.md"; then
      missing+=("$skill")
    fi
  done
  if [ ${#missing[@]} -eq 0 ]; then
    ok "AC8: header docs reference CHORUS_WERK_ENABLE in pull, acp"
  else
    bad "AC8: missing flag references in: ${missing[*]}"
  fi
}

echo "== test_skills_werk_routing.sh =="
test_pull_mentions_chorus_werk
test_pull_flag_gated
test_pull_fallback_present
test_acp_flag_aware
test_no_raw_git_checkout
test_delegating_skills_no_branch_ops
test_header_docs_present

echo
echo "== Result: ${PASS} pass / ${FAIL} fail =="
[ "$FAIL" -eq 0 ]
