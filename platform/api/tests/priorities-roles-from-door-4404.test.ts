// @test-type: unit — injects the roles seam; no live service.
// @domain: cards
// #4404 reopen 1 — /sup accepts the roles the roles door serves, not a
// hard-coded list. abby-normal got 400 on 2026-10-13 11:51 (Silas measured).
import { fetchContextPriorities, roleNames } from '../src/handlers/context-priorities';

const URL_ = '/api/chorus/context/priorities';
const deps = (listRoles: () => Promise<string[]>) => ({
  sparql: { query: async () => ({ results: { bindings: [] } }) } as never,
  readPulse: () => null,
  owl: async () => ({ data: [] }),
  listRoles,
});

describe('#4404 reopen 1 — roles come from the roles door', () => {
  it('a role the door lists is served, abby-normal included', async () => {
    const r = await fetchContextPriorities(deps(async () => ['wren', 'abby-normal']), URL_, 'abby-normal');
    expect(r.status).not.toBe(400);
  });

  // NEGATIVE PROOF: a name the door does not list is refused, naming the door's roles.
  it('a role the door does not list is refused with the door list named', async () => {
    const r = await fetchContextPriorities(deps(async () => ['wren', 'abby-normal']), URL_, 'kade');
    expect(r.status).toBe(400);
    expect((r.body as { error: string }).error).toContain('the roles door lists wren/abby-normal');
  });

  it('the roles door down is a 503 that says so, never a stale list', async () => {
    const r = await fetchContextPriorities(deps(async () => { throw new Error('roles door 502'); }), URL_, 'wren');
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toContain('roles door');
  });

  it('roleNames reads names from the door rows', () => {
    expect(roleNames({ items: [{ name: 'Wren' }, { name: 'abby-normal' }, {}] })).toEqual(['wren', 'abby-normal']);
    expect(roleNames(null)).toEqual([]);
  });
});
