#!/usr/bin/env bats
# @test-type: unit — asserts the gate's scope patterns still match real files;
# no service, store or credential is exercised.
#
# #3785 — the allow-set gate is SCOPED, and this is the canary for that scope.
#
# The gate fires only on deploys whose changed files look identity-related. That
# scoping is deliberate and Wren argued the stronger case for it than I did:
# always-on would couple EVERY land to Fuseki being reachable, so a store outage
# would block landing unrelated fixes — including the fix for the outage.
#
# But a path heuristic is exactly the kind of check that stops matching one day
# and says nothing. Rename solid-auth.ts, and the gate silently stops applying to
# the Clearing's door; nothing goes red, nothing is logged, and the protection is
# gone while the code that "has" it is still there.
#
# So: for every pattern the heuristic uses, assert a real file currently matches
# it. A rename turns silent-stop into a red test — the #3734 discipline applied
# to the scope rather than to the gate.

setup() {
  ROOT="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"   # #3904: derive, never hardcode a /Users path
  DEPLOY_SRC="$ROOT/platform/services/werk-deploy/src/lib.rs"
}

# Every pattern in deploy_canonical's touches_identity check. Kept as a literal
# list on purpose: if someone adds a pattern there and not here, the last test
# in this file fails and says so.
# #3561 renamed owl-api → athena-make; the heuristic in werk-deploy moved with it
# and this canary list did not, so the pattern guarded nothing.
PATTERNS=(chorus-oidc solid-auth identity-principals security-model athena-make/src/lib.rs)

@test "the scope check still exists in the deploy path" {
  grep -q "touches_identity" "$DEPLOY_SRC"
}

@test "CANARY: every scope pattern matches at least one real file today" {
  for pat in "${PATTERNS[@]}"; do
    run bash -c "cd '$ROOT' && git ls-files | grep -q -- '$pat'"
    [ "$status" -eq 0 ] || {
      echo "scope pattern '$pat' matches NO file in the repo." >&2
      echo "The gate has silently stopped applying to whatever that pattern guarded." >&2
      echo "Either the file was renamed (update both the heuristic and this list)," >&2
      echo "or the surface is gone (drop the pattern deliberately, not by decay)." >&2
      return 1
    }
  done
}

@test "NEGATIVE PROOF: the canary fails on a pattern that matches nothing" {
  # Without this, the test above would pass against a broken matcher just as
  # happily as against a healthy one — the two states it exists to separate.
  run bash -c "cd '$ROOT' && git ls-files | grep -q -- 'no-such-surface-3785-canary'"
  [ "$status" -ne 0 ]
}

@test "CANARY: the heuristic in code carries no pattern this file does not know about" {
  # Catches the other drift direction: a pattern added to the deploy heuristic
  # but never given a canary, which would be protection nobody is watching.
  # Range ends at the closure's own '});' — a looser end marker swept in the
  # gate invocation below it and matched the script path as if it were a
  # pattern. The first version of this test failed for exactly that reason.
  block=$(awk '/let touches_identity/{f=1} f{print} f&&/^    \}\);/{exit}' "$DEPLOY_SRC")
  [ -n "$block" ]
  # every quoted literal inside the block must appear in PATTERNS
  while read -r lit; do
    [ -z "$lit" ] && continue
    found=0
    for pat in "${PATTERNS[@]}"; do [ "$pat" = "$lit" ] && found=1; done
    [ "$found" -eq 1 ] || {
      echo "heuristic contains '$lit' with no canary in this file." >&2
      return 1
    }
  done < <(printf '%s' "$block" | grep -oE '"[^"]+"' | tr -d '"')
}
