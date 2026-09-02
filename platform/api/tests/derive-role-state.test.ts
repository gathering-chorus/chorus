// @test-type: unit — pure function over fixture spine lines; no file, no service, brings its own world.
/**
 * #4028 — role state is a function over the streams, nothing else.
 *
 * Jeff, 2026-08-28: "i dont know why we cant rip out 'declared' role state and
 * have it be derived directly from streams." Agreed with Silas 2026-09-02
 * (chat wren-silas-1788369280): the hooks daemon's stream sees a role WORKING
 * (every tool call lands as hook.decision / context.inject.request, credential
 * or not); the security envelope only attributes. "blocked" is the one thing a
 * role still says, as an EVENT that expires the moment activity resumes.
 * Nothing is stored; the state is recomputed on every read.
 *
 * Every test here describes what Jeff sees on /roles, the Clearing tile and
 * the boot envelope — never a file.
 */

import { stateFromStreams, ACTIVITY_EVENTS, type SpineLine } from '../src/derive-role-state';

const T0 = Date.parse('2026-09-02T15:00:00-04:00');
const min = (n: number) => n * 60_000;
const at = (offsetMin: number) => new Date(T0 - min(offsetMin)).toISOString();

function ev(role: string, event: string, offsetMin: number, extra: Partial<SpineLine> = {}): SpineLine {
  return { timestamp: at(offsetMin), role, event, ...extra };
}

const wip = (id: number, owner: string) => ({ id, owner });

describe('stateFromStreams — AC1: one function over the last 15 min of streams', () => {
  it('a role with tool calls in the window is building; its card comes from the board', () => {
    const r = stateFromStreams({
      role: 'wren',
      events: [ev('wren', 'hook.decision', 1), ev('wren', 'context.inject.request', 3)],
      wipCards: [wip(4028, 'Wren')],
      now: T0,
    });
    expect(r.state).toBe('building');
    expect(r.card).toBe(4028);
    expect(r.source).toBe('streams');
    expect(r.lastActivity).toBe(at(1));
  });

  it('a role whose last tool call is older than the window is idle — silence is the reason, not the board', () => {
    const r = stateFromStreams({
      role: 'wren',
      events: [ev('wren', 'hook.decision', 16)],
      wipCards: [wip(4028, 'Wren')],
      now: T0,
    });
    expect(r.state).toBe('idle');
    expect(r.card).toBe(4028); // the board still says what it is on; the streams say it is not working it right now
  });

  it('never answers "unknown": a role with no events and no cards is idle', () => {
    const r = stateFromStreams({ role: 'kade', events: [], wipCards: [], now: T0 });
    expect(r.state).toBe('idle');
    expect(r.card).toBeNull();
  });

  it("only the role's own events count — a busy peer does not make you building", () => {
    const r = stateFromStreams({
      role: 'wren',
      events: [ev('silas', 'hook.decision', 1), ev('kade', 'agent.action', 2)],
      wipCards: [],
      now: T0,
    });
    expect(r.state).toBe('idle');
  });

  it('a demo presented by the role with no go/verdict after it → waiting (the demo lookback is 60 min)', () => {
    const r = stateFromStreams({
      role: 'silas',
      events: [ev('silas', 'demo.presented', 40, { card_id: '4065' }), ev('silas', 'hook.decision', 1)],
      wipCards: [wip(4065, 'Silas')],
      now: T0,
    });
    expect(r.state).toBe('waiting');
    expect(r.card).toBe(4065);
  });

  it('a demo that received its go is no longer waiting — the role is back to what the streams say', () => {
    const r = stateFromStreams({
      role: 'silas',
      events: [
        ev('silas', 'demo.presented', 40, { card_id: '4065' }),
        ev('jeff', 'demo.go', 30, { card_id: '4065' }),
        ev('silas', 'hook.decision', 1),
      ],
      wipCards: [wip(4065, 'Silas')],
      now: T0,
    });
    expect(r.state).toBe('building');
  });

  it('loom-gemba active in the last 10 min → observing, with the target as gemba', () => {
    const r = stateFromStreams({
      role: 'wren',
      events: [ev('wren', 'gemba.observation.started', 4, { payload: 'target=kade' }), ev('wren', 'hook.decision', 1)],
      wipCards: [],
      now: T0,
    });
    expect(r.state).toBe('observing');
    expect(r.gemba).toBe('kade');
  });

  it('two WIP cards → building with card null and multi_wip true; the state does not pick one', () => {
    const r = stateFromStreams({
      role: 'kade',
      events: [ev('kade', 'agent.action', 1)],
      wipCards: [wip(4060, 'Kade'), wip(4063, 'kade')],
      now: T0,
    });
    expect(r.state).toBe('building');
    expect(r.card).toBeNull();
    expect(r.multi_wip).toBe(true);
  });
});

describe('stateFromStreams — blocked is an event that expires, not a stored state', () => {
  it('role.blocked as the latest thing the role did → blocked, with its detail', () => {
    const r = stateFromStreams({
      role: 'wren',
      events: [ev('wren', 'hook.decision', 5), ev('wren', 'role.blocked', 2, { detail: 'waiting on Jeff: is Spine a product' })],
      wipCards: [wip(4045, 'Wren')],
      now: T0,
    });
    expect(r.state).toBe('blocked');
    expect(r.detail).toBe('waiting on Jeff: is Spine a product');
  });

  it('activity after role.blocked un-blocks it — the role is working again, so it is building', () => {
    const r = stateFromStreams({
      role: 'wren',
      events: [ev('wren', 'role.blocked', 5, { detail: 'x' }), ev('wren', 'hook.decision', 1)],
      wipCards: [wip(4045, 'Wren')],
      now: T0,
    });
    expect(r.state).toBe('building');
    expect(r.detail).toBeUndefined();
  });

  it('a role.blocked older than the window is gone — nothing reverts, nothing is remembered', () => {
    const r = stateFromStreams({
      role: 'wren',
      events: [ev('wren', 'role.blocked', 20, { detail: 'x' })],
      wipCards: [],
      now: T0,
    });
    expect(r.state).toBe('idle');
  });
});

describe('stateFromStreams — AC3 negative proof (#3734): a declaration is not an input', () => {
  it('a role that "declared building" but whose streams are silent for 20 min is idle — the function has no declared parameter to consult', () => {
    // The pre-#4028 endpoint answered the file. This function's signature cannot
    // even receive one; the closest a caller can get is a role.state.changed
    // event, and that is NOT activity.
    const r = stateFromStreams({
      role: 'silas',
      events: [ev('silas', 'role.state.changed', 20, { payload: 'state=building' })],
      wipCards: [wip(4058, 'Silas')],
      now: T0,
    });
    expect(r.state).toBe('idle');
    expect(ACTIVITY_EVENTS.has('role.state.changed')).toBe(false);
  });

  it('heartbeats and surfaced nudges are not work — a role that only heartbeats is idle', () => {
    const r = stateFromStreams({
      role: 'silas',
      events: [ev('silas', 'system.heartbeat', 1), ev('silas', 'nudge.surfaced', 2)],
      wipCards: [],
      now: T0,
    });
    expect(r.state).toBe('idle');
  });
});
