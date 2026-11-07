// @domain: awareness
// @test-type: unit — checkRoleSetup and RoleSetupWatch in platform/pulse/src/role-setup.ts over fake reads; no live services
// @card: #4919
// @owner: silas
/**
 * #4919 AC4 — pulse, which runs as system, checks every role's setup from OUTSIDE the role
 * (Wren 09:28: during the lockout the role's own hooks were the broken part, so 1 of ~20
 * replies got out). A broken setup shows on the pulse roles line and Jeff gets one line in
 * #team on each change — sent by pulse as system, never signed by the broken role.
 */
import { checkRoleSetup, RoleSetupWatch, type SetupReads } from './role-setup';

const reads = (over: Partial<SetupReads> = {}): SetupReads => ({
  principals: () => Promise.resolve([{ name: 'silas', hostAccount: 'chorus-silas' }, { name: 'wren', hostAccount: 'chorus-wren' }]),
  keyMode: role => Promise.resolve(role ? 0o600 : null),
  settingsDrift: () => Promise.resolve(false),
  ...over,
});

test('control: a sound role has no setup failures', async () => {
  expect(await checkRoleSetup('silas', reads())).toEqual([]);
});

// NEGATIVE PROOF: the 01:18 lockout — no principal runs as the role's account
test('negative proof: a principal at hostAccount=jeffbridwell is a named failure', async () => {
  const f = await checkRoleSetup('silas', reads({ principals: () => Promise.resolve([{ name: 'silas', hostAccount: 'jeffbridwell' }]) }));
  expect(f).toEqual(['identity: no principal runs as chorus-silas']);
});

// NEGATIVE PROOF: the 07:10 drift — a 660 key
test('negative proof: a key at mode 660 is a named failure', async () => {
  const f = await checkRoleSetup('silas', reads({ keyMode: () => Promise.resolve(0o660) }));
  expect(f).toEqual(['key: nostr.json mode 0660, want 0600']);
});

test('negative proof: settings edited live (differs from git) is a named failure', async () => {
  const f = await checkRoleSetup('silas', reads({ settingsDrift: () => Promise.resolve(true) }));
  expect(f).toEqual(['settings: differs from git']);
});

// A read that fails is a failure that says so, never a pass (#3734)
test('negative proof: an unreadable door or key is a failure, never a pass', async () => {
  const f = await checkRoleSetup('silas', reads({
    principals: () => Promise.reject(new Error('door 000')),
    keyMode: () => Promise.resolve(null),
  }));
  expect(f).toEqual(['identity: roles door unreadable (door 000)', 'key: nostr.json missing']);
});

describe('RoleSetupWatch', () => {
  test('posts one line on refusal and one on recovery, nothing while unchanged', async () => {
    const posted: string[] = [];
    let mode = 0o600;
    const w = new RoleSetupWatch(['silas'], reads({ keyMode: () => Promise.resolve(mode) }), line => { posted.push(line); return Promise.resolve(); });
    await w.tick();
    expect(posted).toEqual([]);
    mode = 0o660;
    await w.tick(); await w.tick();
    expect(posted).toEqual(['role setup broken (silas): key: nostr.json mode 0660, want 0600']);
    expect(w.state()).toEqual({ silas: ['key: nostr.json mode 0660, want 0600'] });
    mode = 0o600;
    await w.tick();
    expect(posted).toEqual(['role setup broken (silas): key: nostr.json mode 0660, want 0600', 'role setup ok (silas): fixed']);
    expect(w.state()).toEqual({ silas: [] });
  });

  // NEGATIVE PROOF: a post that fails is retried on the next tick, never marked as told
  test('negative proof: a failed post is retried next tick', async () => {
    const posted: string[] = [];
    let fail = true;
    const w = new RoleSetupWatch(['silas'], reads({ keyMode: () => Promise.resolve(0o660) }), line => {
      if (fail) return Promise.reject(new Error('pulse down'));
      posted.push(line); return Promise.resolve();
    });
    await w.tick();
    expect(w.postErrors.get('silas')).toBe('pulse down');
    fail = false;
    await w.tick();
    expect(w.postErrors.has('silas')).toBe(false);
    expect(posted).toEqual(['role setup broken (silas): key: nostr.json mode 0660, want 0600']);
  });
});

import { composeHeader } from './pulse-header';

// The pulse roles line shows a refused role as refused, with why (Jeff 09:21: "otherwise u
// operate in degraded state and i dont know"); a sound role stays moving/quiet.
test('the roles line shows a refused role as refused with its failures, others unchanged', async () => {
  const now = new Date('2026-11-07T14:40:00Z');
  const ago = (s: number) => new Date(now.getTime() - s * 1000).toISOString();
  const h = await composeHeader({
    cards: () => Promise.resolve([]),
    actorState: () => ({ actors: [
      { aboutPrincipal: 'principal-silas', stream: [{ timestamp: ago(5) }] },
      { aboutPrincipal: 'principal-wren', stream: [{ timestamp: ago(5) }] },
    ] }),
    messages: () => Promise.resolve([]),
    alerts: () => Promise.resolve([]),
    setup: () => ({ silas: ['key: nostr.json mode 0660, want 0600'], wren: [] }),
  }, now);
  expect(h.data.roles.rows).toEqual([
    { role: 'silas', state: 'refused', secondsSinceLastCall: 5, refused: ['key: nostr.json mode 0660, want 0600'] },
    { role: 'wren', state: 'moving', secondsSinceLastCall: 5 },
  ]);
});
