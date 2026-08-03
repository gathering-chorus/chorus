#!/usr/bin/env bats
# @test-type: integration — reads the HOST's installed LaunchAgents; skip-if-absent
load test_helper
#
# #3734 — "is this script actually reachable?" Nothing in the repo could answer it,
# which is how platform/scripts/nightly-coverage.sh survived as an orphan long
# enough for an audit to read it as the live coverage gate and report its
# behaviour as the system's.
#
# WHY THIS IS INTEGRATION TIER AND NOT UNIT. The question is host-specific by
# nature, and I tried both portable framings before accepting that:
#
#   - "installed on this machine" (~/Library/LaunchAgents) — meaningless on CI,
#     where nothing is installed, so every reference reads as absent. This file
#     was unit-tier for exactly one CI run and failed for that reason.
#   - "declared in the repo" (proving/config/launchagents/) — also unreliable:
#     com.chorus.nightly-suites.plist is INSTALLED AND RUNNING on the role machine
#     and is NOT in the repo's declared set. The repo's plist inventory is itself
#     incomplete — its own missing denominator.
#
# Neither source is authoritative alone, so the honest scope is the machine where
# the question means something: the role box, where the nightly runs. Skipping on
# CI is not decoration here — it runs nightly on the host that matters, and on its
# first run it found THREE scripts referencing LaunchAgents that do not exist.

AGENTS="$HOME/Library/LaunchAgents"

# Allowlisted with a reason, not silently excluded. Each entry is visible debt and
# comes out when its owner rules retire-or-schedule. I have NOT verified these two
# dead — a script may legitimately reference a plist not installed on this box, and
# deleting someone else's script on that inference is the leap #3734 exists to stop.
ALLOW="com.chorus.bridge-subscriber-watchdog.plist com.chorus.werk-sweep.plist"

setup() {
  [ -d "$AGENTS" ] || skip "no $AGENTS on this host — reachability is a role-machine question"
  ls "$AGENTS"/com.chorus.*.plist >/dev/null 2>&1 || skip "no chorus LaunchAgents installed — not a role machine"
}

_orphans() {
  local root="$1" allow="$2" missing=""
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    while IFS= read -r p; do
      [ -n "$p" ] || continue
      case " $allow " in *" $p "*) continue ;; esac
      [ -f "$AGENTS/$p" ] || missing="$missing $p(→$(basename "$f"))"
    done < <(grep -o "com\.chorus\.[a-z-]*\.plist" "$f" | sort -u)
  done < <(grep -rl "com\.chorus\.[a-z-]*\.plist" "$root" 2>/dev/null || true)
  printf '%s' "$missing"
}

@test "no script claims a LaunchAgent that does not exist on this host" {
  local missing; missing=$(_orphans "${CHORUS_ROOT}/platform/scripts" "$ALLOW")
  [ -z "$missing" ] || { echo "scripts reference absent LaunchAgents:$missing"; false; }
}

@test "the reachability sweep still fires on an unallowlisted orphan" {
  # Negative proof (#3734 AC5): an allowlist that swallowed everything would pass
  # forever and prove nothing. Plant a script naming a plist that is neither
  # installed nor allowlisted, and assert the sweep sees it.
  local probe="$BATS_TEST_TMPDIR/scripts"
  mkdir -p "$probe"
  printf '#!/bin/bash\n# launchctl load com.chorus.definitely-not-installed.plist\n' > "$probe/orphan.sh"

  local missing; missing=$(_orphans "$probe" "$ALLOW")
  [ -n "$missing" ]
  echo "$missing" | grep -q "definitely-not-installed"
}

@test "the allowlist does not swallow an entry it was not given" {
  # Second negative proof, on the allowlist itself: it must match exactly, not by
  # prefix or substring. A sloppy match would quietly absorb neighbours — the same
  # laundering shape as a prefix that swallowed a singular/plural graph typo.
  local probe="$BATS_TEST_TMPDIR/scripts2"
  mkdir -p "$probe"
  printf '#!/bin/bash\n# com.chorus.werk-sweep-extra.plist\n' > "$probe/near.sh"

  local missing; missing=$(_orphans "$probe" "$ALLOW")
  [ -n "$missing" ]
  echo "$missing" | grep -q "werk-sweep-extra"
}
