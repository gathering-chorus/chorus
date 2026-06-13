#!/bin/bash
# deploy-daemon-card.sh — sanctioned deploy for daemon-runtime cards (#2925 / #2927).
#
# Daemon-runtime cards (diff touches platform/api/src/**, mcp/server.ts,
# platform/services/chorus-hooks/**, directing/products/cards/src/**) cannot
# ship through chorus_acp/chorus_commit MCP — those tools ARE the daemon being
# changed. This wrapper is the out-of-band bootstrap path that #2923 / #2916
# executed manually 2026-05-14.
#
# Generalized via #2927: routes per deploy unit (chorus-api / chorus-hooks /
# cards-sdk) — defaults to diff-introspect against origin/main; --units <list>
# overrides for explicit subset.
#
# Per-unit authority (#2927 AC4): any role's DEPLOY_ROLE can invoke. DEC-022
# narrows to LaunchAgent / cdhash / TCC surfaces (Silas-only by domain);
# per-unit deploy authority follows the unit's domain owner.

set -u

# #2931 — globals visible to the EXIT trap. card_id is set in main() after
# arg parse; _deploy_role mirrors DEPLOY_ROLE once validated; _current_step
# is updated at each phase so the failure-emit names where we died.
card_id=""
_deploy_role=""
_current_step=""
_deploy_failed_emitted=""

# #2931 — failure-emit trap. Any non-zero exit emits deploy.failed with
# error=card=<id> exit=<N> step=<phase> so chorus_logs_for_card sees the
# failure event with context.
_emit_deploy_failed() {
  local exit_code="$?"
  if [ "$exit_code" -ne 0 ] && [ -z "$_deploy_failed_emitted" ]; then
    _deploy_failed_emitted=1
    local err_msg="card=${card_id:-unknown} exit=${exit_code} step=${_current_step:-unknown}"
    if command -v chorus-log >/dev/null 2>&1; then
      chorus-log deploy.failed "${_deploy_role:-silas}" \
        "domain=chorus" "result=fail" "error=$err_msg" "exit_code=$exit_code" >/dev/null 2>&1 || true
    fi
  fi
}
trap _emit_deploy_failed EXIT

usage() {
  cat <<'EOF'
Usage: deploy-daemon-card.sh <card-id> --probe "<smoke-command>" [--units <list>]

Sanctioned bootstrap deploy for daemon-runtime cards. Sequences:
  1. chorus-werk-sync                (pull canonical to current main)
  2. <per-unit deploy>               (route by --units, else diff-introspect)
  3. probe                            (verify new behavior live)
  4. cards done <card-id>             (only on probe success)

Units (any subset): chorus-api, chorus-hooks, cards-sdk

Default resolution = git diff origin/main introspection from the werk.
  --units <comma-list>  explicit override (subset / testing / staged rollout)
  --probe "<cmd>"       MANDATORY smoke check after deploy

DEPLOY_ROLE must be set (any of kade/wren/silas). Per-unit deploy authority
follows the unit's domain owner — Silas no longer hardcoded.
EOF
}

err() { echo "deploy-daemon-card: $*" >&2; }
info() { echo "deploy-daemon-card: $*"; }

CHORUS_ROOT_DEFAULT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_WERK_BASE_DEFAULT="${CHORUS_WERK_BASE:-/Users/jeffbridwell/CascadeProjects/chorus-werk}"

# =============================================================================
# Unit registry — path pattern + deploy/rollback function pair per unit.
# Each deploy_<unit> takes one arg: the werk path. Build runs inside werk;
# install goes to canonical paths so the running daemon picks it up.
# =============================================================================

KNOWN_UNITS="chorus-api chorus-hooks cards-sdk"

unit_pattern() {
  case "$1" in
    chorus-api)    echo '^platform/api/' ;;
    chorus-hooks)  echo '^platform/services/chorus-hooks/' ;;
    cards-sdk)     echo '^directing/products/cards/' ;;
    *)             return 1 ;;
  esac
}

# Pure: takes paths via stdin (one per line), emits matched units (one per line).
# Always returns 0 — the for-loop's last grep return code is NOT the function's
# exit code; non-match for the final unit must not propagate as failure.
match_units_against_paths() {
  local paths
  paths="$(cat)"
  [ -z "$paths" ] && return 0
  local u pat
  for u in $KNOWN_UNITS; do
    pat="$(unit_pattern "$u")"
    echo "$paths" | grep -qE "$pat" && echo "$u"
  done
  return 0
}

# --- chorus-api ---
deploy_chorus_api() {
  local werk="$1"
  local canon_dist="$CHORUS_ROOT_DEFAULT/platform/api/dist"
  [ -d "$werk/platform/api" ] || { err "chorus-api: werk missing platform/api"; return 1; }
  if [ -d "$canon_dist" ] && [ ! -d "${canon_dist}.prev" ]; then
    cp -a "$canon_dist" "${canon_dist}.prev" || return 1
  fi
  # #3361 — deploy is build+restart; node_modules is NOT rsynced, so a NEW
  # runtime dep (package.json change) never reaches canon and the service 500s
  # on require. Mirror app-state.sh's "package.json change → npm install" protocol
  # by installing deps in BOTH the werk (build + variant runtime) and canon (prod
  # runtime) from the committed lockfile. Idempotent/fast when nothing changed.
  ( cd "$werk/platform/api" && npm install --no-audit --no-fund ) || return 1
  ( cd "$werk/platform/api" && npm run build ) || return 1
  rsync -a --delete "$werk/platform/api/dist/" "$canon_dist/" || return 1
  ( cd "$CHORUS_ROOT_DEFAULT/platform/api" && npm install --no-audit --no-fund ) || return 1
  launchctl kickstart -k "gui/$UID/com.chorus.api" 2>&1 || true
  info "chorus-api: deployed (werk/dist -> canonical/dist + deps installed + kickstart)"
}
rollback_chorus_api() {
  local canon_dist="$CHORUS_ROOT_DEFAULT/platform/api/dist"
  if [ -d "${canon_dist}.prev" ]; then
    mv "$canon_dist" "${canon_dist}.failed.$$" 2>/dev/null || true
    mv "${canon_dist}.prev" "$canon_dist" || return 1
    launchctl kickstart -k "gui/$UID/com.chorus.api" 2>&1 || true
    info "chorus-api: rolled back from dist.prev"
  else
    err "chorus-api: no dist.prev to rollback to"
    return 1
  fi
}

# --- chorus-hooks ---
# CHORUS_BUILD_SIGNED env overrides the build script path (default: canonical's
# platform/scripts/build-signed.sh). Override is for tests + alternate-install
# scenarios; production leaves it unset.
deploy_chorus_hooks() {
  local werk="$1"
  [ -d "$werk/platform/services/chorus-hooks" ] || { err "chorus-hooks: werk missing crate dir"; return 1; }
  local b
  for b in chorus-hooks chorus-hook-shim; do
    if [ -f "$HOME/.chorus/bin/$b" ] && [ ! -f "$HOME/.chorus/bin/$b.prev" ]; then
      cp -a "$HOME/.chorus/bin/$b" "$HOME/.chorus/bin/$b.prev" || return 1
    fi
  done
  local build_signed="${CHORUS_BUILD_SIGNED:-$CHORUS_ROOT_DEFAULT/platform/scripts/build-signed.sh}"
  CHORUS_ROOT="$werk" bash "$build_signed" chorus-hooks || return 1
  launchctl kickstart -k "gui/$UID/com.chorus.hooks" 2>&1 || true
  info "chorus-hooks: deployed (build-signed.sh -> atomic install + kickstart)"
}
rollback_chorus_hooks() {
  local restored=0 b
  for b in chorus-hooks chorus-hook-shim; do
    if [ -f "$HOME/.chorus/bin/$b.prev" ]; then
      mv -f "$HOME/.chorus/bin/$b.prev" "$HOME/.chorus/bin/$b" || return 1
      restored=$((restored+1))
    fi
  done
  if [ "$restored" -gt 0 ]; then
    launchctl kickstart -k "gui/$UID/com.chorus.hooks" 2>&1 || true
    info "chorus-hooks: rolled back $restored binary(s) from .prev"
  else
    err "chorus-hooks: no .prev binaries to rollback to"
    return 1
  fi
}

# --- cards-sdk ---
deploy_cards_sdk() {
  local werk="$1"
  local canon_dist="$CHORUS_ROOT_DEFAULT/directing/products/cards/dist"
  local werk_sdk="$werk/directing/products/cards"
  [ -d "$werk_sdk" ] || { err "cards-sdk: werk missing directing/products/cards"; return 1; }
  if [ -d "$canon_dist" ] && [ ! -d "${canon_dist}.prev" ]; then
    cp -a "$canon_dist" "${canon_dist}.prev" || return 1
  fi
  ( cd "$werk_sdk" && npm run build ) || return 1
  rsync -a --delete "$werk_sdk/dist/" "$canon_dist/" || return 1
  info "cards-sdk: deployed (werk/dist -> canonical/dist; CLI reads on next invoke)"
}
rollback_cards_sdk() {
  local canon_dist="$CHORUS_ROOT_DEFAULT/directing/products/cards/dist"
  if [ -d "${canon_dist}.prev" ]; then
    mv "$canon_dist" "${canon_dist}.failed.$$" 2>/dev/null || true
    mv "${canon_dist}.prev" "$canon_dist" || return 1
    info "cards-sdk: rolled back from dist.prev"
  else
    err "cards-sdk: no dist.prev to rollback to"
    return 1
  fi
}

# =============================================================================
# Unit resolution — --units explicit OR diff-introspect from werk vs origin/main.
# AC5: zero-match → explicit refusal (NOT silent no-op) at the call site.
# =============================================================================

resolve_units_explicit() {
  local list="$1" u
  echo "$list" | tr ',' '\n' | while read -r u; do
    u="$(echo "$u" | xargs)"  # trim leading/trailing whitespace
    [ -z "$u" ] && continue
    if [[ " $KNOWN_UNITS " == *" $u "* ]]; then
      echo "$u"
    else
      echo "REJECT:$u"
    fi
  done
}

resolve_units_introspect() {
  local werk="$1"
  ( [ -d "$werk/.git" ] || [ -f "$werk/.git" ] ) || { err "introspect: werk has no .git"; return 1; }
  git -C "$werk" diff origin/main --name-only 2>/dev/null | match_units_against_paths
}

resolve_units() {
  local explicit_list="$1" werk="$2"
  if [ -n "$explicit_list" ]; then
    resolve_units_explicit "$explicit_list"
    return $?
  fi
  resolve_units_introspect "$werk"
}

reverse_lines() {
  awk '{a[NR]=$0} END {for (i=NR; i>=1; i--) print a[i]}'
}

# =============================================================================
# Main flow — only runs when invoked directly, not when sourced for testing.
# =============================================================================

main() {
  # card_id is the top-level global so the EXIT trap can name it (#2931).
  card_id=""
  local probe="" units_arg=""

  while [ $# -gt 0 ]; do
    case "$1" in
      --probe) probe="${2:-}"; shift 2 ;;
      --units) units_arg="${2:-}"; shift 2 ;;
      --help|-h) usage; exit 0 ;;
      -*) err "unknown flag: $1"; usage >&2; exit 2 ;;
      *)
        if [ -z "$card_id" ]; then card_id="$1"; shift
        else err "unexpected arg: $1"; usage >&2; exit 2
        fi
        ;;
    esac
  done

  if [ -z "$card_id" ]; then err "missing card-id"; usage >&2; exit 2; fi
  if ! [[ "$card_id" =~ ^[0-9]+$ ]]; then err "card-id must be numeric: $card_id"; exit 2; fi

  local role="${DEPLOY_ROLE:-}"
  case "$role" in
    kade|wren|silas) ;;
    *) err "DEPLOY_ROLE must be one of: kade, wren, silas (got '$role')"; exit 3 ;;
  esac
  _deploy_role="$role"  # #2931 — give the EXIT trap the right role for the emit

  # #2931 — inherit ACP trace_id from the HEAD commit's Chorus-Trace-Id git
  # trailer (written by chorus_acp). Without this, deploy.* events mint a
  # fresh trace_id and can't be joined to the ACP step in
  # chorus_logs_for_trace. card_id is authoritative from the CLI arg;
  # trace_id comes from the commit. Env wins over trailer so a caller
  # passing CHORUS_TRACE_ID overrides.
  export CHORUS_CARD_ID="${CHORUS_CARD_ID:-$card_id}"
  if [ -z "${CHORUS_TRACE_ID:-}" ]; then
    _root="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
    _trailers=$(git -C "$_root" log -1 --format=%B 2>/dev/null \
                | git interpret-trailers --parse 2>/dev/null || true)
    _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}')
    [ -n "$_tid" ] && export CHORUS_TRACE_ID="$_tid"
  fi

  if [ -z "$probe" ]; then
    err "missing --probe — daemon-runtime cards require a smoke probe"
    exit 4
  fi

  local werk="$CHORUS_WERK_BASE_DEFAULT/$role-$card_id"
  if [ ! -d "$werk" ]; then
    err "werk not found at $werk — card not pulled, or already torn down post-/acp"
    exit 5
  fi

  local resolved_raw resolved unit_count
  resolved_raw="$(resolve_units "$units_arg" "$werk")"
  if echo "$resolved_raw" | grep -q '^REJECT:'; then
    local rejected
    rejected=$(echo "$resolved_raw" | grep '^REJECT:' | sed 's/^REJECT://' | tr '\n' ' ')
    err "refusing: --units contained unknown unit(s): $rejected(known: $KNOWN_UNITS)"
    exit 6
  fi
  resolved="$(echo "$resolved_raw" | grep -v '^$' | sort -u)"

  # AC5: zero-match refusal — use string-empty check, not count (grep -c on
  # empty stdin + `|| echo 0` produced "0\n0" which broke -eq integer test).
  if [ -z "$resolved" ]; then
    err "no deploy units matched — zero-match refusal (AC5)"
    err "  --units was empty; diff-introspect found no known unit paths"
    err "  patterns checked: chorus-api='^platform/api/', chorus-hooks='^platform/services/chorus-hooks/', cards-sdk='^directing/products/cards/'"
    err "  if the card legitimately has no deploy surface, use /acp directly (not this wrapper)"
    exit 7
  fi

  info "#$card_id ($role) — units: $(echo $resolved | tr '\n' ' ')"

  _current_step="chorus-werk-sync"
  info "step 1/4 — chorus-werk-sync"
  if ! chorus-werk-sync; then
    err "step 1 failed: chorus-werk-sync"
    exit 10
  fi

  local succeeded_units="" deploy_failed_unit="" u
  for u in $resolved; do
    _current_step="deploy-$u"
    info "step 2 — deploy unit: $u"
    if "deploy_$(echo $u | tr - _)" "$werk"; then
      succeeded_units="$succeeded_units $u"
    else
      deploy_failed_unit="$u"
      err "deploy_$u failed; rolling back succeeded units"
      break
    fi
  done

  if [ -n "$deploy_failed_unit" ]; then
    for u in $(echo $succeeded_units | tr ' ' '\n' | grep -v '^$' | reverse_lines); do
      info "rollback: $u"
      "rollback_$(echo $u | tr - _)" "$werk" || err "rollback of $u also failed — manual intervention may be needed"
    done
    exit 11
  fi

  _current_step="probe"
  info "step 3/4 — probe"
  if ! bash -c "$probe"; then
    err "probe rejected the deploy — rolling back all succeeded units"
    for u in $(echo $succeeded_units | tr ' ' '\n' | grep -v '^$' | reverse_lines); do
      info "rollback: $u"
      "rollback_$(echo $u | tr - _)" "$werk" || err "rollback of $u also failed — manual intervention may be needed"
    done
    exit 12
  fi

  _current_step="cards-done"
  info "step 4/4 — cards done $card_id"
  if ! cards done "$card_id"; then
    err "cards done $card_id failed (deploy succeeded, card status didn't update)"
    exit 13
  fi

  info "#$card_id deployed and accepted (units: $(echo $succeeded_units | tr '\n' ' '))"
}

# Source-guard: don't run main when sourced (e.g., by bats for unit testing).
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
