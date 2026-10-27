// @domain: awareness
// @test-type: unit — composeHeader in platform/pulse/src/pulse-header.ts over fake sources, and GET /pulse in platform/pulse/src/service.ts; no live services
/**
 * #4757 — GET /pulse reads each line from the API that holds it, at request time.
 * Jeff 2026-10-26: "why are we writing prompts we already have messages api".
 */
import { composeHeader, cachedCards, type Sources } from './pulse-header';

const now = new Date('2026-10-27T00:07:00Z');
const ago = (s: number) => new Date(now.getTime() - s * 1000).toISOString();

const sources = (over: Partial<Sources> = {}): Sources => ({
  cards: () => Promise.resolve([
    { card: '4747', role: 'kade', step: 'land: borg-queen-proves', live: true, since: ago(120), run: 'cicd run 2' },
    { card: '4755', role: 'kade', step: 'no run yet', live: false, since: '', run: '' },
  ]),
  actorState: () => ({ actors: [
    { aboutPrincipal: 'principal-wren', stream: [{ timestamp: ago(4) }] },
    { aboutPrincipal: 'principal-jeff', stream: [{ timestamp: ago(223) }] },
  ] }),
  messages: () => [{ from: 'jeff', content: 'i ask for a and get b and c', created_at: ago(180) }],
  alerts: () => Promise.resolve([{ name: 'app-down', label: 'Gathering app is not responding' }]),
  ...over,
});

test("the cards line is /cws's step and wait for each WIP card", async () => {
  const h = await composeHeader(sources(), now);
  expect(h.data.cards).toEqual({ source: '/cws', rows: [
    { card: '4747', role: 'kade', step: 'land: borg-queen-proves', waitedSeconds: 120 },
    { card: '4755', role: 'kade', step: 'no run yet', waitedSeconds: null },
  ] });
});

test('the roles line is moving or quiet from each role\'s last call', async () => {
  const h = await composeHeader(sources(), now);
  expect(h.data.roles.rows).toEqual([
    { role: 'wren', state: 'moving', secondsSinceLastCall: 4 },
    { role: 'jeff', state: 'quiet', secondsSinceLastCall: 223 },
  ]);
  expect(h.data.messages.rows?.[0]).toMatchObject({ from: 'jeff', body: 'i ask for a and get b and c' });
  expect(h.data.alerts.rows).toEqual([{ name: 'app-down', label: 'Gathering app is not responding' }]);
});

// NEGATIVE PROOF: a source that cannot be read says so on its own line; the other lines still read
test('negative proof: an unreadable source is unmeasured, never stale rows', async () => {
  const h = await composeHeader(sources({
    cards: () => Promise.reject(new Error('chorus-werk-status --json: exit 2')),
    actorState: () => ({ actors: [] }),
  }), now);
  expect(h.data.cards).toEqual({ source: '/cws', unmeasured: 'chorus-werk-status --json: exit 2' });
  expect(h.data.roles).toEqual({ source: '/api/actor-state', unmeasured: 'actor-state lists no actors' });
  expect(h.data.messages.rows).toHaveLength(1);
});

// NEGATIVE PROOF: a card that left WIP is gone from the next read; nothing is written or kept
test('negative proof: a card that leaves WIP is gone on the next read', async () => {
  let wip = [{ card: '4747', role: 'kade', step: 'build', live: true, since: ago(60), run: 'cicd run 1' }];
  const src = sources({ cards: () => Promise.resolve(wip) });
  expect((await composeHeader(src, now)).data.cards.rows).toHaveLength(1);
  wip = [];
  expect((await composeHeader(src, now)).data.cards.rows).toEqual([]);
});

test('/cws is read at most once per 30 s; a read after that sees the change', async () => {
  let calls = 0;
  const read = cachedCards(() => { calls++; return Promise.resolve([]); }, 50);
  await read(); await read();
  expect(calls).toBe(1);
  await new Promise(r => setTimeout(r, 60));
  await read();
  expect(calls).toBe(2);
});

test('GET /pulse answers the header from pulse itself', async () => {
  const request = require('supertest');
  const { createApp } = require('./service');
  const { MessageStore } = require('./store');
  const res = await request(createApp(new MessageStore(':memory:'))).get('/pulse');
  expect(res.status).toBe(200);
  expect(res.body.kind).toBe('Header');
  expect(Object.keys(res.body.data)).toEqual(['cards', 'roles', 'messages', 'alerts']);
  // no actor-state service here: the roles line says so rather than showing nothing
  expect(res.body.data.roles.unmeasured).toBeTruthy();
}, 60_000);
