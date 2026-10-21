#!/usr/bin/env bats
# @test-type: unit
# @domain: board
# 4682-chunk-sequence.bats — prioritize and sequence cards for a chunk via product-radius (#4682)

setup() {
  export DIR="$BATS_TEST_TMPDIR"
  export SCRIPT="${BATS_TEST_DIRNAME}/../scripts/clearing-chunk"

  # Start a lightweight local SPARQL stub server returning hasProduct mappings
  cat << 'PYSTUB' > "$DIR/sparql_stub.py"
import http.server, json, sys, urllib.parse

class SparqlHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length).decode("utf-8")
        parsed = urllib.parse.parse_qs(body)
        q = parsed.get("query", [""])[0]

        self.send_response(200)
        self.send_header("Content-Type", "application/sparql-results+json")
        self.end_headers()

        # If query asks for alerts-decisions, return borg and loom
        bindings = []
        if 'alerts-decisions' in q:
            bindings = [
                {"prod": {"value": "https://jeffbridwell.com/chorus#borg"}},
                {"prod": {"value": "https://jeffbridwell.com/chorus#loom"}},
            ]
        # Any other chunk returns empty bindings (refused)
        res = {"results": {"bindings": bindings}}
        self.wfile.write(json.dumps(res).encode())

    def log_message(self, format, *args):
        pass

server = http.server.HTTPServer(("127.0.0.1", 0), SparqlHandler)
port = server.server_address[1]
with open(sys.argv[1], "w") as f:
    f.write(str(port))
server.serve_forever()
PYSTUB

  python3 "$DIR/sparql_stub.py" "$DIR/port.txt" &
  export STUB_PID=$!

  for i in $(seq 1 30); do
    [ -s "$DIR/port.txt" ] && break
    sleep 0.1
  done
  export SPARQL_ENDPOINT="http://127.0.0.1:$(cat "$DIR/port.txt")/query"
}

teardown() {
  if [ -n "${STUB_PID:-}" ]; then
    kill "$STUB_PID" 2>/dev/null || true
  fi
}

@test "clearing-chunk prioritize lists open cards whose sequence matches chunk products in board order" {
  cat << 'STUB_CARDS' > "$DIR/fake-cards"
#!/usr/bin/env bash
if [ "$1" = "list" ]; then
  cat << 'BOARD'
WIP:
Next:
  101 Card One [Wren|P1]
  102 Card Two [Silas|P2]
  103 Card Three [Silas|P1]
BOARD
elif [ "$1" = "view" ]; then
  case "$2" in
    101) echo "Domains: sequence:borg" ;;
    102) echo "Domains: sequence:clearing" ;;
    103) echo "Domains: sequence:loom" ;;
  esac
fi
STUB_CARDS
  chmod +x "$DIR/fake-cards"
  export CARDS_CLI="$DIR/fake-cards"

  run "$SCRIPT" prioritize alerts-decisions
  [ "$status" -eq 0 ]
  [[ "$output" =~ "101" ]] || return 1
  [[ "$output" =~ "103" ]] || return 1
  ! [[ "$output" =~ "102" ]] || return 1

  pos101=$(echo "$output" | grep -n "101" | cut -d: -f1)
  pos103=$(echo "$output" | grep -n "103" | cut -d: -f1)
  [ "$pos101" -lt "$pos103" ]
}

@test "clearing-chunk sequence selects the first open card in chunk radius and excludes third product (#3734 negative proof)" {
  cat << 'STUB_CARDS' > "$DIR/fake-cards"
#!/usr/bin/env bash
if [ "$1" = "list" ]; then
  cat << 'BOARD'
Next:
  101 Card from Third Product [Wren|P1]
  102 First Matching Card [Silas|P1]
  103 Second Matching Card [Silas|P2]
BOARD
elif [ "$1" = "view" ]; then
  case "$2" in
    101) echo "Domains: sequence:clearing" ;;
    102) echo "Domains: sequence:borg" ;;
    103) echo "Domains: sequence:loom" ;;
  esac
fi
STUB_CARDS
  chmod +x "$DIR/fake-cards"
  export CARDS_CLI="$DIR/fake-cards"

  run "$SCRIPT" sequence alerts-decisions
  [ "$status" -eq 0 ]
  [ "$output" = "102" ]
  [ "$output" != "101" ]
}

@test "clearing-chunk sequence refuses when chunk has no products linked in graph (#3734 negative proof)" {
  cat << 'STUB_CARDS' > "$DIR/fake-cards"
#!/usr/bin/env bash
echo "Next: 101 Card [Wren|P1]"
STUB_CARDS
  chmod +x "$DIR/fake-cards"
  export CARDS_CLI="$DIR/fake-cards"

  # Chunk 'unlinked-chunk' returns empty bindings from SPARQL stub
  run "$SCRIPT" sequence unlinked-chunk
  [ "$status" -ne 0 ]
  [[ "$output" =~ "refused: chunk 'unlinked-chunk' has no products linked in graph (hasProduct)" ]] || return 1
}
