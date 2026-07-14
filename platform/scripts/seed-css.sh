#!/usr/bin/env bash
# seed-css.sh — provision per-agent CSS identities + client credentials (#3613 / ADR-052)
#
# Creates one pod per agent under Jeff's CSS account (Jeff = root authority; agents
# cannot self-mint), captures the REAL CSS-assigned WebID (source of truth — the model
# records it, never invents it), mints a client_credentials pair per agent WebID, and
# writes it to ~/.chorus/identity/<agent>/cred.json (0600, owner-only). NEVER echoes a
# secret or token value — secrets land only in the 0600 cred files.
#
# Idempotent: re-running logs in, skips pod creation if the pod exists, re-mints creds.
# Usage: ./seed-css.sh            (all agents)
#        AGENTS="silas wren" ./seed-css.sh   (subset)
set -euo pipefail

CSS="${CSS_URL:-http://localhost:3001}"
EMAIL="${CSS_EMAIL:-jeff@jeffbridwell.com}"
ENV_FILE="${GATHERING_APP_ENV:-$HOME/CascadeProjects/jeff-bridwell-personal-site/.env}"
IDENTITY_HOME="$HOME/.chorus/identity"
AGENTS="${AGENTS:-silas wren kade bridge chorus-sdk}"
CK="$(mktemp -t css-seed-XXXXXX)"
trap 'rm -f "$CK"' EXIT

# JSON object from key val key val ... — single-quoted body, no shell expansion, argv-safe.
jkv() { python3 -c 'import json,sys;a=sys.argv[1:];print(json.dumps({a[i]:a[i+1] for i in range(0,len(a),2)}))' "$@"; }

# account password: read by reference, never printed
PW="$(grep -m1 '^CSS_ACCOUNT_PASSWORD=' "$ENV_FILE" | cut -d= -f2- | tr -d '"')"
[ -n "$PW" ] || { echo "ERROR: CSS_ACCOUNT_PASSWORD not in $ENV_FILE" >&2; exit 1; }

echo "seed-css: logging in to $CSS as $EMAIL ..." >&2
curl -s -c "$CK" -X POST "$CSS/.account/login/password/" -H "Content-Type: application/json" \
  --data "$(jkv email "$EMAIL" password "$PW")" >/dev/null
ACCT="$(curl -s -b "$CK" "$CSS/.account/" | python3 -c 'import sys,json
a=json.load(sys.stdin).get("controls",{}).get("account",{})
print(a.get("pod","").split("/account/")[1].split("/")[0] if a.get("pod") else "")')"
[ -n "$ACCT" ] || { echo "ERROR: login failed (no account control)" >&2; exit 1; }
POD_EP="$CSS/.account/account/$ACCT/pod/"
CC_EP="$CSS/.account/account/$ACCT/client-credentials/"
WEBID_EP="$CSS/.account/account/$ACCT/webid/"

EXISTING_WEBIDS="$(curl -s -b "$CK" "$WEBID_EP" | python3 -c 'import sys,json
d=json.load(sys.stdin);w=d.get("webIdLinks") or d.get("webIds") or {}
print("\n".join(w.keys()) if isinstance(w,dict) else "")')"

mkdir -p "$IDENTITY_HOME"; chmod 700 "$IDENTITY_HOME"

for AGENT in $AGENTS; do
  echo "seed-css: [$AGENT] ..." >&2
  # 1. create pod (idempotent) — CSS pods live at /<name>/, WebID /<name>/profile/card#me
  POD_RESP="$(curl -s -b "$CK" -X POST "$POD_EP" -H "Content-Type: application/json" \
    --data "$(jkv name "$AGENT")" || true)"
  WEBID="$(printf '%s' "$POD_RESP" | python3 -c 'import sys,json
try: print(json.load(sys.stdin).get("webId",""))
except Exception: print("")')"
  if [ -z "$WEBID" ]; then
    WEBID="$(printf '%s\n' "$EXISTING_WEBIDS" | grep "/$AGENT/profile/card#me" | head -1 || true)"
    [ -n "$WEBID" ] || WEBID="$CSS/$AGENT/profile/card#me"
    echo "seed-css: [$AGENT] pod exists, webid=$WEBID" >&2
  else
    echo "seed-css: [$AGENT] pod created, webid=$WEBID" >&2
  fi

  # 2. mint client_credentials for THIS agent's WebID (distinct cred = isolation)
  CC_RESP="$(curl -s -b "$CK" -X POST "$CC_EP" -H "Content-Type: application/json" \
    --data "$(jkv name "chorus-agent-$AGENT" webId "$WEBID")")"

  # 3. write cred.json (0600) — the ONLY place the secret lands; secret stays in the
  #    python process (piped stdin → file), never in a shell var or on stdout.
  AGENT_DIR="$IDENTITY_HOME/$AGENT"; mkdir -p "$AGENT_DIR"; chmod 700 "$AGENT_DIR"
  CRED_FILE="$AGENT_DIR/cred.json"
  if ! printf '%s' "$CC_RESP" | CSS="$CSS" WEBID="$WEBID" AGENT="$AGENT" CRED_FILE="$CRED_FILE" python3 -c '
import sys,json,os
r=json.load(sys.stdin)
if not r.get("id") or not r.get("secret"):
    sys.stderr.write("  no id/secret in CC response: %s\n"%(json.dumps(r)[:160])); sys.exit(2)
out={"agent":os.environ["AGENT"],"webId":os.environ["WEBID"],
     "issuer":os.environ["CSS"]+"/","tokenEndpoint":os.environ["CSS"]+"/.oidc/token",
     "id":r["id"],"secret":r["secret"]}
open(os.environ["CRED_FILE"],"w").write(json.dumps(out,indent=2)+"\n")'; then
    echo "seed-css: [$AGENT] FAILED to mint/store cred (see above)" >&2; continue
  fi
  chmod 600 "$CRED_FILE"
  echo "seed-css: [$AGENT] cred stored 0600 at $CRED_FILE (secret not shown)" >&2
done

echo "seed-css: done. Provisioned WebIDs (source of truth = CSS):" >&2
for AGENT in $AGENTS; do
  wid="$(python3 -c "import json;print(json.load(open('$IDENTITY_HOME/$AGENT/cred.json')).get('webId',''))" 2>/dev/null || true)"
  echo "  $AGENT -> ${wid:-<FAILED>}" >&2
done
