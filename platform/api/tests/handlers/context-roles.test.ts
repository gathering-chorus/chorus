/**
 * context-roles handler tests (#2234 Step 3).
 */

import { fetchContextRoles, type ContextRolesDeps } from '../../src/handlers/context-roles';

function stubSparql(): ContextRolesDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

describe('fetchContextRoles', () => {
  it('returns all three known roles in stable order', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) => ({ role, state: 'idle' }),
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    expect(r.status).toBe(200);
    const names = r.body.data.roles.map((x) => x.name);
    expect(names).toEqual(['silas', 'wren', 'kade']);
  });

  it('merges state record + spine event per role', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) =>
        role === 'silas'
          ? { role, state: 'building', card: 2234 }
          : { role, state: 'idle' },
      tailSpine: (role) =>
        role === 'silas'
          ? { timestamp: '2026-04-19T14:15:00-04:00', role, event: 'card.demo.started' }
          : null,
    }, '/api/chorus/context/roles');
    const silas = r.body.data.roles.find((x) => x.name === 'silas')!;
    expect(silas.state).toBe('building');
    expect(silas.card).toBe(2234);
    expect(silas.lastEvent).toBe('card.demo.started');
    expect(silas.lastActivity).toBe('2026-04-19T14:15:00-04:00');
    const wren = r.body.data.roles.find((x) => x.name === 'wren')!;
    expect(wren.lastEvent).toBeNull();
  });

  it('missing state record → state "unknown", card/gemba null', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: () => null,
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.state).toBe('unknown');
    expect(kade.card).toBeNull();
    expect(kade.gemba).toBeNull();
    expect(kade.lastEvent).toBeNull();
  });

  it('envelope is system-scoped (no domain / subdomain / step / product fields)', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: () => null,
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    expect(r.body).not.toHaveProperty('domain');
    expect(r.body).not.toHaveProperty('subdomain');
    // stampHeader with null domainId returns only timestamp, so envelope has
    // only timestamp + source + data at the top level.
    const keys = Object.keys(JSON.parse(JSON.stringify(r.body))).sort();
    expect(keys).toEqual(['data', 'source', 'timestamp']);
  });

  it('source URL is passed through verbatim to the envelope', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: () => null,
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    expect(r.body.source).toBe('/api/chorus/context/roles');
  });

  it('state with gemba surfaces gemba field', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) =>
        role === 'kade'
          ? { role, state: 'observing', gemba: 'silas' }
          : { role, state: 'idle' },
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.state).toBe('observing');
    expect(kade.gemba).toBe('silas');
    expect(kade.card).toBeNull();
  });
});
