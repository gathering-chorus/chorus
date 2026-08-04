// @test-type: unit — injected fakes only; no Fuseki, no live session, brings its own world.
/**
 * #3743 — identity from the session WebID, authority from the Principal.
 *
 * Mark logged in with his own CSS account and the room showed him as Jeff:
 * resolveJeffMessageSender read a hand-set cookie / client field and defaulted
 * to 'jeff', never consulting the WebID the login had already verified. Worse,
 * jeff-authority (the /api/jeff-input delivery path) rode on that name string.
 *
 * These tests bring their own world: verifySession and principalFor are
 * injected fakes — no Fuseki, no live CSS, no live session.
 */

import { resolveSenderIdentity } from '../src/sender-identity';

const JEFF_WEBID = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
const MARK_WEBID = 'https://id.lightlifeurbangardens.com/marknakib/profile/card#me';

const principals: Record<string, { id: string; name: string }> = {
  [JEFF_WEBID]: { id: 'principal-jeff', name: 'jeff' },
  [MARK_WEBID]: { id: 'principal-marknakib', name: 'marknakib' },
};

function deps(webid: string | null, opts: { isLocal?: boolean } = {}) {
  return {
    isLocal: !!opts.isLocal,
    verifySession: (_cookieHeader: string) => (webid ? { webid, fresh: true } : null),
    principalFor: async (w: string) => principals[w] || null,
  };
}

describe('#3743 resolveSenderIdentity', () => {
  test('Mark\'s authenticated session shows Mark — no identify step, no jeff default', async () => {
    const id = await resolveSenderIdentity({
      cookieHeader: 'clearing_session=x',
      ...deps(MARK_WEBID),
    });
    expect(id.name).toBe('marknakib');
    expect(id.authenticated).toBe(true);
    expect(id.principal).toBe('principal-marknakib');
  });

  test('Jeff\'s authenticated session is jeff with jeff-authority', async () => {
    const id = await resolveSenderIdentity({
      cookieHeader: 'clearing_session=x',
      ...deps(JEFF_WEBID),
    });
    expect(id.name).toBe('jeff');
    expect(id.jeffAuthority).toBe(true);
  });

  test('NEGATIVE: Mark\'s session does NOT carry jeff-authority', async () => {
    const id = await resolveSenderIdentity({
      cookieHeader: 'clearing_session=x',
      ...deps(MARK_WEBID),
    });
    expect(id.jeffAuthority).toBe(false);
  });

  test('NEGATIVE: a spoofed fromField cannot override the authenticated identity', async () => {
    const id = await resolveSenderIdentity({
      cookieHeader: 'clearing_session=x',
      fromField: 'jeff',
      ...deps(MARK_WEBID),
    });
    expect(id.name).toBe('marknakib');
    expect(id.jeffAuthority).toBe(false);
  });

  test('local connection without a session keeps today\'s behaviour — fallback name, jeff-authority (machine possession)', async () => {
    const id = await resolveSenderIdentity({
      cookieHeader: '',
      ...deps(null, { isLocal: true }),
    });
    expect(id.name).toBe('jeff');
    expect(id.authenticated).toBe(false);
    expect(id.jeffAuthority).toBe(true);
  });

  test('guest fallback name survives ONLY for unauthenticated flows (bridge_name cookie)', async () => {
    const id = await resolveSenderIdentity({
      cookieHeader: 'bridge_name=Visitor',
      ...deps(null, { isLocal: true }),
    });
    expect(id.name).toBe('Visitor');
  });

  test('NEGATIVE: non-local, no session → guest with NO jeff-authority', async () => {
    const id = await resolveSenderIdentity({
      cookieHeader: 'bridge_name=Visitor',
      ...deps(null, { isLocal: false }),
    });
    expect(id.name).toBe('Visitor');
    expect(id.jeffAuthority).toBe(false);
  });

  test('NEGATIVE: an allowed WebID with no Principal mapping is not silently jeff', async () => {
    const id = await resolveSenderIdentity({
      cookieHeader: 'clearing_session=x',
      isLocal: false,
      verifySession: () => ({ webid: 'https://id.example/unknown#me', fresh: true }),
      principalFor: async () => null,
    });
    expect(id.name).not.toBe('jeff');
    expect(id.jeffAuthority).toBe(false);
  });
});
