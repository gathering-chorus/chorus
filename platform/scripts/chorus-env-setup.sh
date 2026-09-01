# chorus-env-setup.sh — canonical CHORUS_ROOT + CHORUS_ROLE source.
# #2571 — retire shell-rc-discipline. Source this script as line 1 of any
# chorus shell script that needs CHORUS_ROOT or CHORUS_ROLE; it self-locates
# from BASH_SOURCE so cwd, pre-set env, and shell rc state don't matter.
#
# Source pattern (bash/zsh):
#   source "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh"
# or from outside platform/scripts/:
#   source /path/to/chorus/platform/scripts/chorus-env-setup.sh
#
# LaunchAgent pattern (plist):
#   <key>ProgramArguments</key>
#   <array>
#     <string>/bin/bash</string>
#     <string>-c</string>
#     <string>source /path/to/chorus/platform/scripts/chorus-env-setup.sh && exec your-command</string>
#   </array>
#
# Authoritative: ignores stale CHORUS_ROOT in env; re-derives from script
# location (matches #2505 fail-loud philosophy — don't trust caller's env).
# Consolidates platform/shell/chorus-role-env.sh CHORUS_ROLE derivation.

# --- self-locate -------------------------------------------------------------
# BASH_SOURCE[0] is this script's path when sourced from bash. zsh uses %x.
__chorus_env_self="${BASH_SOURCE[0]:-${(%):-%x}}"
__chorus_env_dir="$(cd "$(dirname "$__chorus_env_self")" && pwd -P)"
# This script lives at $CHORUS_ROOT/platform/scripts/chorus-env-setup.sh.
__chorus_env_root="$(cd "$__chorus_env_dir/../.." && pwd -P)"

# --- export CHORUS_ROOT (authoritative, ignores prior env) -------------------
export CHORUS_ROOT="$__chorus_env_root"

# --- derive CHORUS_ROLE from cwd --------------------------------------------
# Two shapes, because a role works in two places (#3959):
#   roles/<role>/...                     — session anchor, role state
#   chorus-werk/<role>-<card>/...        — the WERK, where building happens
#
# Only the first was matched until 2026-08-21. Since every card is built in a
# werk, the variable was absent for exactly the work Jeff was trying to see:
# the shim had no DEPLOY_ROLE to inject (shim.rs:386, env does not cross the
# socket), so HookInput::role() fell to Role::Unknown (types.rs:37) and ~26,000
# events a day were written with no owner. The werk slot's owner is the segment
# before the first "-" — the same rule from_werk_slot() applies (types.rs:48).
case "$PWD" in
  */roles/wren*)          export CHORUS_ROLE=wren  DEPLOY_ROLE=wren  ;;
  */roles/silas*)         export CHORUS_ROLE=silas DEPLOY_ROLE=silas ;;
  */roles/kade*)          export CHORUS_ROLE=kade  DEPLOY_ROLE=kade  ;;
  */chorus-werk/wren-*)   export CHORUS_ROLE=wren  DEPLOY_ROLE=wren  ;;
  */chorus-werk/silas-*)  export CHORUS_ROLE=silas DEPLOY_ROLE=silas ;;
  */chorus-werk/kade-*)   export CHORUS_ROLE=kade  DEPLOY_ROLE=kade  ;;
esac
export DEPLOY_ROLE

# --- werk + bin (#2735) -----------------------------------------------------
# CHORUS_HOME is the canonical chorus checkout (the read-only-during-sessions
# tree). When sourced from canonical, CHORUS_HOME == CHORUS_ROOT. When
# sourced from a werk (e.g., /chorus-werk/kade), CHORUS_HOME points at the
# sibling /chorus directory — the role's session-start anchor and read
# surface for role state.
__chorus_env_parent="$(cd "$CHORUS_ROOT/.." && pwd -P)"
case "$CHORUS_ROOT" in
  *chorus-werk/*)
    # Werk: canonical lives at <parent of chorus-werk>/chorus
    __chorus_env_werk_parent="$(cd "$__chorus_env_parent/.." && pwd -P)"
    export CHORUS_HOME="$__chorus_env_werk_parent/chorus"
    unset __chorus_env_werk_parent ;;
  *)
    # Canonical or unknown: CHORUS_HOME == CHORUS_ROOT
    export CHORUS_HOME="$CHORUS_ROOT" ;;
esac

# CHORUS_WERK_BASE is where per-role git worktrees live. Default sibling to
# CHORUS_HOME so chorus-werk and chorus stay symmetric on disk.
# Caller-provided value is preserved (tests / CI may override).
if [ -z "${CHORUS_WERK_BASE:-}" ]; then
  __chorus_env_home_parent="$(cd "$CHORUS_HOME/.." && pwd -P)"
  export CHORUS_WERK_BASE="$__chorus_env_home_parent/chorus-werk"
  unset __chorus_env_home_parent
fi
unset __chorus_env_parent

# Per-role werk var: <ROLE>_WERK points at the role's ephemeral werk —
# but only when exactly one exists. The ephemeral model (#2913/#2917) has
# no single persistent werk per role; a role has 0, 1, or N
# chorus-werk/<role>-<card>/ worktrees. Resolve like resolveWorkingTree:
# exactly one match → set it; zero or many → leave unset, so callers see
# absence rather than a guessed or deleted path. `find` (not a shell glob
# or array) keeps this identical under bash and zsh — this script is
# sourced from both.
if [ -n "${CHORUS_ROLE:-}" ]; then
  # `find` exits 1 if CHORUS_WERK_BASE doesn't exist yet (fresh role, no werks
  # ever created); `|| true` keeps that from aborting callers under `set -e`.
    # #3606 — EXCLUDE the -bin directory from the werk glob. env-setup itself
    # provisions "$CHORUS_WERK_BASE/<role>-bin" (WERK_<ROLE>_BIN, below), so
    # -name "<role>-*" matched BOTH the real werk AND that bin dir. The count then
    # came to 2, the `= "1"` guard silently declined, and <ROLE>_WERK was never
    # exported — leaving #3016's per-role CHORUS_MCP_PORT resolution permanently
    # inert on any machine where the bin dir exists, which is all of them.
    # Silent because count!=1 is ALSO the legitimate no-werk and two-werks case
    # (both asserted below), so nothing distinguished "no werk" from "the glob
    # matched its own scaffolding".
  __chorus_env_werk_dir="$(find "$CHORUS_WERK_BASE" -maxdepth 1 -type d -name "$CHORUS_ROLE-*" ! -name "$CHORUS_ROLE-bin" 2>/dev/null || true)"
  # `grep -c .` exits 1 on empty input (zero werks); `|| true` keeps the
  # count at "0" instead of aborting callers that run `set -e` (#3012).
  __chorus_env_werk_count="$(printf '%s' "$__chorus_env_werk_dir" | grep -c . || true)"
  if [ "$__chorus_env_werk_count" = "1" ]; then
    case "$CHORUS_ROLE" in
      kade)  export KADE_WERK="$__chorus_env_werk_dir"   ;;
      wren)  export WREN_WERK="$__chorus_env_werk_dir"   ;;
      silas) export SILAS_WERK="$__chorus_env_werk_dir"  ;;
    esac
  fi
  unset __chorus_env_werk_dir __chorus_env_werk_count
fi

# CHORUS_BIN: single deployed location for chorus-* binaries (#2734 target).
# Prepend to PATH so signed installs override target/release builds.
# Idempotent: re-sourcing does not duplicate the entry.
export CHORUS_BIN="$HOME/.chorus/bin"
case ":$PATH:" in
  *":$CHORUS_BIN:"*) ;;  # already present, no-op
  *) export PATH="$CHORUS_BIN:$PATH" ;;
esac

# #2995 / #3020 — WERK_<ROLE>_BIN: the role's binary slot. It is a PEER of the
# card werks (chorus-werk/<role>-bin), NOT a child of any one werk. It used to
# live at <werk>/.werk-bin, which broke two ways: with more than one card open
# the werk lookup grabbed the wrong (often stale) one, and the slot was deleted
# when that card's werk was torn down at /acp. As a per-role peer it is stable
# across cards, never resolves to a stale werk, and survives acp. The promote
# step copies the slot into CHORUS_BIN.
#
# #3197 — these slots are GLOBAL, not role-scoped. Each is just
# $CHORUS_WERK_BASE/<role>-bin (CHORUS_WERK_BASE is itself global), so the
# derivation has ONE home — here — and every consumer (chorus-bin-install)
# READS the exported var instead of re-deriving the formula. Exported
# unconditionally so a role-LESS daemon (chorus-mcp / chorus-api) that sources
# this file at boot can resolve ANY role's slot at request time. Before, these
# were gated on CHORUS_ROLE, so a daemon with no boot role exported none — the
# exit-7 "requires WERK_<ROLE>_BIN" that broke every werk-targeted deploy.
export WERK_KADE_BIN="$CHORUS_WERK_BASE/kade-bin"
export WERK_WREN_BIN="$CHORUS_WERK_BASE/wren-bin"
export WERK_SILAS_BIN="$CHORUS_WERK_BASE/silas-bin"
mkdir -p "$WERK_KADE_BIN" "$WERK_WREN_BIN" "$WERK_SILAS_BIN" 2>/dev/null || true

# PATH-prefix only the CURRENT role's slot — that part IS session-scoped: a role
# resolves its own in-flight build before canonical. A role-less daemon skips
# this (it has the vars above for lookup, but prepends no one's slot to PATH).
if [ -n "${CHORUS_ROLE:-}" ]; then
  case "$CHORUS_ROLE" in
    kade)  __chorus_env_role_bin="$WERK_KADE_BIN"  ;;
    wren)  __chorus_env_role_bin="$WERK_WREN_BIN"  ;;
    silas) __chorus_env_role_bin="$WERK_SILAS_BIN" ;;
    *)     __chorus_env_role_bin="" ;;
  esac
  if [ -n "$__chorus_env_role_bin" ]; then
    case ":$PATH:" in
      *":$__chorus_env_role_bin:"*) ;;  # already present, no-op
      *) export PATH="$__chorus_env_role_bin:$PATH" ;;
    esac
  fi
  unset __chorus_env_role_bin
fi

# #3016 — CHORUS_MCP_PORT: per-session chorus-mcp endpoint for daemon
# try-before-buy. The daemon equivalent of WERK_<ROLE>_BIN above: a binary is
# isolated per session via PATH-prefix, but a daemon is one shared process on a
# fixed port, so isolation is per-session ENDPOINT instead. Canonical chorus-mcp
# listens on CHORUS_MCP_PORT_CANONICAL (:3341). When the role has an active werk
# whose werk-mcp daemon is deployed (marker file present, written by
# `werk-deploy <card> <role> --target werk`), the session resolves its own daemon
# on a deterministic per-role port; .mcp.json interpolates CHORUS_MCP_PORT into
# its url. No active werk daemon → canonical :3341 (no-regression default, AC6).
export CHORUS_MCP_PORT_CANONICAL="${CHORUS_MCP_PORT_CANONICAL:-3341}"
export CHORUS_MCP_PORT="$CHORUS_MCP_PORT_CANONICAL"
if [ -n "${CHORUS_ROLE:-}" ]; then
  # Deterministic per-role werk port — distinct, no collision with chorus-api
  # :3340 or canonical chorus-mcp :3341.
  # LIMITATION (#3016, flagged by Wren): the port is per-ROLE, not per-werk. A
  # role running two werks (two cards in flight) would have both resolve to the
  # same port — collision. Acceptable today (one-card-per-role is the norm);
  # port-per-werk is the follow-on if concurrent-cards-per-role becomes real.
  case "$CHORUS_ROLE" in
    silas) __chorus_mcp_werk_port=3351 ;;
    kade)  __chorus_mcp_werk_port=3352 ;;
    wren)  __chorus_mcp_werk_port=3353 ;;
    *)     __chorus_mcp_werk_port="" ;;
  esac
  if [ -n "$__chorus_mcp_werk_port" ]; then
    export CHORUS_MCP_WERK_PORT="$__chorus_mcp_werk_port"
    case "$CHORUS_ROLE" in
      kade)  __chorus_mcp_role_werk="${KADE_WERK:-}"  ;;
      wren)  __chorus_mcp_role_werk="${WREN_WERK:-}"  ;;
      silas) __chorus_mcp_role_werk="${SILAS_WERK:-}" ;;
      *)     __chorus_mcp_role_werk=""                ;;
    esac
    if [ -n "$__chorus_mcp_role_werk" ] && [ -f "$__chorus_mcp_role_werk/.werk-mcp/active" ]; then
      export CHORUS_MCP_PORT="$__chorus_mcp_werk_port"
    fi
    unset __chorus_mcp_role_werk
  fi
  unset __chorus_mcp_werk_port
fi

# --- Ollama embed hosts (#3217) ---------------------------------------------
# Tracked source of truth so a plist regen can't silently revert them (the
# 2026-06-04 outage: OLLAMA_URL lived only in the out-of-VC plist, pointed at a
# dead Bedroom host, and search burned 48s/query). SEARCH query-embed = localhost
# (latency-critical, same box). BULK embed-delta = Bedroom (M2-Pro/32GB GPU, DEC-054).
export OLLAMA_URL="${OLLAMA_URL:-http://127.0.0.1:11434}"
export OLLAMA_BULK_URL="${OLLAMA_BULK_URL:-http://192.168.86.242:11434}"

# --- #3690: verified role identity for the DAL ------------------------------
# CSS token verification config (the JWKS hairpin, see chorus-identity-token):
# the token's iss is the LOGICAL issuer (for the iss-check); JWKS is fetched
# LOCALLY because the logical origin is Cloudflare-blocked server-side. Same
# values athena-make's door runs with. Deployment config — override per box.
export CSS_ISSUER="${CSS_ISSUER:-https://id.lightlifeurbangardens.com/}"
export CHORUS_JWKS_URL="${CHORUS_JWKS_URL:-http://localhost:3001/.oidc/jwks}"
# A shell-invoked DAL call presents a fresh VERIFIED token instead of trusting
# the DEPLOY_ROLE string: the wrapper mints (cached ~8min) per call so a
# session's later calls never carry an expired token. Non-breaking — a failed
# mint (no cred / CSS down) leaves CHORUS_IDENTITY_TOKEN empty and the DAL falls
# back to DEPLOY_ROLE (#3356 additive path). #3687 removes that fallback.
# Only real role sessions (cred exists); bootstrap/generic shells skip.
if [ -n "${CHORUS_ROLE:-}" ] && [ -f "$HOME/.chorus/identity/${CHORUS_ROLE}/cred.json" ]; then
  # #3718 — renamed to athena-model (the model is Athena's, as werk-* is code's).
  # The wrapper follows the binary; `chorus-model` is a fail-loud stub that exits 2.
  athena-model() {
    local __tok
    # #3837 — the minter lives at platform/scripts/ and is NOT on PATH in role
    # shells (only CHORUS_BIN + the role bin are prepended), so `command
    # chorus-identity-token` was 127 every call, the wrapper passed an EMPTY
    # token, and every shell-session model write refused "identity-token-required"
    # (wren ×11 on 08-27, silas ×4 on 08-28). Resolve by path, PATH as fallback.
    local __minter="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}/platform/scripts/chorus-identity-token"
    [ -x "$__minter" ] || __minter="$(command -v chorus-identity-token 2>/dev/null || true)"
    __tok="$( [ -n "$__minter" ] && "$__minter" "${DEPLOY_ROLE:-$CHORUS_ROLE}" 2>/dev/null || true)"
    [ -n "$__tok" ] || echo "athena-model: WARN no identity token minted (minter=${__minter:-none}) — the write will refuse" >&2
    CHORUS_IDENTITY_TOKEN="$__tok" command athena-model "$@"
  }
fi

# --- fuseki backup lot (#4043) -----------------------------------------------
# The ONE home for where fuseki backups live. Before this, the destination was
# set only in com.gathering.fuseki-backup's plist while restore-drill.sh fell
# back to the abandoned lot (/Users/jeffbridwell/Backups/...) — so the drill
# spent 24 minutes restoring a 16-day-old leftover and graded the wrong thing
# (2026-08-31). Writer and drill both read these; per-box override via env.
export FUSEKI_BACKUP_REMOTE="${FUSEKI_BACKUP_REMOTE:-Jeffs-Mac-mini.local}"
export FUSEKI_BACKUP_DEST="${FUSEKI_BACKUP_DEST:-/Volumes/VideosNew/backups/library/fuseki}"

# --- cleanup tmp vars --------------------------------------------------------
unset __chorus_env_self __chorus_env_dir __chorus_env_root
