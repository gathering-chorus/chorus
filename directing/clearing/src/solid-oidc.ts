/**
 * #3669 lane 3 — the HUMAN browser login for the Clearing (Solid-OIDC auth-code
 * + PKCE against CSS).
 *
 * Decided with Wren (ADR-052 §9): the browser flow is NOT DPoP. A human logs in
 * at CSS (auth-code + PKCE), the Clearing exchanges the code for the WebID, checks
 * it against the seam allow-set (reusing solid-auth's isWebIdAllowed), and issues a
 * long-lived SIGNED SESSION COOKIE. That cookie IS the human auth — the parallel
 * to the agent DPoP path, not a copy of it. The browser never touches DPoP.
 *
 * Security properties baked in here (the pure, testable core):
 *   • PKCE S256 — no client secret (public client), code interception is useless.
 *   • state — CSRF: minted at redirect, echoed by CSS, verified on callback.
 *   • signed cookies — HMAC-SHA256 over a server secret; the login cookie (verifier
 *     + state + returnPath) and the session cookie (WebID) are tamper-evident. The
 *     login cookie is a COOKIE, not server memory, so a Clearing restart mid-login
 *     doesn't strand the user (Wren's note 2).
 *   • return-path is open-redirect-guarded — must be a local absolute path, never
 *     a scheme or protocol-relative //host that could bounce the user off-site.
 */

import crypto from 'crypto';

const b64url = (b: Buffer): string =>
  b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

// --- PKCE (S256) -------------------------------------------------------------

export function makePkce(): { verifier: string; challenge: string } {
  const verifier = b64url(crypto.randomBytes(32)); // 43 chars, RFC 7636 range
  const challenge = b64url(crypto.createHash('sha256').update(verifier).digest());
  return { verifier, challenge };
}

export function makeState(): string {
  return b64url(crypto.randomBytes(16));
}

// --- signed cookies (tamper-evident, no server-side session store) -----------

/** `<b64url(json)>.<b64url(hmac)>` — verifiable without a store, tamper-evident. */
export function signCookie(payload: unknown, secret: string): string {
  const body = b64url(Buffer.from(JSON.stringify(payload)));
  const mac = b64url(crypto.createHmac('sha256', secret).update(body).digest());
  return `${body}.${mac}`;
}

/**
 * Verify + parse a signed cookie; null on any tamper/format/HMAC mismatch.
 * `expectedTyp` (Wren fix 2) closes the login↔session confusion class: both
 * cookies are `body.mac` under the same secret, so a valid-signature login cookie
 * could be replayed where a session cookie is read. Requiring `payload.typ ===
 * expectedTyp` rejects the cross-use with one field instead of relying on caller
 * field-discipline.
 */
export function verifyCookie<T = unknown>(
  cookie: string | undefined,
  secret: string,
  expectedTyp?: 'login' | 'session',
): T | null {
  if (!cookie || !cookie.includes('.')) return null;
  const [body, mac] = cookie.split('.', 2);
  const expected = b64url(crypto.createHmac('sha256', secret).update(body).digest());
  // constant-time compare — equal-length buffers required
  const a = Buffer.from(mac);
  const b = Buffer.from(expected);
  if (a.length !== b.length || !crypto.timingSafeEqual(a, b)) return null;
  try {
    const payload = JSON.parse(
      Buffer.from(body.replace(/-/g, '+').replace(/_/g, '/'), 'base64').toString(),
    );
    if (expectedTyp && payload?.typ !== expectedTyp) return null;
    return payload as T;
  } catch {
    return null;
  }
}

// --- open-redirect guard on the "where was the user heading" path ------------

/**
 * A safe post-login redirect target: a LOCAL absolute path only. Rejects schemes
 * (`https://evil`), protocol-relative (`//evil`), and non-absolute inputs — any of
 * which could bounce the just-authenticated user off-site. Falls back to '/'.
 */
export function safeReturnPath(raw: string | undefined): string {
  if (!raw || typeof raw !== 'string') return '/';
  if (!raw.startsWith('/') || raw.startsWith('//') || raw.startsWith('/\\')) return '/';
  return raw;
}

// --- the CSS authorization URL ----------------------------------------------

export interface OidcConfig {
  issuer: string; // browser-facing issuer, e.g. https://id.lightlifeurbangardens.com
  clientId: string; // the /clientid.jsonld URL
  redirectUri: string; // .../auth/callback
  scope: string; // "openid webid offline_access"
  // #3669 hardening 3 (Wren) — the SERVER-SIDE token exchange must NOT hairpin the
  // public issuer through Cloudflare (bot-check 1010; and LAN login would die with
  // the internet). It targets CSS locally with the Host-override trick; the issuer
  // stays browser-facing only. tokenEndpoint defaults to issuer/.oidc/token.
  tokenEndpoint?: string; // e.g. http://localhost:3001/.oidc/token
  tokenHost?: string; // Host header for the local exchange, e.g. id.lightlifeurbangardens.com
}

export function buildAuthUrl(cfg: OidcConfig, state: string, codeChallenge: string): string {
  const u = new URL('/.oidc/auth', cfg.issuer);
  u.searchParams.set('response_type', 'code');
  u.searchParams.set('client_id', cfg.clientId);
  u.searchParams.set('redirect_uri', cfg.redirectUri);
  u.searchParams.set('scope', cfg.scope);
  u.searchParams.set('state', state);
  u.searchParams.set('code_challenge', codeChallenge);
  u.searchParams.set('code_challenge_method', 'S256');
  // #3669 fix 1 (Wren): NO prompt=consent — that forces the consent screen on
  // every login. CSS's own 30-day session makes returning logins silent, which is
  // the point of the long-lived cookie. Consent shows once, at first authorize.
  return u.toString();
}

// --- code → WebID exchange ---------------------------------------------------

/**
 * Exchange the auth code for an ID token and return its WebID. Public client
 * (no secret), PKCE verifier proves the exchange. The token endpoint is the
 * issuer's /.oidc/token. Returns null on any failure (never throws).
 */
export async function exchangeCodeForWebId(
  cfg: OidcConfig,
  code: string,
  verifier: string,
  fetchImpl: typeof fetch = fetch,
): Promise<string | null> {
  try {
    const body = new URLSearchParams({
      grant_type: 'authorization_code',
      code,
      redirect_uri: cfg.redirectUri,
      client_id: cfg.clientId,
      code_verifier: verifier,
    });
    // #3669 hardening 3 — exchange LOCALLY (no Cloudflare hairpin / bot-check /
    // internet dependency). Target the local token endpoint with the Host-override
    // so CSS matches its baseUrl; fall back to the public issuer only if no local
    // endpoint is configured (e.g. tests / a future off-box deploy).
    const endpoint = cfg.tokenEndpoint || new URL('/.oidc/token', cfg.issuer).toString();
    const headers: Record<string, string> = { 'content-type': 'application/x-www-form-urlencoded' };
    if (cfg.tokenHost) {
      headers['Host'] = cfg.tokenHost;
      headers['X-Forwarded-Proto'] = 'https';
      headers['X-Forwarded-Host'] = cfg.tokenHost;
    }
    const res = await fetchImpl(endpoint, { method: 'POST', headers, body: body.toString() });
    if (!res.ok) return null;
    const tok = (await res.json()) as { id_token?: string; access_token?: string };
    const jwt = tok.id_token || tok.access_token;
    // #3669 hardening 3 — with skip-verify weakened to loopback, pin the issuer:
    // the token's iss MUST equal our configured issuer, else reject.
    return jwt ? webIdFromJwt(jwt, cfg.issuer) : null;
  } catch {
    return null;
  }
}

/** Pull the `webid` (or `sub` fallback) claim from a JWT. Signature is not
 *  re-verified — the token came straight from CSS on our own loopback POST, and
 *  the authoritative check is allow-set membership. When `expectedIssuer` is given
 *  (hardening 3), the token's `iss` claim MUST match it or we return null. */
export function webIdFromJwt(jwt: string, expectedIssuer?: string): string | null {
  const parts = jwt.split('.');
  if (parts.length < 2) return null;
  try {
    const payload = JSON.parse(
      Buffer.from(parts[1].replace(/-/g, '+').replace(/_/g, '/'), 'base64').toString(),
    );
    if (expectedIssuer) {
      const iss = String(payload.iss || '').replace(/\/$/, '');
      if (iss !== expectedIssuer.replace(/\/$/, '')) return null;
    }
    return payload.webid || payload.sub || null;
  } catch {
    return null;
  }
}
