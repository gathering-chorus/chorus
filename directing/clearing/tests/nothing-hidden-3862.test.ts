// @test-type: unit — in-memory router, no live services.
/**
 * #3862 — the room must not hide what a role says to Jeff.
 *
 * Jeff, 2026-08-13: "i cant operate in clearing when 50% of the messages dont
 * even fucking render", "i dont want to hide any fucking nudges", and — the one
 * that settles what `visible:false` actually means to a reader — "the ui
 * definitely does not show collapsed messages that i can unfold - they
 * literally are NOT THERE".
 *
 * Every row below is REAL. They were pulled from the running room's own store
 * via /api/messages?includeHidden=1 at 17:22 Boston, with their recorded
 * classification. Three replies written directly to Jeff were filed hidden; a
 * debug probe, test traffic, and a tool error rendered full-size on his phone.
 *
 * NEGATIVE PROOF (#3734): this fixture must FAIL on the code as it stands.
 * Each case asserts the OPPOSITE of what the router does today, so a run
 * against unmodified router.ts goes red on the hidden-reply cases. A version of
 * this file that passes before the fix is the wrong file — see the last block,
 * which fails if the corpus stops containing a case the pre-fix classifier
 * gets wrong.
 */

import { MessageRouter } from '../src/router';

const TS = '2026-08-13T21:22:00Z';

// ---- Replies written TO Jeff that the room hid. Observed hidden today. ----

describe('#3862 — replies to Jeff that the room hid', () => {
  it('renders a reply that names other roles (was filed role-to-role, hidden)', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'wren',
      text: 'Opened it. Three real things, none of which the cards touched. Kade has streams. #3827 is mine.',
      ts: TS,
    });
    expect(r.getRecent(10, true)[0].visible).toBe(true);
  });

  it('renders a reply from a turn that used tools (was tagged pm-thinking, hidden)', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'wren',
      text: "Silas confirms his seam needs no change. I can't edit canonical without a WIP card.",
      ts: TS,
      type: 'pm-thinking',
    });
    expect(r.getRecent(10, true)[0].visible).toBe(true);
  });

  it('renders a reply that quotes a filesystem path (isSystemNoise ate it)', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'wren',
      text: 'The fix is one block in /Users/jeffbridwell/CascadeProjects/chorus/directing/clearing/public/index.html',
      ts: TS,
    });
    expect(r.getRecent(10, true)[0].visible).toBe(true);
  });

  it('renders a role-to-role nudge — Jeff: "i dont want to hide any fucking nudges"', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'silas',
      text: '[nudge from silas | 2026-08-13 16:54 Boston] The importmap fix is yours. Ship it.',
      ts: TS,
    });
    expect(r.getRecent(10, true)[0].visible).toBe(true);
  });
});

// ---- Machinery that rendered as conversation on his phone. ----

describe('#3862 — machinery that must stay out of the room', () => {
  it('keeps test traffic out', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'silas', text: '[e2e-ack] silas received [e2e-test] e2e-lan-456', ts: TS });
    expect(r.getRecent(10, true)[0].visible).toBe(false);
  });

  it('keeps a tool error out', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'kade',
      text: 'API Error: 529 Overloaded. This is a server-side issue, usually temporary — try again in a moment.',
      ts: TS,
    });
    expect(r.getRecent(10, true)[0].visible).toBe(false);
  });

  it('keeps an MCP refusal out, and never attributes one to Jeff', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'jeff',
      text: 'tool=chorus_nudge_message type=is-error msg=word-cap: 90 words, cap is 50.',
      ts: TS,
    });
    expect(r.getRecent(10, true)[0].visible).toBe(false);
  });

  it('keeps synthetic health probes out — 129 of 200 rows are these', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'probe', text: 'clearing probe', ts: TS, type: 'probe' });
    expect(r.getRecent(10, true)[0].visible).toBe(false);
  });
});

describe('#3862 — what must keep working', () => {
  it("renders Jeff's own message", () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'jeff',
      text: 'weve done a lot of cards today and i see no improvement in clearing',
      ts: TS,
    });
    expect(r.getRecent(10, true)[0].visible).toBe(true);
  });
});

/**
 * The fixture's own negative proof.
 *
 * #3734's worked case is a fixture that passed for the wrong reason. This guard
 * fails if the file ever stops exercising a shape the OLD rules got wrong — if
 * someone turns this suite green by weakening the cases instead of the
 * classifier. The three shapes are exactly the rules #3862 changes, and each
 * was hidden by a DIFFERENT rule.
 */
describe('#3862 — the fixture must be capable of failing', () => {
  it('still exercises each shape the old rules hid, one per rule', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'wren', text: 'Kade has streams. #3827 is mine.', ts: TS });
    r.ingest({ from: 'wren', text: 'a finished reply', ts: TS, type: 'pm-thinking' });
    r.ingest({ from: 'wren', text: 'see /Users/jeffbridwell/x.html', ts: TS });

    const seen = r.getRecent(10, true).map((m) => m.visible);
    expect(seen).toEqual([true, true, true]);
  });
});
