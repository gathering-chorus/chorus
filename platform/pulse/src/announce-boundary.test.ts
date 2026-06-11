// #3357 — the announce boundary (RED first). Fixtures are TODAY's real tape
// (2026-06-11): the event-loop alert that hit Jeff 20+ times across three
// terminals, the cards-family typed refusals announced as errors, Wren's six
// self-echoes. The boundary types every delivery, dedupes by class signature,
// kills self-echo, and meters what it does — without losing information.

import {
  classify,
  signature,
  decide,
  AnnounceState,
} from './announce-boundary';

const EVENT_LOOP_ALERT =
  'chorus-api event loop blocked 8000ms at 2026-06-11 14:40:00 EDT. Captured op: probe-timeout. No cause inferred; this is the measured block only.';
const EVENT_LOOP_ALERT_2 =
  'chorus-api event loop blocked 6083ms at 2026-06-11 13:30:35 EDT. The slow request is in the access log at this timestamp.';
const CARDS_REFUSAL =
  '[mcp.error] mcp.tool.error tool=chorus_cards_tag — chorus_cards_tag refused: use-cards-set — setting a domain value is owned by chorus_cards_set (one writer per field, ADR-031).';
const REAL_MCP_ERROR =
  '[mcp.error] mcp.tool.error tool=chorus_werk — spawn ENOENT: werk binary not found';
const ROLE_MESSAGE =
  '[#3339 ACK — silas] Read the card + the reconcile receipts. (1) Products: …';

describe('#3357 classify — typed lanes and classes', () => {
  test('role-to-role message is lane=role, class=message', () => {
    const c = classify('silas', 'kade', ROLE_MESSAGE);
    expect(c.lane).toBe('role');
    expect(c.cls).toBe('message');
  });

  test('system alert is lane=machine, class=alert', () => {
    const c = classify('system', 'jeff', EVENT_LOOP_ALERT);
    expect(c.lane).toBe('machine');
    expect(c.cls).toBe('alert');
  });

  test("typed refusal text is class=refusal — control flow, not error (today's cards cluster)", () => {
    const c = classify('chorus-mcp', 'wren', CARDS_REFUSAL);
    expect(c.cls).toBe('refusal');
  });

  test('a real tool failure stays class=error', () => {
    const c = classify('chorus-mcp', 'wren', REAL_MCP_ERROR);
    expect(c.cls).toBe('error');
  });
});

describe('#3357 signature — repeats collapse, distinct incidents do not', () => {
  test('the same alert with different numbers/timestamps has ONE signature', () => {
    expect(signature(EVENT_LOOP_ALERT)).toBe(signature(EVENT_LOOP_ALERT_2.slice(0, 60)));
  });

  test('different alerts have different signatures', () => {
    expect(signature(EVENT_LOOP_ALERT)).not.toBe(signature(CARDS_REFUSAL));
  });
});

describe('#3357 decide — the boundary policy', () => {
  const t0 = 1_000_000;

  test('self-echo is killed: a sender never receives its own send', () => {
    const st = new AnnounceState();
    const d = decide('wren', 'wren', ROLE_MESSAGE, t0, st);
    expect(d.deliver).toBe(false);
    expect(d.suppressReason).toBe('self-echo');
  });

  test('typed refusals are suppressed as benign control flow', () => {
    const st = new AnnounceState();
    const d = decide('chorus-mcp', 'wren', CARDS_REFUSAL, t0, st);
    expect(d.deliver).toBe(false);
    expect(d.suppressReason).toBe('benign-refusal');
  });

  test('machine repeats collapse: first delivers, repeats within cooldown suppress with a count', () => {
    const st = new AnnounceState();
    const first = decide('system', 'jeff', EVENT_LOOP_ALERT, t0, st);
    expect(first.deliver).toBe(true);
    // same incident, new numbers, 10 minutes later — still inside the machine cooldown
    const repeat = decide('system', 'jeff', EVENT_LOOP_ALERT_2, t0 + 10 * 60_000, st);
    expect(repeat.deliver).toBe(false);
    expect(repeat.suppressReason).toBe('dup-class');
    expect(repeat.suppressedCount).toBe(1);
    // after the cooldown lapses it announces again, carrying the suppressed count
    const later = decide('system', 'jeff', EVENT_LOOP_ALERT, t0 + 31 * 60_000, st);
    expect(later.deliver).toBe(true);
    expect(later.suppressedSinceLast).toBe(1);
  });

  test('role-to-role messages always deliver (exact-dup guard #3335 stays upstream)', () => {
    const st = new AnnounceState();
    const a = decide('silas', 'kade', ROLE_MESSAGE, t0, st);
    const b = decide('silas', 'kade', ROLE_MESSAGE + ' more', t0 + 1000, st);
    expect(a.deliver).toBe(true);
    expect(b.deliver).toBe(true);
  });

  test('real machine errors deliver first time (never silently eaten)', () => {
    const st = new AnnounceState();
    const d = decide('chorus-mcp', 'wren', REAL_MCP_ERROR, t0, st);
    expect(d.deliver).toBe(true);
  });
});

describe("#3357 AC6 — replay today's tape: ≤1/3 the deliveries, zero loss", () => {
  test('the 2026-06-11 machine tape collapses', () => {
    const st = new AnnounceState();
    let delivered = 0;
    const minute = 60_000;
    let t = 0;
    // 7 event-loop alerts over ~2.5h (real cadence), each to 3 terminals = 21 deliveries before
    for (let i = 0; i < 7; i++) {
      t = i * 22 * minute;
      for (const to of ['jeff', 'wren', 'silas']) {
        const d = decide('system', to, EVENT_LOOP_ALERT.replace('8000', String(4000 + i * 700)), t, st);
        if (d.deliver) delivered++;
      }
    }
    // ~10 cards-family refusal announcements (the cluster)
    for (let i = 0; i < 10; i++) {
      const d = decide('chorus-mcp', 'wren', CARDS_REFUSAL, t + i * minute, st);
      if (d.deliver) delivered++;
    }
    // 6 self-echoes
    for (let i = 0; i < 6; i++) {
      const d = decide('wren', 'wren', ROLE_MESSAGE + i, t + i * minute, st);
      if (d.deliver) delivered++;
    }
    // before: 21 + 10 + 6 = 37 deliveries. AC: ≤ 1/3.
    expect(delivered).toBeLessThanOrEqual(12);
    expect(delivered).toBeGreaterThan(0); // never silent — the incident IS announced
  });
});
