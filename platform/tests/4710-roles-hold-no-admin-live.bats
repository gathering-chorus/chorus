#!/usr/bin/env bats
# @test-type: smoke — live: a role account holds no GitHub credential, and GitHub refuses it
# @domain: security
# @needs: machine — run as a role account (chorus-<role>), with GitHub reachable
# @card: #4710 · owner: wren
# What Jeff sees: no role can lift main's protection, post his go or push main. Borg Queen's
# bypass 1 (10-23): every role ran as Jeff with his admin login. Roles now run as their own Mac
# accounts; measured as chorus-wren 11-11 17:10: `gh` is not logged in. This checks that each
# of the three moves is refused by GitHub from the role account.
REPO="${CHORUS_REPO:-gathering-chorus/chorus}"
API="${GITHUB_API:-https://api.github.com}"

# prints every GitHub credential this account could hand to a call, by name only
credentials() {
  [ -n "${GH_TOKEN:-}" ] && echo "GH_TOKEN is set"
  [ -n "${GITHUB_TOKEN:-}" ] && echo "GITHUB_TOKEN is set"
  if command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then echo "gh is logged in"; fi
  grep -qs 'oauth_token' "${GH_CONFIG_DIR:-$HOME/.config/gh}/hosts.yml" && echo "gh's hosts file holds a token"
  local cred
  cred="$(printf 'protocol=https\nhost=github.com\n\n' | GIT_TERMINAL_PROMPT=0 git credential fill 2>/dev/null | grep -c '^password=' || true)"
  [ "${cred:-0}" -gt 0 ] && echo "git has a credential for github.com"
  return 0
}

@test "AC1: this role account holds no GitHub credential" {
  case "$(id -un)" in chorus-*) ;; *) echo "unmeasured: run as a role account, not $(id -un)"; return 1;; esac
  out="$(credentials)"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC1: GitHub refuses the three moves from this account (protection, jeff-go, push)" {
  case "$(id -un)" in chorus-*) ;; *) echo "unmeasured: run as a role account, not $(id -un)"; return 1;; esac
  p="$(curl -s -m 15 -o /dev/null -w '%{http_code}' -X PUT "$API/repos/$REPO/branches/main/protection" -d '{}')"
  s="$(curl -s -m 15 -o /dev/null -w '%{http_code}' -X POST "$API/repos/$REPO/statuses/0000000000000000000000000000000000000000" -d '{"state":"success","context":"jeff-go"}')"
  case "$p" in 401|403|404) ;; *) echo "protection change answered $p"; return 1;; esac
  case "$s" in 401|403|404) ;; *) echo "a jeff-go status answered $s"; return 1;; esac
  run env GIT_TERMINAL_PROMPT=0 git push --dry-run "https://github.com/$REPO.git" HEAD:refs/heads/main
  [ "$status" -ne 0 ] || { echo "a push to main was accepted"; return 1; }
}

@test "AC1 negative proof: a set GH_TOKEN (the 10-23 state, Jeff's admin login) is caught" {
  out="$(GH_TOKEN=x credentials)"
  [[ "$out" == *"GH_TOKEN is set"* ]] || { echo "$out"; return 1; }
}

@test "AC1 negative proof: a planted gh hosts file with a token in a fixture home is caught" {
  h="$BATS_TEST_TMPDIR/home"; mkdir -p "$h/.config/gh"
  printf 'github.com:\n    oauth_token: PLANTED\n    user: x\n' > "$h/.config/gh/hosts.yml"
  out="$(HOME="$h" GH_CONFIG_DIR="$h/.config/gh" credentials)"
  [[ "$out" == *"gh's hosts file holds a token"* ]] || { echo "$out"; return 1; }
  [[ "$out" != *PLANTED* ]] || { echo "the token reached the output"; return 1; }
}
