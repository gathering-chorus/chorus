#!/usr/bin/env bats
# @test-type: integration — boots a scratch Fuseki (the live jar, an in-memory dataset, a free port, the committed shiro.ini); never touches :3030 or the store.
# @domain: security — no device on the LAN reads the store anonymously (#4582)
#
# #4582 — Fuseki binds *:3030 and shiro.ini left /query, /sparql and /get
# anonymous, so any device on the home LAN read the whole graph, security and
# identity included (192.168.86.36:3030 -> 200, measured 10-17). The committed
# config now gives reads the same loopback-origin filter writes have had since
# #3579. AC1: a read from this host's own LAN address (found at run time, never
# written here; 3370 ratchet) is refused on every read endpoint, and a loopback
# read still answers. AC2 (NEGATIVE PROOF): the same scratch store with reads
# set back to anon answers the LAN read, so the check can go red.

ROOT="$BATS_TEST_DIRNAME/../.."
SHIRO="$ROOT/platform/scripts/launchagents-canonical/fuseki-shiro.ini"
FUSEKI_HOME="${FUSEKI_HOME:-$HOME/.gathering/data/fuseki-5.1.0}"
JAVA="${JAVA:-/opt/homebrew/opt/openjdk/bin/java}"

setup() {
  [ -r "$FUSEKI_HOME/fuseki-server.jar" ] || skip "UNMEASURED: no Fuseki jar at $FUSEKI_HOME"
  # /usr/sbin is not on the pipeline's PATH (11-19 land: both cases skipped)
  LAN="$(/usr/sbin/ipconfig getifaddr en0 2>/dev/null || /usr/sbin/ipconfig getifaddr en1 2>/dev/null || true)"
  [ -n "$LAN" ] || skip "UNMEASURED: this host has no LAN address, so no non-loopback read can be made"
  case "$LAN" in 127.*|::1) skip "UNMEASURED: the LAN address $LAN is loopback";; esac
  T="$(mktemp -d)"
  PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("",0));print(s.getsockname()[1])')
}

teardown() {
  [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null || true
  rm -rf "$T"
}

# boot <shiro.ini>: a scratch Fuseki on every interface with that security file
boot() {
  mkdir -p "$T/base"
  sed -E 's/\$\{FUSEKI_[A-Z_]*PASSWORD\}/scratch-4582/g' "$1" > "$T/base/shiro.ini"
  (cd "$T/base" && FUSEKI_HOME="$FUSEKI_HOME" FUSEKI_BASE="$T/base" exec "$JAVA" -Xmx256m \
     -cp "$FUSEKI_HOME/fuseki-server.jar" org.apache.jena.fuseki.cmd.FusekiCmd \
     --mem --port="$PORT" /pods > "$T/server.log" 2>&1) &
  PID=$!
  up=""
  for _ in $(seq 1 60); do curl -sf -o /dev/null --max-time 1 "http://127.0.0.1:$PORT/\$/ping" && { up=1; break; }; sleep 0.5; done
  [ -n "$up" ] || { echo "the scratch Fuseki never answered /\$/ping"; cat "$T/server.log"; return 1; }
  grep -qE "Shiro file: file://(/private)?$T/base/shiro.ini" "$T/server.log" || { echo "the scratch server did not load our shiro.ini"; cat "$T/server.log"; return 1; }
}

code() { curl -s -o /dev/null -w '%{http_code}' --max-time 5 -G --data-urlencode 'query=ASK {}' "http://$1:$PORT/pods/$2"; }

@test "AC1: the committed config refuses every read from the LAN address, and a loopback read answers" {
  boot "$SHIRO"
  for ep in query sparql get; do
    c=$(code "$LAN" "$ep")
    [ "$c" = 403 ] || { echo "LAN read of /pods/$ep answered $c, want 403"; false; }
  done
  c=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "http://$LAN:$PORT/\$/status")
  [ "$c" = 403 ] || { echo "LAN /\$/status answered $c, want 403 (it lists the datasets)"; false; }
  for host in 127.0.0.1 localhost '[::1]'; do
    for ep in query sparql; do
      c=$(code "$host" "$ep")
      [ "$c" = 200 ] || { echo "loopback read of /pods/$ep via $host answered $c, want 200"; false; }
    done
    # GSP get of the default graph: answers (not 401/403) on loopback
    c=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "http://$host:$PORT/pods/get?default")
    [ "$c" = 200 ] || { echo "loopback GSP get via $host answered $c, want 200"; false; }
    c=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "http://$host:$PORT/\$/status")
    [ "$c" = 200 ] || { echo "loopback /\$/status via $host answered $c, want 200"; false; }
  done
}

@test "AC2 NEGATIVE PROOF: the same store with reads open answers the LAN read" {
  sed -E 's#^(/\*\*/(query|sparql|get)) = localhostFilter$#\1 = anon#' "$SHIRO" > "$T/open.ini"
  ! cmp -s "$SHIRO" "$T/open.ini" || { echo "the fixture did not reopen any read rule: the committed file changed shape"; false; }
  boot "$T/open.ini"
  c=$(code "$LAN" query)
  [ "$c" = 200 ] || { echo "with reads open the LAN read answered $c; the check cannot tell open from closed"; false; }
}
