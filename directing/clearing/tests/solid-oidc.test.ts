// @test-type: security
/**
 * #3669 lane 3 — the human browser login core. These pin the security-critical
 * pure pieces: PKCE S256, tamper-evident signed cookies, and the open-redirect
 * guard on the post-login return path. The token exchange is covered with a
 * mocked fetch (deterministic, no live CSS).
 */

import {
  makePkce, makeState, signCookie, verifyCookie, safeReturnPath,
  buildAuthUrl, exchangeCodeForWebId, webIdFromJwt,
} from '../src/solid-oidc';

const CFG = {
  issuer: 'https://id.lightlifeurbangardens.com',
  clientId: 'https://clearing.lightlifeurbangardens.com/clientid.jsonld',
  redirectUri: 'https://clearing.lightlifeurbangardens.com/auth/callback',
  scope: 'openid webid offline_access',
};
const WREN = 'https://id.lightlifeurbangardens.com/wren/profile/card#me';

describe('#3669 PKCE', () => {
  test('verifier is RFC-7636-length and challenge is its S256 digest', () => {
    const { verifier, challenge } = makePkce();
    expect(verifier.length).toBeGreaterThanOrEqual(43);
    expect(verifier).toMatch(/^[A-Za-z0-9\-_]+$/); // base64url, no padding
    const crypto = require('crypto');
    const expected = crypto.createHash('sha256').update(verifier).digest('base64')
      .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    expect(challenge).toBe(expected);
  });
  test('each call is unique', () => {
    expect(makePkce().verifier).not.toBe(makePkce().verifier);
    expect(makeState()).not.toBe(makeState());
  });
});

describe('#3669 signed cookies — tamper-evident', () => {
  const SECRET = 'test-secret-abc';
  test('round-trips a payload', () => {
    const c = signCookie({ v: 'verifier', s: 'state', p: '/room' }, SECRET);
    expect(verifyCookie(c, SECRET)).toEqual({ v: 'verifier', s: 'state', p: '/room' });
  });
  test('typ discrimination: a login cookie is refused where a session is required (Wren fix 2)', () => {
    const login = signCookie({ typ: 'login', v: 'x' }, SECRET);
    const session = signCookie({ typ: 'session', webid: WREN }, SECRET);
    expect(verifyCookie(login, SECRET, 'session')).toBeNull();     // replay refused
    expect(verifyCookie(session, SECRET, 'login')).toBeNull();     // and the reverse
    expect(verifyCookie(session, SECRET, 'session')).toEqual({ typ: 'session', webid: WREN });
    expect(verifyCookie(login, SECRET, 'login')).toEqual({ typ: 'login', v: 'x' });
  });
  test('a tampered body is rejected', () => {
    const c = signCookie({ webid: WREN }, SECRET);
    const [body, mac] = c.split('.');
    const forged = `${Buffer.from(JSON.stringify({ webid: 'https://evil/me' })).toString('base64url')}.${mac}`;
    expect(verifyCookie(forged, SECRET)).toBeNull();
    expect(body).toBeTruthy();
  });
  test('a wrong secret is rejected', () => {
    const c = signCookie({ webid: WREN }, SECRET);
    expect(verifyCookie(c, 'other-secret')).toBeNull();
  });
  test('malformed input is null, never throws', () => {
    expect(verifyCookie(undefined, SECRET)).toBeNull();
    expect(verifyCookie('nodot', SECRET)).toBeNull();
    expect(verifyCookie('a.b.c', SECRET)).toBeNull();
  });
});

describe('#3669 safeReturnPath — open-redirect guard', () => {
  test('a local absolute path passes', () => {
    expect(safeReturnPath('/api/chat')).toBe('/api/chat');
    expect(safeReturnPath('/')).toBe('/');
  });
  test('off-site targets are refused → /', () => {
    expect(safeReturnPath('https://evil.com')).toBe('/');
    expect(safeReturnPath('//evil.com')).toBe('/');       // protocol-relative
    expect(safeReturnPath('/\\evil.com')).toBe('/');       // backslash trick
    expect(safeReturnPath('javascript:alert(1)')).toBe('/');
    expect(safeReturnPath('')).toBe('/');
    expect(safeReturnPath(undefined)).toBe('/');
  });
});

describe('#3669 buildAuthUrl', () => {
  test('targets CSS /.oidc/auth with PKCE S256 + all params', () => {
    const url = new URL(buildAuthUrl(CFG, 'state123', 'chal456'));
    expect(url.origin + url.pathname).toBe('https://id.lightlifeurbangardens.com/.oidc/auth');
    expect(url.searchParams.get('response_type')).toBe('code');
    expect(url.searchParams.get('client_id')).toBe(CFG.clientId);
    expect(url.searchParams.get('redirect_uri')).toBe(CFG.redirectUri);
    expect(url.searchParams.get('code_challenge')).toBe('chal456');
    expect(url.searchParams.get('code_challenge_method')).toBe('S256');
    expect(url.searchParams.get('state')).toBe('state123');
  });
  test('does NOT force prompt=consent (Wren fix 1 — no daily consent ceremony)', () => {
    const url = new URL(buildAuthUrl(CFG, 's', 'c'));
    expect(url.searchParams.get('prompt')).toBeNull();
  });
});

describe('#3669 webIdFromJwt', () => {
  const mkJwt = (claims: object) =>
    `h.${Buffer.from(JSON.stringify(claims)).toString('base64url')}.sig`;
  test('prefers the webid claim', () => {
    expect(webIdFromJwt(mkJwt({ webid: WREN, sub: 'x' }))).toBe(WREN);
  });
  test('falls back to sub', () => {
    expect(webIdFromJwt(mkJwt({ sub: WREN }))).toBe(WREN);
  });
  test('garbage is null, never throws', () => {
    expect(webIdFromJwt('not-a-jwt')).toBeNull();
    expect(webIdFromJwt('a.!!!.c')).toBeNull();
  });
  test('issuer pin (hardening 3): a token from the wrong iss is refused', () => {
    const good = mkJwt({ webid: WREN, iss: 'https://id.lightlifeurbangardens.com/' });
    const evil = mkJwt({ webid: WREN, iss: 'https://evil-issuer.example/' });
    expect(webIdFromJwt(good, 'https://id.lightlifeurbangardens.com')).toBe(WREN);
    expect(webIdFromJwt(evil, 'https://id.lightlifeurbangardens.com')).toBeNull();
  });
});

describe('#3669 exchangeCodeForWebId', () => {
  const okFetch = (idToken: string): typeof fetch =>
    (async () => ({ ok: true, json: async () => ({ id_token: idToken }) })) as unknown as typeof fetch;
  test('returns the WebID from the exchanged token (iss matches)', async () => {
    const jwt = `h.${Buffer.from(JSON.stringify({ webid: WREN, iss: CFG.issuer })).toString('base64url')}.s`;
    expect(await exchangeCodeForWebId(CFG, 'code', 'verifier', okFetch(jwt))).toBe(WREN);
  });
  test('a non-200 token response → null (never throws)', async () => {
    const bad = (async () => ({ ok: false, json: async () => ({}) })) as unknown as typeof fetch;
    expect(await exchangeCodeForWebId(CFG, 'code', 'verifier', bad)).toBeNull();
  });
  test('a network throw → null', async () => {
    const boom = (async () => { throw new Error('net'); }) as unknown as typeof fetch;
    expect(await exchangeCodeForWebId(CFG, 'code', 'verifier', boom)).toBeNull();
  });
});
