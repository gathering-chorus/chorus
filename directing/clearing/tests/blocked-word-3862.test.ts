// @test-type: unit — in-memory router, no live services.
/**
 * #3862 (second leg) — saying the word "blocked" must not raise a CRITICAL alert.
 *
 * Observed live in Jeff's room at 17:47 Boston. The ALERTS pane, which is meant
 * for system failures, was showing chat:
 *
 *   CRITICAL 5:37 PM  wren: Nothing lost — #3862 is committed and pushed…
 *   CRITICAL 5:21 PM  silas: You're right — "noise echo"…
 *
 * The chain: router.ts classifies any message whose TEXT CONTAINS "blocked" as
 * type 'blocked'; index.html then reads type 'blocked' as level critical and
 * files it in the alerts panel. So a reply that merely quotes an alert — or
 * says "I'm not blocked" — becomes an alert itself.
 *
 * Same shape as the rules this card already fixed: a check that cannot separate
 * the two states it exists to separate. "A role IS blocked" and "a role SAID
 * blocked" are not the same fact, and only one of them is an alert.
 *
 * NEGATIVE PROOF (#3734): the first three cases assert the OPPOSITE of today's
 * behaviour and must fail before the fix. The last case pins that real blocked
 * signalling still types 'blocked' — without it, this suite would pass just as
 * well against a router that deleted the rule outright, which would silence the
 * alerts panel instead of correcting it.
 */

import { MessageRouter } from '../src/router';

const TS = '2026-08-13T21:47:00Z';

describe('#3862 — the word "blocked" in prose is not an alert', () => {
  it('does not alert on a reply that quotes an alert', () => {
    const r = new MessageRouter();
    r.ingest({
      from: 'wren',
      text: 'Two fired while you were out: chorus-api event loop blocked 4712ms at SendStream.sendFile.',
      ts: TS,
    });
    expect(r.getRecent(10)[0].type).not.toBe('blocked');
  });

  it('does not alert on a role saying it is NOT blocked', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'kade', text: "I'm not blocked — the build is just slow.", ts: TS });
    expect(r.getRecent(10)[0].type).not.toBe('blocked');
  });

  it('does not alert on the word inside a longer sentence', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'silas', text: 'The canonical write guard blocked my edit, so I moved to the werk.', ts: TS });
    expect(r.getRecent(10)[0].type).not.toBe('blocked');
  });

  // The other half: real blocked signalling must still reach the panel, or this
  // card trades a noisy alerts pane for a dead one.
  it('still types an explicit blocked declaration', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'silas', text: '[blocked] waiting on fuseki auth', ts: TS });
    expect(r.getRecent(10)[0].type).toBe('blocked');
  });

  it('still types a message the emitter tagged blocked', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'silas', text: 'waiting on fuseki auth', ts: TS, type: 'blocked' });
    expect(r.getRecent(10)[0].type).toBe('blocked');
  });
});
