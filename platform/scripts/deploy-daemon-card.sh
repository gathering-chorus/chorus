#!/bin/bash
# deploy-daemon-card.sh — sanctioned deploy for daemon-runtime cards (#2925).
#
# Daemon-runtime cards (diff touches platform/api/src/**, mcp/server.ts,
# platform/services/chorus-hooks/**) cannot ship through chorus_acp/chorus_commit
# MCP — those tools ARE the daemon being changed. This wrapper is the out-of-band
# bootstrap path that #2923 / #2916 executed manually last night.
#
# Sequence: chorus-werk-sync → chorus-deploy chorus-api → probe → cards done
# Refuses non-silas roles per DEC-022 ("builds are Silas").
# AC4 will add: chorus-deploy --rollback invoked on probe failure.

set -u

usage() {
  cat <<'EOF'
Usage: deploy-daemon-card.sh <card-id> --probe "<smoke-command>"

Sanctioned bootstrap deploy for daemon-runtime cards. Sequences:
  1. chorus-werk-sync           (pull canonical to current main)
  2. chorus-deploy chorus-api   (rebuild + restart)
  3. probe                      (verify new behavior live in restarted daemon)
  4. cards done <card-id>       (only on probe success)

Requires DEPLOY_ROLE=silas. --probe is mandatory.
EOF
}

err() { echo "deploy-daemon-card: $*" >&2; }

card_id=""
probe=""

while [ $# -gt 0 ]; do
  case "$1" in
    --probe) probe="${2:-}"; shift 2 ;;
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

# #2931 — inherit ACP trace_id from the HEAD commit's `Chorus-Trace-Id:` git
# trailer (written by chorus_acp). Without this, deploy.* events mint a fresh
# trace_id and can't be joined to the ACP step in chorus_logs_for_trace.
# card_id is authoritative from the CLI arg; trace_id comes from the commit.
# Env wins over trailer so a caller passing CHORUS_TRACE_ID overrides.
export CHORUS_CARD_ID="${CHORUS_CARD_ID:-$card_id}"
if [ -z "${CHORUS_TRACE_ID:-}" ]; then
  _root="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
  _trailers=$(git -C "$_root" log -1 --format=%B 2>/dev/null \
              | git interpret-trailers --parse 2>/dev/null || true)
  _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}')
  [ -n "$_tid" ] && export CHORUS_TRACE_ID="$_tid"
fi

# #2931 — failure-emit trap. AC5 of #2931: simulated deploy failure produces
# result=fail event with error= field visible in chorus_logs_for_card.
# Script uses `set -u` only (not -e), so most failures are explicit `err+exit`
# calls. Wrap those + EXIT trap so any non-zero exit emits deploy.failed.
_deploy_role="${DEPLOY_ROLE:-silas}"
_emit_deploy_failed() {
  local exit_code="$?"
  # Only emit on non-zero exits AND only once (guard via marker var).
  if [ "$exit_code" -ne 0 ] && [ -z "${_deploy_failed_emitted:-}" ]; then
    _deploy_failed_emitted=1
    local err_msg="card=${card_id} exit=${exit_code} step=${_current_step:-unknown}"
    if command -v chorus-log >/dev/null 2>&1; then
      chorus-log deploy.failed "$_deploy_role" \
        "result=fail" "error=$err_msg" "exit_code=$exit_code" >/dev/null 2>&1 || true
    fi
  fi
}
trap _emit_deploy_failed EXIT

# Role guard — "builds are Silas" (DEC-022, Jeff 2026-05-15)
role="${DEPLOY_ROLE:-}"
if [ "$role" != "silas" ]; then
  err "deploy-daemon-card.sh restricted to silas (DEPLOY_ROLE='$role')"
  err "per DEC-022, builds are Silas. Reroute via Silas or correct DEPLOY_ROLE."
  exit 3
fi

if [ -z "$probe" ]; then
  err "missing --probe — daemon-runtime cards require a smoke probe"
  err "(AC3 enforces probe presence at card-add; wrapper requires it at deploy)"
  exit 4
fi

echo "deploy-daemon-card: #$card_id (silas) — starting"

_current_step="chorus-werk-sync"
echo "deploy-daemon-card: step 1/4 — $_current_step"
if ! chorus-werk-sync; then
  err "step 1 failed: chorus-werk-sync — canonical not synced; aborting before deploy"
  exit 10
fi

_current_step="chorus-deploy"
echo "deploy-daemon-card: step 2/4 — chorus-deploy chorus-api"
if ! chorus-deploy chorus-api; then
  err "step 2 failed: chorus-deploy chorus-api — daemon not rebuilt; aborting before probe"
  exit 11
fi

_current_step="probe"
echo "deploy-daemon-card: step 3/4 — probe"
if ! bash -c "$probe"; then
  err "step 3 failed: probe rejected the deploy — invoking rollback"
  if ! chorus-deploy chorus-api --rollback; then
    err "rollback ALSO failed — manual intervention required"
  fi
  exit 12
fi

_current_step="cards-done"
echo "deploy-daemon-card: step 4/4 — cards done $card_id"
if ! cards done "$card_id"; then
  err "step 4 failed: cards done $card_id — card not marked done (deploy succeeded, card status didn't update)"
  exit 13
fi

echo "deploy-daemon-card: #$card_id deployed and accepted"
