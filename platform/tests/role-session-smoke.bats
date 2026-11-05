#!/usr/bin/env bats
# @test-type: e2e — starts a real headless session of each changed role from this tree
# @domain: roles
# @card: #4889 · owner: silas
# What Jeff sees: a card that changes a role's session config (its CLAUDE.md, its
# .claude settings and hooks, the shared fragments) is proven to sign in
# and make one tool call from the card's own tree, in the card's one test run, before
# his go. Moved from the demo's session-smoke step (#4502) by #4889 (Kade 19:37:
# "smoke → into werk-test, the one test run").
#
# werk-test selects a suite by the paths it names, so these are the files a session
# loads (a fragment change regenerates its role's CLAUDE.md in the same diff):
#   roles/wren/CLAUDE.md roles/silas/CLAUDE.md roles/kade/CLAUDE.md
#   roles/wren/.claude/settings.json roles/silas/.claude/settings.json
#   roles/kade/.claude/settings.json .claude/settings.json
#
# Not covered here: a skill change (one tool call never loads a skill) and a
# chorus-hooks source change (the hooks crate's own tests run for it).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CLAUDE_ROLES="wren silas kade"   # abby-normal and borg-queen run on agy, not claude

# the roles whose session config these changed paths touch, one per line
roles_to_smoke() {
  local changed="$1" r
  if grep -qE '^(designing/claudemd/shared/|\.claude/)' <<<"$changed"; then
    for r in $CLAUDE_ROLES; do echo "$r"; done
  fi
  for r in wren silas kade abby-normal borg-queen; do
    grep -qE "^(roles/$r/|designing/claudemd/roles/$r/)" <<<"$changed" && echo "$r"
  done
}

# ok when the reply carries the nonce only the tool call can read
reply_proves_the_call() { [[ "$1" == *"$2"* ]]; }

smoke_one() {
  local r="$1" nonce file out
  nonce="smoke-$r-$$-$RANDOM$RANDOM"
  # inside the session's own folder: a role session may only read its working dirs
  file=".smoke-nonce-$$"
  printf '%s\n' "$nonce" > "$ROOT/roles/$r/$file"
  out="$(cd "$ROOT/roles/$r" && CHORUS_ROLE="$r" ROLE="$r" CHORUS_HEADLESS=1 \
    perl -e 'alarm shift; exec @ARGV' "${CHORUS_SMOKE_TIMEOUT_SECS:-180}" \
    claude -p "Run exactly one Bash command, \`cat $file\`, and reply with its output only." \
      --no-session-persistence --output-format text --allowedTools "Bash(cat $file)" 2>&1)"
  local rc=$?
  rm -f "$ROOT/roles/$r/$file"
  [ "$rc" -eq 0 ] || { echo "$r: session exited $rc: $(tail -1 <<<"$out")"; return 1; }
  reply_proves_the_call "$out" "$nonce" || { echo "$r: the tool call did not come back: $(tail -1 <<<"$out")"; return 1; }
}

@test "each changed role's session, from this tree, signs in and makes one tool call" {
  changed="$(git -C "$ROOT" diff --name-only origin/main...HEAD)"
  roles="$(roles_to_smoke "$changed" | sort -u)"
  [ -n "$roles" ] || { echo "no role session config in this diff: nothing to smoke"; return 0; }
  failed=0
  for r in $roles; do
    if [[ " $CLAUDE_ROLES " != *" $r "* ]]; then echo "$r: runs on agy, unmeasured here"; continue; fi
    smoke_one "$r" || failed=1
  done
  [ "$failed" -eq 0 ]
}

@test "the roles picked: a role's own files pick that role; shared config picks every claude role" {
  [ "$(roles_to_smoke 'roles/kade/CLAUDE.md')" = "kade" ]
  [ "$(roles_to_smoke 'designing/claudemd/roles/wren/x.md')" = "wren" ]
  [ "$(roles_to_smoke '.claude/settings.json' | sort -u | tr '\n' ' ')" = "kade silas wren " ]
  [ -z "$(roles_to_smoke 'skills/demo/SKILL.md')" ]
  [ -z "$(roles_to_smoke 'platform/services/werk-demo/src/lib.rs')" ]
}

@test "negative proof: a reply without the nonce (answered without running the tool) fails" {
  reply_proves_the_call "the file says smoke-x-1" "smoke-x-1"
  ! reply_proves_the_call "I ran it, it printed the token" "smoke-x-1" || return 1
}
