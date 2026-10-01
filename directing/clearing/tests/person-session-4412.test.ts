// @test-type: unit — fake fetch and a temp store file; no CSS, no athena-make.
// @card: #4412
// @owner: wren
/**
 * #4412 — Jeff's Clearing sign-in writes his browser Session, as him; activity
 * keeps it current; sign-out closes it. Role rows are never written here.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  openPersonSession, touchPersonSession, closePersonSession, getRecord,
  openRoleSessionNames, type PersonSessionDeps, type SignInClaims,
} from '../src/person-session';

type Call = { method: string; url: string; auth?: string; body?: Record<string, unknown> };

const LIST = {
  items: [
    { name: 'kade-a-1', actsAs: 'role-kade', sessionState: 'open' },
    { name: 'silas-b-2', actsAs: 'role-silas', sessionState: 'open' },
    { name: 'wren-c-3', actsAs: 'role-wren', sessionState: 'closed' },
    { name: 'jeff-browser-old', actsAs: 'jeff', sessionState: 'open' },
  ],
};

function jwt(payload: Record<string, unknown>): string {
  return `h.${Buffer.from(JSON.stringify(payload)).toString('base64url')}.s`;
}

function world(opts: { postStatus?: number; putStatus?: number; listDown?: boolean; refreshed?: SignInClaims | null } = {}) {
  const calls: Call[] = [];
  let t = Date.parse('2026-09-30T18:00:00Z');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ps-4412-'));
  const fetchImpl = (async (url: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET';
    const headers = (init?.headers ?? {}) as Record<string, string>;
    calls.push({ method, url, auth: headers.Authorization, body: init?.body ? JSON.parse(String(init.body)) : undefined });
    if (method === 'GET') {
      if (opts.listDown) throw new Error('connect ECONNREFUSED');
      return { ok: true, status: 200, json: async () => LIST } as Response;
    }
    const status = method === 'POST' ? opts.postStatus ?? 201 : opts.putStatus ?? 200;
    return {
      ok: status < 300, status,
      json: async () => ({ data: { iri: 'https://jeffbridwell.com/chorus#session-jeff-browser-stored' } }),
    } as Response;
  }) as unknown as typeof fetch;
  const deps: PersonSessionDeps = {
    api: 'http://api', fetchImpl, now: () => t,
    storePath: path.join(dir, 'store.json'),
    refresh: async () => opts.refreshed ?? null,
  };
  const claims: SignInClaims = {
    webid: 'https://id.example/jeff/profile/card#me',
    idToken: jwt({ jti: 'tok-1', iat: t / 1000, exp: t / 1000 + 3600 }),
    refreshToken: 'refresh-1',
    iat: t / 1000,
    exp: t / 1000 + 3600,
  };
  return { calls, deps, claims, dir, advance: (ms: number) => { t += ms; } };
}

describe('#4412 — his Clearing sign-in writes his browser Session', () => {
  it('writes one open browser row as him, binding only the open role sessions', async () => {
    const w = world();
    const key = await openPersonSession(w.deps, 'jeff', w.claims);
    expect(key).toBeTruthy();
    const post = w.calls.filter((c) => c.method === 'POST');
    expect(post).toHaveLength(1);
    expect(post[0].url).toBe('http://api/v1/identity/sessions');
    expect(post[0].auth).toBe(`Bearer ${w.claims.idToken}`);
    expect(post[0].body).toMatchObject({ ownedBy: 'jeff', actsAs: 'jeff', channel: 'browser', sessionState: 'open', tokenId: 'tok-1' });
    // NEGATIVE PROOF: a closed role session and another person's row are not bound
    expect(post[0].body?.binds).toEqual(['kade-a-1', 'silas-b-2']);
    // role rows are never written by the person
    expect(w.calls.filter((c) => c.method === 'PUT')).toHaveLength(0);
    // the row never carries the token itself
    expect(JSON.stringify(post[0].body)).not.toContain(w.claims.idToken);
    expect(getRecord(w.deps, key!)?.rowName).toBe('jeff-browser-stored');
  });

  it('keeps the store readable by this account only', async () => {
    const w = world();
    await openPersonSession(w.deps, 'jeff', w.claims);
    expect(fs.statSync(w.deps.storePath).mode & 0o777).toBe(0o600);
  });

  it('NEGATIVE PROOF — a refused write stores nothing and returns no key', async () => {
    const w = world({ postStatus: 403 });
    expect(await openPersonSession(w.deps, 'jeff', w.claims)).toBeNull();
    expect(fs.existsSync(w.deps.storePath)).toBe(false);
  });

  it('with the list unreadable the row still goes out, binding nothing', async () => {
    const w = world({ listDown: true });
    expect(await openPersonSession(w.deps, 'jeff', w.claims)).toBeTruthy();
    expect(w.calls.find((c) => c.method === 'POST')?.body?.binds).toBeUndefined();
  });
});

describe('#4412 — activity keeps lastSeenAt current, at most once a minute', () => {
  it('writes nothing inside the minute, then one PUT with the new lastSeenAt', async () => {
    const w = world();
    const key = (await openPersonSession(w.deps, 'jeff', w.claims))!;
    w.advance(30_000);
    await touchPersonSession(w.deps, key);
    expect(w.calls.filter((c) => c.method === 'PUT')).toHaveLength(0);
    w.advance(40_000);
    await touchPersonSession(w.deps, key);
    const puts = w.calls.filter((c) => c.method === 'PUT');
    expect(puts).toHaveLength(1);
    expect(puts[0].url).toBe('http://api/v1/identity/sessions/jeff-browser-stored');
    expect(puts[0].body?.lastSeenAt).toBe('2026-09-30T18:01:10Z');
    expect(puts[0].body?.sessionState).toBe('open');
  });

  it('an expired token is refreshed before the write', async () => {
    const fresh = jwt({ jti: 'tok-2' });
    const w = world({ refreshed: { webid: 'x', idToken: fresh, iat: 0, exp: Date.parse('2026-09-30T21:00:00Z') / 1000 } });
    const key = (await openPersonSession(w.deps, 'jeff', w.claims))!;
    w.advance(2 * 3600_000);
    expect(await touchPersonSession(w.deps, key)).toBe(true);
    expect(w.calls.filter((c) => c.method === 'PUT')[0].auth).toBe(`Bearer ${fresh}`);
  });

  it('NEGATIVE PROOF — an expired token that cannot be refreshed writes nothing', async () => {
    const w = world({ refreshed: null });
    const key = (await openPersonSession(w.deps, 'jeff', w.claims))!;
    w.advance(2 * 3600_000);
    expect(await touchPersonSession(w.deps, key)).toBe(false);
    expect(w.calls.filter((c) => c.method === 'PUT')).toHaveLength(0);
  });
});

describe('#4412 — the row lasts as long as his sign-in, and binds follow the roles', () => {
  it('expiresAt is the sign-in life (30 days), never the hour-long token', async () => {
    const w = world();
    await openPersonSession(w.deps, 'jeff', w.claims);
    const body = w.calls.find((c) => c.method === 'POST')!.body!;
    expect(body.expiresAt).toBe('2026-10-30T18:00:00Z');
    // NEGATIVE PROOF: not the token's exp (18:00 + 1h)
    expect(body.expiresAt).not.toBe('2026-09-30T19:00:00Z');
  });

  it('a role that logs in after he signed in is bound at the next activity', async () => {
    const w = world();
    const key = (await openPersonSession(w.deps, 'jeff', w.claims))!;
    LIST.items.push({ name: 'wren-d-4', actsAs: 'role-wren', sessionState: 'open' });
    try {
      w.advance(61_000);
      await touchPersonSession(w.deps, key);
      const put = w.calls.filter((c) => c.method === 'PUT')[0];
      expect(put.body?.binds).toEqual(['kade-a-1', 'silas-b-2', 'wren-d-4']);
    } finally { LIST.items.pop(); }
  });
});

describe('#4412 — sign-out closes his row', () => {
  it('PUTs the row closed with endedAt, and forgets the tokens', async () => {
    const w = world();
    const key = (await openPersonSession(w.deps, 'jeff', w.claims))!;
    w.advance(5_000);
    expect(await closePersonSession(w.deps, key)).toBe(true);
    const put = w.calls.filter((c) => c.method === 'PUT')[0];
    expect(put.body).toMatchObject({ sessionState: 'closed', endedAt: '2026-09-30T18:00:05Z', binds: ['kade-a-1', 'silas-b-2'] });
    expect(getRecord(w.deps, key)).toBeNull();
    expect(fs.readFileSync(w.deps.storePath, 'utf-8')).not.toContain('refresh-1');
  });

  it('NEGATIVE PROOF — a refused close still forgets the tokens, and says it failed', async () => {
    const w = world({ putStatus: 502 });
    const key = (await openPersonSession(w.deps, 'jeff', w.claims))!;
    expect(await closePersonSession(w.deps, key)).toBe(false);
    expect(getRecord(w.deps, key)).toBeNull();
  });

  it('an unknown key closes nothing', async () => {
    const w = world();
    expect(await closePersonSession(w.deps, 'nope')).toBe(false);
    expect(w.calls).toHaveLength(0);
  });
});

describe('openRoleSessionNames', () => {
  it('reads a list in any of the API shapes and keeps open role rows only', () => {
    expect(openRoleSessionNames(LIST)).toEqual(['kade-a-1', 'silas-b-2']);
    expect(openRoleSessionNames({ data: LIST.items })).toEqual(['kade-a-1', 'silas-b-2']);
    expect(openRoleSessionNames(null)).toEqual([]);
  });
});
