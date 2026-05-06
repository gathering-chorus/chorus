#!/usr/bin/env python3
# clearing-flow-shape-validator.py — Validates /api/flow shape (#2333).
#
# Reads /api/flow JSON on stdin. Exits 0 on PASS, 1 on FAIL. Emits spine events
# via $CHORUS_LOG (binary path, default no-op). $ROLE used as the spine `role`.
#
# Hard checks: domains.chorus.cards non-empty, first card has sequences[] array.
# AC4 conditional: hard-asserts label validity when a multi-seq card exists,
# emits warn (`clearing.smoke.no_multi_seq`) without failing when none does.

import json, os, subprocess, sys

CHORUS_LOG = os.environ.get("CHORUS_LOG", "")
ROLE = os.environ.get("ROLE", "system")


def emit(event, *kvs):
    if not CHORUS_LOG:
        return
    try:
        subprocess.run([CHORUS_LOG, event, ROLE, *kvs], check=False)
    except Exception:
        pass


def fail(stage, detail):
    emit("clearing.smoke.failed", f"stage={stage}", detail)
    print(f"FAIL: {stage} {detail}", file=sys.stderr)
    sys.exit(1)


def validate(raw: str) -> int:
    try:
        flow = json.loads(raw)
    except json.JSONDecodeError as e:
        fail("flow_json", f"parse_error={e.msg}")

    cards = flow.get("domains", {}).get("chorus", {}).get("cards", [])
    if not cards:
        fail("flow_shape", "no_chorus_cards")

    first = cards[0]
    seqs = first.get("sequences")
    if not isinstance(seqs, list):
        fail("shape_first_card",
             f"card={first.get('id')} sequences_type={type(seqs).__name__}")

    multi = [c for c in cards
             if isinstance(c.get("sequences"), list) and len(c["sequences"]) > 1]
    if multi:
        bad = [c for c in multi
               if not all(isinstance(s, str) and s for s in c["sequences"])]
        if bad:
            ids = ",".join(str(c.get("id")) for c in bad[:3])
            fail("multi_seq_labels", f"bad_cards={ids}")
        emit("clearing.smoke.passed",
             f"cards={len(cards)}", f"multi_seq={len(multi)}",
             f"sample_card={multi[0].get('id')}",
             f"sample_seqs={','.join(multi[0]['sequences'])}")
    else:
        emit("clearing.smoke.no_multi_seq",
             f"cards={len(cards)}",
             "note=AC4_conditional_unsatisfiable_until_2329")
        emit("clearing.smoke.passed", f"cards={len(cards)}", "multi_seq=0")
    return 0


if __name__ == "__main__":
    sys.exit(validate(sys.stdin.read()))
