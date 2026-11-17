// @test-type: unit — the WIP fact read from a stubbed /context/board/wip, then deriveActorState; no live door.
// @domain: roles
// @card: #4987
// @owner: wren
// #4987 (AC2): a teammate's tile shows a card only while the board holds it in WIP
/**
 * #4987 — Jeff's screenshot 2026-11-17 05:45: Wren's Teammates row read "building · #4986"
 * ten minutes after #4986 landed and went Done. The tile's card is pulse's WIP fact, read from
 * /api/chorus/context/board/wip; that endpoint answered from a stale copy. The endpoint now reads
 * the board (api tests); here: the tile follows whatever the endpoint says, and an unreadable
 * board is recorded as unmeasured, never quietly treated as "no card".
 */
import { deriveActorState, fetchWipCardsFact, type ActorFactInput } from './actor-state';

const now = new Date('2026-11-17T11:00:00Z');
const WREN = 'principal-wren';
const realFetch = global.fetch;
afterEach(() => { global.fetch = realFetch; });

function boardSays(status: number, cards: Array<{ id: number; owner: string; title?: string }>): void {
  global.fetch = (async () => ({ ok: status === 200, status, json: async () => ({ data: { cards } }) })) as unknown as typeof fetch;
}

const live = (wipCards: ActorFactInput['wipCards']): ActorFactInput => ({
  now,
  principals: [WREN],
  sessions: [{ id: 's-wren', principal: WREN, channel: 'pane', lastSeenAt: '2026-11-17T10:59:00Z', expiresAt: '2026-11-18T00:00:00Z' } as never],
  actions: { wren: { lastActionAt: '2026-11-17T10:59:00Z', recent5mCount: 3 } },
  wipCards,
});

test('#4987 AC2 a card the board holds in WIP shows on the owner\'s tile', async () => {
  boardSays(200, [{ id: 4987, owner: 'wren', title: 'Header and Teammates read the board' }]);
  const unmeasured: Array<{ source: string; reason: string }> = [];
  const wip = await fetchWipCardsFact('http://api', unmeasured);
  const w = deriveActorState(live(wip)).find((s) => s.aboutPrincipal === WREN);
  expect(w?.stateValue).toBe('building');
  expect(w?.currentCard).toBe(4987);
  expect(unmeasured).toEqual([]);
});

test('#4987 AC2 NEGATIVE PROOF: once the board no longer holds the card (landed, Done), the tile shows no card and is not building', async () => {
  boardSays(200, []);
  const wip = await fetchWipCardsFact('http://api', []);
  const w = deriveActorState(live(wip)).find((s) => s.aboutPrincipal === WREN);
  expect(w?.stateValue).not.toBe('building');
  expect(w?.currentCard ?? null).toBeNull();
});

test('#4987 AC2 NEGATIVE PROOF: an unreadable board is recorded as unmeasured, not passed off as an empty WIP column', async () => {
  boardSays(503, []);
  const unmeasured: Array<{ source: string; reason: string }> = [];
  await fetchWipCardsFact('http://api', unmeasured);
  expect(unmeasured).toEqual([{ source: 'chorus:board:wip', reason: 'HTTP 503' }]);
});
