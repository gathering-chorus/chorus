// @test-type: unit — deriveActorState over fixture sessions; no live door.
// @domain: roles
// @card: #4985
// @owner: silas
/**
 * #4985 — Jeff 11-16: "they start when we log in and end when we log out." An agent's pane
 * session is live from login to logout, quiet or not. On 11-16 at 16:48 Kade and Abby were
 * logged in (pane reachable, row open; last turns 11:23 and 12:59 in Loki, no logout) and had
 * no Clearing v2 tile: pulse read their panes not live after 30 quiet minutes, so their state
 * went 'off' and the tile was filtered.
 */
import { deriveActorState, type ActorFactInput } from './actor-state';

const now = new Date('2026-11-16T21:48:00Z');
const KADE = 'principal-kade';
// last turn five hours ago: the 16:48 read of Kade
const QUIET = '2026-11-16T16:23:53Z';
const input = (session: Record<string, string>): ActorFactInput => ({
  now,
  principals: [KADE],
  sessions: [{ id: 's-kade', principal: KADE, channel: 'pane', lastSeenAt: QUIET, expiresAt: '2026-11-15T00:00:00Z', ...session } as never],
  actions: { kade: { lastActionAt: QUIET, recent5mCount: 0 } },
});

test('#4985 a logged-in pane quiet for five hours is still live, and the role is not off', () => {
  const k = deriveActorState(input({})).find((s) => s.aboutPrincipal === KADE);
  expect(k?.sessions.map((s) => s.live)).toEqual([true]);
  expect(k?.stateValue).not.toBe('off');
});

test('#4985 a logged-in pane with no turn at all since login is still live', () => {
  const k = deriveActorState({ ...input({ lastSeenAt: '' }), actions: {} }).find((s) => s.aboutPrincipal === KADE);
  expect(k?.sessions.map((s) => s.live)).toEqual([true]);
});

test('#4985 NEGATIVE PROOF: the same quiet pane, logged out, is not live and the role is off', () => {
  const k = deriveActorState(input({ endedAt: '2026-11-16T21:00:00Z', sessionState: 'closed' })).find((s) => s.aboutPrincipal === KADE);
  expect(k?.sessions.map((s) => s.live)).toEqual([false]);
  expect(k?.stateValue).toBe('off');
});

test('#4985 NEGATIVE PROOF: a quiet browser sign-in still goes stale (Jeff\'s sign-in is not a role session)', () => {
  const k = deriveActorState(input({ channel: 'browser', expiresAt: '2026-11-17T00:00:00Z' })).find((s) => s.aboutPrincipal === KADE);
  expect(k?.sessions.map((s) => s.live)).toEqual([false]);
});

// AC4 (Wren 16:53): a pane whose process died without a logout ends on a fact, never a timer.
// The sweep ends a crashed RUN (pid gone, chorus-principal sweep); with no live run the
// pane session is not live. Keyed on the run, not on reachability (Wren 16:55).
test('#4985 a pane whose run crashed (no live run) is not live, and the role is off', () => {
  const k = deriveActorState({ ...input({}), liveRun: { kade: false } }).find((s) => s.aboutPrincipal === KADE);
  expect(k?.sessions.map((s) => s.live)).toEqual([false]);
  expect(k?.stateValue).toBe('off');
});

test('#4985 NEGATIVE PROOF: a live run with no Presence (reachability offline) keeps a quiet pane live', () => {
  const k = deriveActorState({ ...input({}), liveRun: { kade: true }, reachability: { kade: 'offline' } }).find((s) => s.aboutPrincipal === KADE);
  expect(k?.sessions.map((s) => s.live)).toEqual([true]);
});

test('#4985 NEGATIVE PROOF: runs not read keep a pane as its row says (no guess from silence)', () => {
  const k = deriveActorState({ ...input({}), liveRun: undefined }).find((s) => s.aboutPrincipal === KADE);
  expect(k?.sessions.map((s) => s.live)).toEqual([true]);
});

test('#4985 NEGATIVE PROOF: a fresh browser sign-in is never ended by its principal having no run (the run rule is for panes only)', () => {
  const fresh = '2026-11-16T21:47:00Z';
  const k = deriveActorState({
    ...input({ channel: 'browser', expiresAt: '2026-11-17T00:00:00Z', lastSeenAt: fresh }),
    actions: { kade: { lastActionAt: fresh, recent5mCount: 1 } },
    liveRun: { kade: false },
  }).find((s) => s.aboutPrincipal === KADE);
  expect(k?.sessions.map((s) => s.live)).toEqual([true]);
});
