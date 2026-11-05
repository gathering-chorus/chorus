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

# one session of role $1, started in $2 (default: this tree's roles/$1), one at a time.
# Caged like every werk-test run: the pulse door is a dead port, so a hook that would
# nudge or post reaches nothing live (#4628).
# #4889 (Jeff 20:55) — claude's path is the tool.path.claude property, which werk-test reads
# through the resolver and hands in; outside werk-test, set CHORUS_TOOL_CLAUDE. No fallback.
smoke_one() {
  [ -n "${CHORUS_TOOL_CLAUDE:-}" ] || { echo "no claude: ${CHORUS_TOOL_CLAUDE_ERROR:-CHORUS_TOOL_CLAUDE is not set (werk-test sets it from the tool.path.claude property)}"; return 1; }
  local r="$1" dir="${2:-$ROOT/roles/$1}" nonce file out
  nonce="smoke-$r-$$-$RANDOM$RANDOM"
  # inside the session's own folder: a role session may only read its working dirs
  file=".smoke-nonce-$$"
  printf '%s\n' "$nonce" > "$dir/$file"
  # DEPLOY_ROLE is the role the hook shim hands the daemon (as deploy_role) when the
  # account is not chorus-<role>: dagu runs this as jeffbridwell, which hosts every
  # role, and without it the hooks refuse the call (#4889 run 3).
  out="$(cd "$dir" && CHORUS_ROLE="$r" DEPLOY_ROLE="$r" ROLE="$r" CHORUS_HEADLESS=1 \
    CHORUS_PULSE_URL=http://127.0.0.1:9/api/nudge \
    perl -e 'alarm shift; exec @ARGV' "${CHORUS_SMOKE_TIMEOUT_SECS:-180}" \
    "$CHORUS_TOOL_CLAUDE" -p "Run exactly one Bash command, \`cat $file\`, and reply with its output only." \
      --no-session-persistence --output-format text --allowedTools "Bash(cat $file)" 2>&1)"
  local rc=$?
  rm -f "$dir/$file"
  [ "$rc" -eq 0 ] || { echo "$r: session exited $rc: $(tail -1 <<<"$out")"; return 1; }
  # the whole reply: a refusal spans lines, and its last line alone said nothing (#4889 run 4)
  reply_proves_the_call "$out" "$nonce" || { echo "$r: the tool call did not come back (account $(id -un), dir $dir):"; printf '%s\n' "$out"; return 1; }
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

@test "negative proof: a role whose settings block its tool call turns the smoke red" {
  # a copy of kade's session folder whose settings add a hook that refuses every tool
  d="$BATS_TEST_TMPDIR/kade"; cp -R "$ROOT/roles/kade" "$d"
  python3 - "$d/.claude/settings.json" <<'PY'
import json, sys
p = sys.argv[1]; s = json.load(open(p))
s.setdefault("hooks", {})["PreToolUse"] = [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo planted-block >&2; exit 2"}]}]
json.dump(s, open(p, "w"))
PY
  run smoke_one kade "$d"
  [ "$status" -ne 0 ] || { echo "a blocked session passed the smoke: $output"; return 1; }
  # control: the same copy with its own settings passes
  cp "$ROOT/roles/kade/.claude/settings.json" "$d/.claude/settings.json"
  run smoke_one kade "$d"
  [ "$status" -eq 0 ] || { echo "control failed: $output"; return 1; }
}
