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

echo "deploy-daemon-card: step 1/4 — chorus-werk-sync"
if ! chorus-werk-sync; then
  err "step 1 failed: chorus-werk-sync — canonical not synced; aborting before deploy"
  exit 10
fi

echo "deploy-daemon-card: step 2/4 — chorus-deploy chorus-api"
if ! chorus-deploy chorus-api; then
  err "step 2 failed: chorus-deploy chorus-api — daemon not rebuilt; aborting before probe"
  exit 11
fi

echo "deploy-daemon-card: step 3/4 — probe"
if ! bash -c "$probe"; then
  err "step 3 failed: probe rejected the deploy — invoking rollback"
  if ! chorus-deploy chorus-api --rollback; then
    err "rollback ALSO failed — manual intervention required"
  fi
  exit 12
fi

echo "deploy-daemon-card: step 4/4 — cards done $card_id"
if ! cards done "$card_id"; then
  err "step 4 failed: cards done $card_id — card not marked done (deploy succeeded, card status didn't update)"
  exit 13
fi

echo "deploy-daemon-card: #$card_id deployed and accepted"
