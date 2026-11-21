# @domain: tests
"""#4915 round 5 (Jeff 13:13; Wren 15:15) — the anchor for a live test's window.

A live test that asks "did X happen in the last hour?" reads red at 04:00 only because nobody was
active. Its window is anchored here to the newest event at the SOURCE (Loki's platform-chorus
stream, which the spine feeds), never to now and never to the surface under test, so a frozen
surface cannot agree with itself and stay green. No source event in the horizon (24h) is
UNMEASURED: exit 2, never green.

  python3 live_anchor.py newest '<logql>' [horizon_secs]
      → prints "<epoch seconds>\t<the newest line>" and exits 0; nothing in the horizon: exit 2
"""
import json
import os
import sys
import time
import urllib.parse
import urllib.request

LOKI = os.environ.get("LOKI_URL", "http://localhost:3102")


def newest(logql, horizon=86400):
    """(epoch seconds, line) of the newest line matching logql in the horizon, or None."""
    end = int(time.time() * 1e9)
    q = urllib.parse.urlencode({"query": logql, "limit": "1", "direction": "backward",
                                "start": str(end - int(horizon * 1e9)), "end": str(end)})
    with urllib.request.urlopen(f"{LOKI}/loki/api/v1/query_range?{q}", timeout=20) as r:
        res = json.load(r)["data"]["result"]
    vals = [v for s in res for v in s.get("values", [])]
    if not vals:
        return None
    ns, line = max(vals, key=lambda v: int(v[0]))
    return int(ns) / 1e9, line


def main(argv):
    if len(argv) >= 2 and argv[0] == "newest":
        try:
            got = newest(argv[1], float(argv[2]) if len(argv) > 2 else 86400)
        except Exception as e:  # the source did not answer: never a pass
            print(f"UNMEASURED: the source (Loki at {LOKI}) did not answer: {e}", file=sys.stderr)
            return 2
        if got is None:
            print("UNMEASURED: no source event in the horizon", file=sys.stderr)
            return 2
        print(f"{got[0]:.3f}\t{got[1]}")
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
