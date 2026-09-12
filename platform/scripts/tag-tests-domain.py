#!/usr/bin/env python3
"""tag-tests-domain.py — RETIRED by #4154 (2026-09-12). Fail loud, do nothing.

It was a second walker: it discovered test files, classified them, cleared
urn:chorus:domains:tests on every run and rewrote it by raw SPARQL (the wiping
hydrator Jeff named the defect on 2026-09-03; ADR-051 froze the catch-all,
ADR-040 says the DAL mints IRIs). Its pure parts live on as a library:

    platform/scripts/testfiles.py   is_test_file · declared · classify · case_names · discover

The one walker (crawler-hydrate-graph.sh) discovers test files and persists
Test rows through the generated API (/tests, batch), never SPARQL, never a clear.
"""
import sys
sys.stderr.write(__doc__)
sys.exit(2)
