// @test-type: unit — injects readEvents/listWipCards/sparql stubs; no spine file, no live service, brings its own world.
/**
 * context-roles handler tests — #4028: /api/chorus/context/roles serves ONLY
 * the state derived from the streams. There is no declared file to read and
 * no "unknown" to answer.
 */

import { agentRolesFrom, loggedInRoles, fetchContextRoles, type ContextRolesDeps } from '../../src/handlers/context-roles';
import type { SpineLine } from '../../src/derive-role-state';

const T0 = Date.parse('2026-09-02T15:00:00-04:00');
const at = (offsetMin: number) => new Date(T0 - offsetMin * 60_000).toISOString();

function stubSparql(): ContextRolesDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

// #4432 — the roles door as it answers today, Abby Normal the fourth agent.
const DOOR = { data: [
  { name: 'abby-normal', roleKind: 'agent', rolePriority: '4' }, { name: 'jeff', roleKind: 'human', rolePriority: '0' },
  { name: 'kade', roleKind: 'agent', rolePriority: '1' }, { name: 'nightly', roleKind: '' },
  { name: 'silas', roleKind: 'agent', rolePriority: '2' }, { name: 'wren', roleKind: 'agent', rolePriority: '3' },
] };

function deps(over: Partial<ContextRolesDeps> = {}): ContextRolesDeps {
  return {
    sparql: stubSparql(),
    readEvents: () => [],
    listWipCards: () => [],
    listAgentRoles: async () => agentRolesFrom(DOOR).map((name) => ({ name, sessions: [] })),
    now: () => new Date(T0),
    ...over,
  };
}

describe('fetchContextRoles (#4028 — derived, never declared)', () => {
  it('#4432 returns every agent role the roles door lists, by rolePriority — Abby gets a tile', async () => {
    const r = await fetchContextRoles(deps(), '/api/chorus/context/roles');
    expect(r.status).toBe(200);
    expect(r.body.data.roles.map((x) => x.name)).toEqual(['kade', 'silas', 'wren', 'abby-normal']);
  });

  it('#4432 NEGATIVE PROOF: a door that cannot answer is a 503, never three guessed tiles', async () => {
    const r = await fetchContextRoles(deps({ listAgentRoles: async () => { throw new Error('HTTP 502'); } }), '/api/chorus/context/roles');
    expect(r.status).toBe(503);
    expect(JSON.stringify(r.body)).toMatch(/roles door unreadable/);
  });

  it('#4432 tiles follow logins: Jeff and the agents with an open session, Jeff first', () => {
    const sessions = { data: [
      { ownedBy: 'principal-wren', sessionState: 'open' }, { ownedBy: 'principal-abby-normal', sessionState: 'open' },
      { ownedBy: 'principal-kade', sessionState: 'closed' }, { ownedBy: 'principal-jeff', sessionState: 'open' },
    ] };
    expect(loggedInRoles(sessions, DOOR).map((r) => r.name)).toEqual(['jeff', 'wren', 'abby-normal']);
  });

  it('#4432 NEGATIVE PROOF: Abby logs out (session closed) and her tile goes', () => {
    const before = { data: [{ ownedBy: 'principal-abby-normal', sessionState: 'open' }] };
    const after = { data: [{ ownedBy: 'principal-abby-normal', sessionState: 'closed' }] };
    expect(loggedInRoles(before, DOOR).map((r) => r.name)).toEqual(['abby-normal']);
    expect(loggedInRoles(after, DOOR)).toEqual([]);
    expect(() => loggedInRoles({ error: 'down' }, DOOR)).toThrow(/sessions door/);
    // and Jeff's own tile follows his login the same way
    expect(loggedInRoles({ data: [{ ownedBy: 'principal-jeff', sessionState: 'closed' }] }, DOOR)).toEqual([]);
  });

  it('#4432 one tile per principal: two open sessions are one row carrying both, oldest first (Jeff 2026-10-06)', async () => {
    const sessions = { data: [
      { ownedBy: 'principal-wren', sessionState: 'open', channel: 'pane', startedAt: '2026-10-06T12:46:00Z', lastSeenAt: '2026-10-06T12:50:00Z' },
      { ownedBy: 'principal-wren', sessionState: 'open', channel: 'pane', startedAt: '2026-10-02T13:03:00Z', lastSeenAt: '2026-10-02T19:51:00Z' },
      { ownedBy: 'principal-wren', sessionState: 'closed', channel: 'pane', startedAt: '2026-10-01T09:00:00Z', lastSeenAt: '2026-10-01T10:00:00Z' },
    ] };
    const rows = loggedInRoles(sessions, DOOR);
    expect(rows).toHaveLength(1);
    expect(rows[0].sessions.map((x) => x.startedAt)).toEqual(['2026-10-02T13:03:00Z', '2026-10-06T12:46:00Z']);
    const r = await fetchContextRoles(deps({ listAgentRoles: async () => rows }), '/api/chorus/context/roles');
    expect(r.body.data.roles.map((x) => [x.name, x.sessions.length])).toEqual([['wren', 2]]);
  });

  it('#4432 NEGATIVE PROOF: a closed session is never listed inside the tile', () => {
    const rows = loggedInRoles({ data: [
      { ownedBy: 'principal-kade', sessionState: 'open', channel: 'pane', startedAt: '2026-10-06T11:00:00Z' },
      { ownedBy: 'principal-kade', sessionState: 'closed', channel: 'agent', startedAt: '2026-10-06T10:00:00Z' },
    ] }, DOOR);
    expect(rows[0].sessions).toEqual([{ channel: 'pane', startedAt: '2026-10-06T11:00:00Z', lastSeenAt: '' }]);
  });

  it('#4432 a human or unkinded row never gets an agent tile', () => {
    expect(agentRolesFrom(DOOR)).not.toContain('jeff');
    expect(agentRolesFrom(DOOR)).not.toContain('nightly');
    expect(() => agentRolesFrom({ data: [] })).toThrow(/no agent role/);
  });

  it('a role with tool calls in the window is building on its board card; lastEvent/lastActivity come from the streams', async () => {
    const events: SpineLine[] = [
      { timestamp: at(1), role: 'silas', event: 'hook.decision' },
      { timestamp: at(3), role: 'silas', event: 'context.inject.request' },
    ];
    const r = await fetchContextRoles(deps({
      readEvents: (role) => events.filter((e) => e.role === role),
      listWipCards: () => [{ id: 4058, owner: 'Silas' }],
    }), '/api/chorus/context/roles');
    const silas = r.body.data.roles.find((x) => x.name === 'silas')!;
    expect(silas.state).toBe('building');
    expect(silas.card).toBe(4058);
    expect(silas.lastEvent).toBe('hook.decision');
    expect(silas.lastActivity).toBe(at(1));
    expect(silas.stale).toBe(false);
    expect(silas.source).toBe('streams');
  });

  it('a role with no events is idle, not "unknown" — and stale is true', async () => {
    const r = await fetchContextRoles(deps(), '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.state).toBe('idle');
    expect(kade.card).toBeNull();
    expect(kade.gemba).toBeNull();
    expect(kade.stale).toBe(true);
    expect(r.body.data.roles.some((x) => x.state === 'unknown')).toBe(false);
  });

  it('AC3 negative proof: a role.state.changed event saying building, with silent streams for 20 min, is idle', async () => {
    const r = await fetchContextRoles(deps({
      readEvents: (role) => role === 'wren'
        ? [{ timestamp: at(20), role: 'wren', event: 'role.state.changed', payload: 'state=building' }]
        : [],
      listWipCards: () => [{ id: 4045, owner: 'Wren' }],
    }), '/api/chorus/context/roles');
    const wren = r.body.data.roles.find((x) => x.name === 'wren')!;
    expect(wren.state).toBe('idle');
  });

  it('blocked comes from the stream with its detail, and the row carries it', async () => {
    const r = await fetchContextRoles(deps({
      readEvents: (role) => role === 'wren'
        ? [{ timestamp: at(2), role: 'wren', event: 'role.blocked', detail: 'waiting on Jeff' }]
        : [],
    }), '/api/chorus/context/roles');
    const wren = r.body.data.roles.find((x) => x.name === 'wren')!;
    expect(wren.state).toBe('blocked');
    expect(wren.detail).toBe('waiting on Jeff');
  });

  it('consumer shape is unchanged: derived_state and drift_state still exist, drift is never divergent (nothing to drift against)', async () => {
    const r = await fetchContextRoles(deps({
      readEvents: (role) => role === 'kade' ? [{ timestamp: at(1), role: 'kade', event: 'agent.action' }] : [],
      listWipCards: () => [{ id: 4063, owner: 'Kade' }],
    }), '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.derived_state).toEqual({ state: 'building', card: 4063, wip_count: 1, recent_commit_count: null });
    expect(kade.drift_state.divergent).toBe(false);
    expect(kade.drift_state.card_inferred).toBe(4063);
  });

  it('envelope is system-scoped and the source URL passes through', async () => {
    const r = await fetchContextRoles(deps(), '/api/chorus/context/roles');
    const keys = Object.keys(JSON.parse(JSON.stringify(r.body))).sort();
    expect(keys).toEqual(['data', 'source', 'timestamp']);
    expect(r.body.source).toBe('/api/chorus/context/roles');
  });

  it('role field mirrors name so consumers can key off either', async () => {
    const r = await fetchContextRoles(deps(), '/api/chorus/context/roles');
    for (const row of r.body.data.roles) expect(row.role).toBe(row.name);
  });
});
