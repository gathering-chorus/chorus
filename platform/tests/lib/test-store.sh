# #4332 — the store a bats suite writes to. Never /pods.
#
# Reading all 267 bats suites on 2026-09-26 found suites that wrote to the
# production dataset (/pods) on every nightly: throwaway graphs, probe rows
# left behind, an INSERT with no gate. A test that writes prod makes the next
# red a lie, and it is the class that emptied the ontology on 2026-08-28.
#
# test_store gives a suite its own dataset: an in-memory Fuseki dataset,
# created on first use and shared by the run. Suites point their curl calls
# and athena-deploy at it through FUSEKI_GSP / FUSEKI_QUERY / FUSEKI_UPDATE.
# Reads of the real model stay on /pods (read-only).
#
#   . "$ROOT/platform/tests/lib/test-store.sh"
#   setup() { test_store || skip "UNMEASURED: $TEST_STORE_WHY"; ... }
#
# Returns non-zero (and sets TEST_STORE_WHY) when Fuseki is down or the
# dataset cannot be created, so the suite reports UNMEASURED, never green
# against the wrong store.

test_store() {
  local base="${FUSEKI_BASE_URL:-http://localhost:3030}"
  local name="${CHORUS_TEST_STORE:-chorus-test}"
  if [ "$name" = "pods" ]; then
    TEST_STORE_WHY="CHORUS_TEST_STORE=pods is the production dataset"
    return 3
  fi
  if [ -z "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
    # shellcheck disable=SC1091
    . "${CHORUS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}/platform/scripts/fuseki-auth.sh" >/dev/null 2>&1 || true
  fi
  local auth=()
  [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ] && auth=(-u "${FUSEKI_ADMIN_USER:-admin}:${FUSEKI_ADMIN_PASSWORD}")
  if ! curl -sf --max-time 5 -o /dev/null "$base/\$/ping"; then
    TEST_STORE_WHY="Fuseki unreachable at $base"
    return 2
  fi
  if ! curl -sf --max-time 5 -o /dev/null "${auth[@]+"${auth[@]}"}" "$base/\$/datasets/$name"; then
    if ! curl -sf --max-time 10 -o /dev/null "${auth[@]+"${auth[@]}"}" \
        -X POST "$base/\$/datasets" --data "dbName=$name&dbType=mem"; then
      TEST_STORE_WHY="could not create the test dataset /$name"
      return 3
    fi
  fi
  export TEST_STORE="$base/$name"
  export FUSEKI_GSP="$TEST_STORE/data"
  export FUSEKI_QUERY="$TEST_STORE/query"
  export FUSEKI_UPDATE="$TEST_STORE/update"
  return 0
}

# A write URL a suite is about to use must not be the production dataset.
# Fails loudly, so a suite that drifts back to /pods goes red, not silent.
assert_not_prod() {
  case "$1" in
    */pods/*|*/pods) echo "refusing: $1 is the production dataset (#4332)" >&2; return 1 ;;
  esac
  return 0
}

# True when the variant at $1 answers from the production store. Today a werk
# variant's athena-make has no CHORUS_FUSEKI of its own, so it reads and writes
# /pods (measured 2026-09-26: prove-live's 4185 run posted 3 rows into prod and
# only the mass-delete guard stopped it deleting 6,371). The tell: it serves
# crawled CodeFile rows, which only the production store holds — the crawler
# runs against prod. A suite that writes must report UNMEASURED here, never
# write. (#4334)
variant_shares_prod() {
  local n
  n=$(curl -s --max-time 20 "$1/code/files?limit=1" \
    | python3 -c 'import sys,json; print(len(json.load(sys.stdin).get("data",[])))' 2>/dev/null)
  [ "${n:-0}" -gt 0 ]
}

# #4416 — a private door for a suite that writes through athena-make: its own
# in-memory dataset holding a copy of the model it needs (the shapes, the
# security and identity graphs, the Domain rows, and the reference rows a
# crawled row points at), and its own athena-make serving it on a free port.
# The crawler suites (4180, 4185) skipped every night for want of one: the
# only door was prod's (:3360), which they rightly refuse.
#
#   setup_file()    { private_door || skip "UNMEASURED: $TEST_STORE_WHY"; }
#   PRIVATE_DOOR_PREP=<SPARQL update> shapes the copy before the door starts
#   teardown_file() { private_door_stop; }
#
# Exports OWL_URL (the door) and PRIVATE_DOOR_PID. Reads of /pods are
# read-only copies; nothing here writes /pods.
private_door() {
  local base="${FUSEKI_BASE_URL:-http://localhost:3030}"
  local root="${CHORUS_ROOT_REAL:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
  if [ -z "${FUSEKI_ADMIN_PASSWORD:-}" ]; then
    # shellcheck disable=SC1091
    . "$root/platform/scripts/fuseki-auth.sh" >/dev/null 2>&1 || true
  fi
  local auth=()
  [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ] && auth=(-u "${FUSEKI_ADMIN_USER:-admin}:${FUSEKI_ADMIN_PASSWORD}")
  local bin="${ATHENA_MAKE_BIN:-$root/platform/services/athena-make/target/release/athena-make}"
  [ -x "$bin" ] || bin="$(command -v athena-make 2>/dev/null)"
  if [ -z "$bin" ] || [ ! -x "$bin" ]; then TEST_STORE_WHY="athena-make not built"; return 2; fi
  PRIVATE_DOOR_DS="chorus-door-$$-${RANDOM}"
  if ! curl -sf --max-time 10 -o /dev/null "${auth[@]+"${auth[@]}"}" -X POST "$base/\$/datasets" \
      --data "dbName=$PRIVATE_DOOR_DS&dbType=mem"; then
    TEST_STORE_WHY="could not create the private dataset (Fuseki at $base)"; return 2
  fi
  export PRIVATE_DOOR_DS
  local tmp; tmp="$(mktemp -d)"
  local g
  for g in urn:chorus:ontology urn:chorus:domains:security urn:chorus:domains:identity \
           urn:chorus:domains:domains urn:chorus:domains:infrastructure; do
    curl -sf --max-time 30 "${auth[@]+"${auth[@]}"}" -H 'Accept: application/n-triples' \
      "$base/pods/data?graph=$g" -o "$tmp/g.nt" || { TEST_STORE_WHY="could not read $g"; return 2; }
    curl -sf --max-time 30 -o /dev/null "${auth[@]+"${auth[@]}"}" -X PUT -H 'Content-Type: application/n-triples' \
      --data-binary "@$tmp/g.nt" "$base/$PRIVATE_DOOR_DS/data?graph=$g" || { TEST_STORE_WHY="could not copy $g"; return 2; }
  done
  # the reference rows a crawled code row points at (kinds, languages)
  local q='CONSTRUCT { ?s ?p ?o } WHERE { GRAPH <urn:chorus:domains:code> { ?s a ?c ; ?p ?o FILTER(?c IN (<https://jeffbridwell.com/chorus#CodeKind>, <https://jeffbridwell.com/chorus#Language>)) } }'
  curl -sf --max-time 30 "${auth[@]+"${auth[@]}"}" -H 'Accept: application/n-triples' \
    --data-urlencode "query=$q" "$base/pods/query" -o "$tmp/ref.nt" \
    && curl -sf --max-time 30 -o /dev/null "${auth[@]+"${auth[@]}"}" -X PUT -H 'Content-Type: application/n-triples' \
      --data-binary "@$tmp/ref.nt" "$base/$PRIVATE_DOOR_DS/data?graph=urn:chorus:domains:code" \
    || { TEST_STORE_WHY="could not copy the code reference rows"; return 2; }
  rm -rf "$tmp"
  # a suite may shape its private copy before the door reads it (the door
  # caches each principal's permissions for 5 minutes once it has)
  if [ -n "${PRIVATE_DOOR_PREP:-}" ]; then
    curl -sf --max-time 30 -o /dev/null "${auth[@]+"${auth[@]}"}" --data-urlencode "update=$PRIVATE_DOOR_PREP" \
      "$base/$PRIVATE_DOOR_DS/update" || { TEST_STORE_WHY="PRIVATE_DOOR_PREP update failed"; return 2; }
  fi
  local port; port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
  # #4416 reopened — the door verifies every token against the issuer's keys.
  # The nightly's launchd env carries neither name, so every write came back
  # 401 at 03:00 and passed by hand. The door names both itself, with the same
  # defaults chorus-env-setup.sh uses.
  CHORUS_FUSEKI="$base/$PRIVATE_DOOR_DS" CHORUS_HOME="${BATS_FILE_TMPDIR:-/tmp}/door-home" \
    CSS_ISSUER="${CSS_ISSUER:-https://id.lightlifeurbangardens.com/}" \
    CHORUS_JWKS_URL="${CHORUS_JWKS_URL:-http://localhost:3001/.oidc/jwks}" \
    "$bin" serve --port "$port" >"${BATS_FILE_TMPDIR:-/tmp}/door.log" 2>&1 &
  PRIVATE_DOOR_PID=$!
  export PRIVATE_DOOR_PID OWL_URL="http://127.0.0.1:$port"
  local i
  for i in $(seq 1 60); do
    curl -sf --max-time 2 -o /dev/null "$OWL_URL/" && return 0
    sleep 0.5
  done
  TEST_STORE_WHY="private athena-make did not answer on :$port"
  return 2
}

private_door_stop() {
  [ -n "${PRIVATE_DOOR_PID:-}" ] && kill "$PRIVATE_DOOR_PID" 2>/dev/null
  if [ -n "${PRIVATE_DOOR_DS:-}" ]; then
    local auth=()
    [ -n "${FUSEKI_ADMIN_PASSWORD:-}" ] && auth=(-u "${FUSEKI_ADMIN_USER:-admin}:${FUSEKI_ADMIN_PASSWORD}")
    curl -s --max-time 10 -o /dev/null "${auth[@]+"${auth[@]}"}" -X DELETE \
      "${FUSEKI_BASE_URL:-http://localhost:3030}/\$/datasets/$PRIVATE_DOOR_DS" || true
  fi
  return 0
}
