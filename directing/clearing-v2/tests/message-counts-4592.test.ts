// @test-type: unit — pure counting over fixture spine lines and notes; no Loki, no relay.
// @card: #4592
// @owner: silas
// @domain: messages
/**
 * #4592 — one page shows, for today, each message kind: sent, reached Buzz,
 * and the gap. Jeff, 10-17 17:42: "im really not clear what % of messages are
 * in buzz and what are going old path".
 */
import { sentFromPulse, countKinds, bostonMidnight } from '../src/sources/message-counts';
import { renderCountsPage } from '../src/views';

// pulse's /api/messages rows, as it stores them (nudges keep their delivery header)
const pulse = [
  { type: 'nudge', from: 'kade', to: 'silas', content: '[nudge from kade | 2026-10-18 10:00 Boston] ping, again' },
  { type: 'nudge', from: 'system', to: 'wren', content: '[nudge from system | 2026-10-18 10:01 Boston] log stale' },
  { type: 'nudge', from: 'wren', to: 'jeff', content: '[nudge from wren | 2026-10-18 10:02 Boston] ready' },
  { type: 'jeff-input', from: 'jeff', to: 'wren', content: 'hello team' },
  { type: 'jeff-input', from: 'jeff', to: 'kade', content: 'hello team' },
];
const note = (content: string, tags: string[][] = [['t', 'team']]) => ({ content, tags });

describe('#4592 sent vs reached, per kind', () => {
  test('each kind is read from pulse as the text the room carries; one Jeff message counts once', () => {
    const sent = sentFromPulse(pulse);
    expect(sent).toEqual([
      { kind: 'nudge-between-roles', text: '[nudge from kade → silas] ping, again' },
      { kind: 'system-alert', text: '[alert → wren] log stale' },
      { kind: 'nudge-to-jeff', text: 'ready' },
      { kind: 'jeff-input', text: 'hello team' },
    ]);
  });

  test('everything that reached Buzz matches: no gap in any measured row', () => {
    const notes = [
      note('[nudge from kade → silas] ping, again', [['t', 'team'], ['kind', 'nudge']]),
      note('[alert → wren] log stale', [['t', 'team'], ['kind', 'alert']]),
      note('ready'), note('hello team'), note('a reply', [['t', 'reply']]),
    ];
    const rows = countKinds(sentFromPulse(pulse), notes);
    for (const r of rows.filter((x) => x.sent !== null)) expect(r.gap).toBe(0);
    expect(rows.find((r) => r.kind === 'role-reply')).toEqual(expect.objectContaining({ sent: null, reached: 1, gap: null }));
  });

  // NEGATIVE PROOF (AC): a kind that skips Buzz shows a non-zero gap. No alert
  // and no between-roles note reached the relay; a note with the right words
  // but not the room's text does not count as reached.
  test('negative proof: a kind that skips Buzz shows a non-zero gap', () => {
    const rows = countKinds(sentFromPulse(pulse), [note('ping, again'), note('ready'), note('hello team')]);
    const by = Object.fromEntries(rows.map((r) => [r.kind, r]));
    expect(by['system-alert'].gap).toBe(1);
    expect(by['nudge-between-roles'].gap).toBe(1);
    expect(by['nudge-to-jeff'].gap).toBe(0);
    expect(by['jeff-input'].gap).toBe(0);
  });

  test('the day starts at Boston midnight', () => {
    expect(bostonMidnight(new Date('2026-10-18T14:30:00Z'))).toBe(Date.parse('2026-10-18T04:00:00Z') / 1000);
  });

  test('the page shows one row per kind with sent, reached and gap, and names what it cannot measure', () => {
    const html = renderCountsPage(countKinds(sentFromPulse(pulse), []), { day: '2026-10-18' }, []);
    expect(html).toContain('Nudge between roles');
    expect(html).toMatch(/System alert<\/td>\s*<td[^>]*>1<\/td>\s*<td[^>]*>0<\/td>\s*<td[^>]*>1<\/td>/);
    expect(html).toContain('not measured');
  });

  test('a source that cannot be read is named on the page, never shown as zero', () => {
    const html = renderCountsPage([], { day: '2026-10-18', error: 'pulse answered 502 for nudge' }, []);
    expect(html).toContain('pulse answered 502');
    expect(html).not.toMatch(/<td[^>]*>0<\/td>/);
  });
});

import { BuzzRelayClient, MockWebSocket } from '../src/relay-client';

describe('#4592 the relay is asked for today, separately from the room', () => {
  const ev = (id: string) => ({ id, pubkey: 'p', created_at: 1, kind: 1, tags: [['t', 'team']], content: id, sig: 's' });

  test('fetchSince collects its own subscription until EOSE, and the room never sees those events', async () => {
    const ws = new MockWebSocket();
    const roomGot: string[] = [];
    const relay = new BuzzRelayClient({ relayUrl: 'ws://x', wsFactory: () => ws, onEvent: (e) => roomGot.push(e.id) });
    relay.connect();
    ws.triggerOpen();
    const pending = relay.fetchSince(1000);
    const req = JSON.parse(ws.sentMessages[ws.sentMessages.length - 1]);
    expect(req[0]).toBe('REQ');
    expect(req[2]).toEqual(expect.objectContaining({ since: 1000, kinds: [1] }));
    ws.triggerMessage(JSON.stringify(['EVENT', req[1], ev('a')]));
    ws.triggerMessage(JSON.stringify(['EVENT', req[1], ev('b')]));
    ws.triggerMessage(JSON.stringify(['EOSE', req[1]]));
    expect((await pending).map((e) => e.id)).toEqual(['a', 'b']);
    expect(roomGot).toEqual([]);
    expect(JSON.parse(ws.sentMessages[ws.sentMessages.length - 1])).toEqual(['CLOSE', req[1]]);
  });

  // NEGATIVE PROOF: a relay that never finishes is a refusal with a reason,
  // never an empty list that would read as "nothing reached Buzz".
  test('negative proof: no EOSE in time rejects, naming why', async () => {
    const ws = new MockWebSocket();
    const relay = new BuzzRelayClient({ relayUrl: 'ws://x', wsFactory: () => ws });
    relay.connect();
    ws.triggerOpen();
    await expect(relay.fetchSince(1000, 20)).rejects.toThrow(/EOSE/);
  });
});

import http from 'http';
import { createApp } from '../src/server';

describe('#4592 the /counts page', () => {
  // NEGATIVE PROOF: with the relay unreachable the page names the reason and
  // shows no zeros, so "0 reached" can never be a dead relay in disguise.
  test('an unreadable relay is named on the page, not shown as zeros', async () => {
    const { app } = createApp({ relayUrl: 'ws://127.0.0.1:3499', pulseUrl: 'http://127.0.0.1:9', knownKeys: new Map() });
    const server = http.createServer(app);
    await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
    const port = (server.address() as { port: number }).port;
    const html = await (await fetch(`http://127.0.0.1:${port}/counts`)).text();
    await new Promise<void>((r) => server.close(() => r()));
    expect(html).toContain('Counts could not be read');
    expect(html).not.toMatch(/<td[^>]*>0<\/td>/);
  });
});
