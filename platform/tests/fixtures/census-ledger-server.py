#!/usr/bin/env python3
"""#4105 — a fixture ledger door for the registered-vs-executed census.

Serves the two collections `werk-test --reconcile` reads, in the generated
API's shape, so the walk can be exercised without the live store:

  /tests        the registered inventory (one page)
  /testresults  the executed ledger, paged by ?cursor=&limit= with links.next

Env:
  LEDGER_ROWS   total executed rows to serve (default 5)
  PAGE_SIZE     rows per page (default 2)
  REGISTERED    total registered rows (default = LEDGER_ROWS)
  FAIL_PAGE     1-based page number to answer 502 on (default 0 = never)
  PORT          port to bind (default 0 = ephemeral; the port is printed)
"""
import json, os, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse, parse_qs

ROWS = int(os.environ.get("LEDGER_ROWS", "5"))
PAGE = int(os.environ.get("PAGE_SIZE", "2"))
REG = int(os.environ.get("REGISTERED", str(ROWS)))
FAIL_PAGE = int(os.environ.get("FAIL_PAGE", "0"))


def test_row(i):
    return {
        "name": "test-%d" % i,
        "filePath": "platform/tests/f%d.bats" % i,
        "testName": "case %d" % i,
        "covers": "chorus",
        "pyramidLayer": "unit",
        "hermeticity": "hermetic",
        "testConcern": "behavior",
    }


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, body):
        raw = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        u = urlparse(self.path)
        q = parse_qs(u.query)
        path = u.path.rstrip("/")
        if path.endswith("/tests"):
            data = [test_row(i) for i in range(REG)]
            self._send(200, {"kind": "Test", "data": data, "links": {}})
            return
        if path.endswith("/testresults"):
            cursor = int(q.get("cursor", ["0"])[0])
            limit = int(q.get("limit", [str(PAGE)])[0])
            page_no = cursor // limit + 1 if limit else 1
            if FAIL_PAGE and page_no == FAIL_PAGE:
                self._send(502, {"kind": "Error", "data": {
                    "status": 502, "title": "Bad Gateway",
                    "detail": "fuseki-query failed: "}})
                return
            end = min(cursor + limit, ROWS)
            data = [test_row(i) for i in range(cursor, end)]
            links = {}
            if end < ROWS:
                links["next"] = "/v1/testresults?cursor=%d&limit=%d" % (end, limit)
            self._send(200, {"kind": "TestResult", "data": data, "links": links})
            return
        self._send(404, {"kind": "Error", "data": {"status": 404}})


if __name__ == "__main__":
    srv = HTTPServer(("127.0.0.1", int(os.environ.get("PORT", "0"))), H)
    print(srv.server_port, flush=True)
    srv.serve_forever()
