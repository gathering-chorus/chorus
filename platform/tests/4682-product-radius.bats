#!/usr/bin/env bats
# @test-type: unit
# @domain: ontology
# 4682-product-radius.bats — test product-radius graph inventory and cleanup report (#4682)

setup() {
  export DIR="$BATS_TEST_TMPDIR"
  export SCRIPT="${BATS_TEST_DIRNAME}/../scripts/product-radius"
}

teardown() {
  if [ -n "${STUB_PID:-}" ]; then
    kill "$STUB_PID" 2>/dev/null || true
  fi
}

@test "product-radius runs against local stub SPARQL endpoint without touching prod" {
  cat << 'PYSTUB' > "$DIR/stub_server.py"
import http.server, json, sys

class SparqlHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        _ = self.rfile.read(length)
        self.send_response(200)
        self.send_header("Content-Type", "application/sparql-results+json")
        self.end_headers()
        res = {"results": {"bindings": []}}
        self.wfile.write(json.dumps(res).encode())

    def log_message(self, format, *args):
        pass

server = http.server.HTTPServer(("127.0.0.1", 0), SparqlHandler)
port = server.server_address[1]
with open(sys.argv[1], "w") as f:
    f.write(str(port))
server.serve_forever()
PYSTUB

  python3 "$DIR/stub_server.py" "$DIR/port.txt" &
  STUB_PID=$!

  for i in $(seq 1 30); do
    [ -s "$DIR/port.txt" ] && break
    sleep 0.1
  done
  PORT=$(cat "$DIR/port.txt")

  SPARQL_ENDPOINT="http://127.0.0.1:${PORT}/query" run "$SCRIPT"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "Products: 0" ]] || return 1
}

@test "product-radius generates valid HTML report with --html" {
  cat << 'FIXTURE' > "$DIR/fixture.json"
{
  "counts": {"products": 1, "domains": 1, "services": 0, "pipelines": 0, "valueStreams": 0},
  "products": [
    {
      "product": "test-product",
      "owner": "wren",
      "parts": [],
      "domains": [{"domain": "test-domain", "owner": "wren", "status": "operating", "step": "plan", "services": [], "flags": []}],
      "consumes": [],
      "pipelines": [],
      "flags": []
    }
  ],
  "unplacedDomains": [],
  "services": [],
  "pipelines": [],
  "issues": []
}
FIXTURE

  run "$SCRIPT" --from-fixture "$DIR/fixture.json" --html "$DIR/report.html"
  [ "$status" -eq 0 ]
  [ -f "$DIR/report.html" ]
  grep -q "<title>Graph Cleanup by Owner</title>" "$DIR/report.html"
  grep -q "test-product" "$DIR/report.html"
}

@test "product-radius filters output by --owner" {
  cat << 'FIXTURE' > "$DIR/multi.json"
{
  "counts": {"products": 2, "domains": 2, "services": 0, "pipelines": 0, "valueStreams": 0},
  "products": [
    {
      "product": "wren-product",
      "owner": "wren",
      "parts": [],
      "domains": [],
      "consumes": [],
      "pipelines": [],
      "flags": []
    },
    {
      "product": "kade-product",
      "owner": "kade",
      "parts": [],
      "domains": [],
      "consumes": [],
      "pipelines": [],
      "flags": []
    }
  ],
  "unplacedDomains": [],
  "services": [],
  "pipelines": [],
  "issues": []
}
FIXTURE

  run "$SCRIPT" --from-fixture "$DIR/multi.json" --owner wren --html "$DIR/wren.html"
  [ "$status" -eq 0 ]
  grep -q "<h2>wren" "$DIR/wren.html"
  ! grep -q "<h2>kade" "$DIR/wren.html" || return 1
}

@test "product-radius derives unmodeled yaml files and unlinked value streams" {
  mkdir -p "$DIR/pipelines"
  touch "$DIR/pipelines/rogue-pipeline.yaml"

  cat << 'FIXTURE' > "$DIR/stub_graph.json"
{
  "counts": {"products": 1, "domains": 0, "services": 0, "pipelines": 0, "valueStreams": 1},
  "products": [{"product": "test-prod", "owner": "wren", "parts": [], "domains": [], "consumes": [], "pipelines": [], "flags": []}],
  "unplacedDomains": [],
  "services": [],
  "pipelines": [],
  "issues": [
    {"kind": "yaml-no-rows", "subject": "rogue-pipeline.yaml", "text": "pipeline file rogue-pipeline.yaml exists on disk but has no matching Pipeline row in graph"},
    {"kind": "valuestream-no-product-link", "subject": "value-stream-unlinked", "text": "value stream has no explicit hasValueStream link to a Product in graph"}
  ]
}
FIXTURE

  run "$SCRIPT" --from-fixture "$DIR/stub_graph.json"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "yaml-no-rows: 1" ]] || return 1
  [[ "$output" =~ "valuestream-no-product-link: 1" ]] || return 1
}

@test "product-radius flags planted self-consume row (#3734 negative proof)" {
  cat << 'FIXTURE' > "$DIR/planted-fixture.json"
{
  "counts": {"products": 1, "domains": 1, "services": 0, "pipelines": 0, "valueStreams": 0},
  "products": [
    {
      "product": "test-product",
      "owner": "test-owner",
      "parts": [],
      "domains": [
        {
          "domain": "test-domain",
          "owner": "test-owner",
          "status": "operating",
          "step": "plan",
          "services": [],
          "flags": ["consumes-own-domain"]
        }
      ],
      "consumes": ["test-domain"],
      "pipelines": [],
      "flags": ["consumes-own-domain"]
    }
  ],
  "unplacedDomains": [],
  "services": [],
  "pipelines": [],
  "issues": [
    {
      "kind": "consumes-own-domain",
      "subject": "test-product",
      "text": "consumes test-domain, which it already owns"
    }
  ]
}
FIXTURE

  run "$SCRIPT" --from-fixture "$DIR/planted-fixture.json"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "consumes-own-domain: 1" ]] || return 1

  run "$SCRIPT" --from-fixture "$DIR/planted-fixture.json" --check
  [ "$status" -eq 2 ]

  run "$SCRIPT" --from-fixture "$DIR/planted-fixture.json" --html "$DIR/planted.html"
  [ "$status" -eq 0 ]
  grep -q "consumes a domain it owns" "$DIR/planted.html"
  grep -q "test-product" "$DIR/planted.html"
}
