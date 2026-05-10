#!/usr/bin/env python3
"""crawler-error-check.py — print max consecutive failures across all domains.

Reads /tmp/crawler-domain-status.json (maintained by index-crawler-snapshots.sh
on every run) and prints the maximum value of `consecutive_failures` across
all domains. The alert YAML fires when this is >= 2 — a single chorus-api
kickstart burst registers as consecutive=1 for the affected domains, which
the next polling pass clears (consecutive resets to 0). Persistent breakage
on any domain grows consecutive without bound and fires reliably.

Pre-#2871 this script counted events in stdin matching a time window; that
shape captured a single burst as 5-10 minutes of repeated alert noise as
the events aged out. The status-file shape is the canonical truth: it
already encodes "is this domain currently failing repeatedly" (which is
what we actually want to alert on), instead of "did any domain fail
recently in any window" (which fires on every kickstart).

Sibling-script pattern (#2861): YAML check stays bash-only, no awk
truncation risk on column-0 python lines.
"""
import json
import os
import sys


STATUS_FILE_DEFAULT = "/tmp/crawler-domain-status.json"


def main() -> int:
    status_path = os.environ.get("CRAWLER_STATUS_FILE", STATUS_FILE_DEFAULT)
    if not os.path.exists(status_path):
        # No status file yet (crawler never ran). Treat as 0 — quiet.
        print("0")
        return 0
    try:
        with open(status_path) as f:
            status = json.load(f)
    except Exception:
        # Malformed file — quiet rather than fire spuriously. The crawler-stale
        # alert catches the "crawler not running" class via a separate signal.
        print("0")
        return 0
    max_consec = 0
    for domain, info in status.items():
        if not isinstance(info, dict):
            continue
        n = info.get("consecutive_failures", 0)
        try:
            n = int(n)
        except (TypeError, ValueError):
            continue
        if n > max_consec:
            max_consec = n
    print(max_consec)
    return 0


if __name__ == "__main__":
    sys.exit(main())
