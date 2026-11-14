// @test-type: unit — deriveActorState over fixture sessions; no live door.
// @domain: roles
// @card: #4948
// @owner: wren
/**
 * #4948 — an agent's session lasts from login to logout. Its expiresAt is written once at
 * login (the shape requires it) and never renewed, so pulse ignores it on a pane session;
 * all 10 open pane rows carried a lapsed one on 11-10 (Silas, pair 4948).
 */
import { deriveActorState, type ActorFactInput } from './actor-state';

const now = new Date('2026-11-10T21:00:00Z');
const WREN = 'principal-wren';
const input = (session: Record<string, string>): ActorFactInput => ({
  now,
  principals: [WREN],
  sessions: [{ id: 's-wren', principal: WREN, channel: 'pane', lastSeenAt: '2026-11-10T20:59:00Z', ...session } as never],
  actions: { wren: { lastActionAt: '2026-11-10T20:59:00Z', recent5mCount: 3 } },
});

test('#4948 an agent pane session whose login expiry lapsed, with a turn a minute ago, reads live', () => {
  const wren = deriveActorState(input({ expiresAt: '2026-11-09T16:32:00Z' })).find((s) => s.aboutPrincipal === WREN);
  expect(wren?.sessions.map((s) => s.live)).toEqual([true]);
});

test('#4948 NEGATIVE PROOF: the same session ended at logout never reads live', () => {
  const wren = deriveActorState(input({ endedAt: '2026-11-10T20:59:30Z' })).find((s) => s.aboutPrincipal === WREN);
  expect(wren?.sessions.map((s) => s.live)).toEqual([false]);
});

test('#4948 NEGATIVE PROOF: a browser sign-in past its expiresAt still never reads live', () => {
  const wren = deriveActorState(input({ channel: 'browser', expiresAt: '2026-11-10T20:00:00Z' })).find((s) => s.aboutPrincipal === WREN);
  expect(wren?.sessions.map((s) => s.live)).toEqual([false]);
});
