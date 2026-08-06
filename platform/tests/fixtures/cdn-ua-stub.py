#!/usr/bin/env python3
"""A stand-in for the CDN in front of the identity provider (#3771).

Behaves the way Cloudflare actually behaved on 2026-08-06: 403 for the default
`Python-urllib` agent, 200 for a request that names itself. Measured, not guessed
— same URL, same minute, 403 with the default agent and 200 with any other.

This exists so the share-guard suite can exercise the OUTBOUND path. Before
#3771 the suite ran entirely offline, which meant it proved the guard degrades
gracefully when the provider is unreachable and could never notice that the
provider was unreachable for every real request.
"""
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

DISCOVERY = (b'{"issuer":"http://stub/","authorization_endpoint":"http://stub/auth",'
             b'"token_endpoint":"http://stub/token","registration_endpoint":"http://stub/reg",'
             b'"jwks_uri":"http://stub/jwks"}')


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.headers.get("User-Agent", "").startswith("Python-urllib"):
            self.send_response(403)
            self.send_header("Content-Length", "7")
            self.end_headers()
            self.wfile.write(b"blocked")
            return
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(DISCOVERY)))
        self.end_headers()
        self.wfile.write(DISCOVERY)

    def log_message(self, *a):
        pass


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
