#!/usr/bin/env python3
"""crawler-error-check.py — count crawler.domain.failed events since CUTOFF.

Sibling-script pattern (#2861): keeps the alert YAML's check block bash-only
so awk extraction can't truncate python source on a column-0 lowercase line.
"""
import datetime
import json
import os
import re
import sys


def normalize_iso(ts: str) -> str:
    ts = re.sub(r"([+-])(\d{2})(\d{2})$", r"\1\2:\3", ts)
    return ts.replace("Z", "+00:00")


def main() -> int:
    cutoff_raw = os.environ.get("CUTOFF", "")
    if not cutoff_raw:
        print("0")
        return 0
    cutoff = datetime.datetime.fromisoformat(normalize_iso(cutoff_raw))
    hits = 0
    for line in sys.stdin:
        try:
            d = json.loads(line)
            parsed = datetime.datetime.fromisoformat(normalize_iso(d.get("timestamp", "")))
            if parsed >= cutoff:
                hits += 1
        except Exception:
            pass
    print(hits)
    return 0


if __name__ == "__main__":
    sys.exit(main())
