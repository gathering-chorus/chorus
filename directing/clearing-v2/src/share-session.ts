// @domain: messages
/**
 * #3775 — verify the guard's session cookie (DEC-2209 clause 2).
 *
 * The common door (chorus.lightlifeurbangardens.com) mints `chorus_share_session`
 * scoped to the parent domain, so it reaches this host.
 *
 *   cookie  = <payload>.<mac>
 *   payload = base64url(JSON {"webid": string, "exp": unix-seconds}), unpadded
 *   mac     = base64url(HMAC-SHA256(key, PAYLOAD_STRING)), unpadded
 */
import crypto from 'crypto';
import fs from 'fs';
import path from 'path';

export const SHARE_COOKIE_NAME = 'chorus_share_session';

/** Guard state file — same default as the guard, override for tests. */
const STATE_FILE = process.env.SHARE_STATE_FILE
  || path.join(process.env.HOME || '', '.chorus', 'share-oidc.json');

export type SessionVerdict =
  | { ok: true; webid: string; exp: number; sid?: string }
  | { ok: false; reason: 'malformed' | 'bad-signature' | 'expired' | 'no-key' };

function b64urlDecode(s: string): Buffer {
  return Buffer.from(s.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
}

function b64urlEncode(b: Buffer): string {
  return b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

let keyWarned = false;
export function _resetKeyWarn(): void { keyWarned = false; }

export function readCookieKey(stateFile: string = STATE_FILE): Buffer | null {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- stateFile is STATE_FILE (SHARE_STATE_FILE / ~/.chorus) or a test override, never request input
    const raw = JSON.parse(fs.readFileSync(stateFile, 'utf-8'));
    if (typeof raw.cookie_key !== 'string' || !raw.cookie_key) {
      if (!keyWarned) {
        keyWarned = true;
        console.error(`share-session: state file <${stateFile}> has no cookie_key — EVERY guard session will fail closed.`);
      }
      return null;
    }
    keyWarned = false;
    return b64urlDecode(raw.cookie_key);
  } catch (e) {
    if (!keyWarned) {
      keyWarned = true;
      console.error(`share-session: cannot read cookie key at <${stateFile}> (${(e as Error).message}) — EVERY guard session fails closed until this is readable.`);
    }
    return null;
  }
}

/**
 * Verify signature + expiry. Does NOT consult the allow-set.
 */
export function verifyShareSession(
  cookie: string | undefined,
  key: Buffer | null,
  now: number = Math.floor(Date.now() / 1000),
): SessionVerdict {
  if (!key) return { ok: false, reason: 'no-key' };
  if (!cookie || typeof cookie !== 'string') return { ok: false, reason: 'malformed' };

  const cut = cookie.lastIndexOf('.');
  if (cut <= 0 || cut === cookie.length - 1) return { ok: false, reason: 'malformed' };
  const payloadStr = cookie.slice(0, cut);
  const macStr = cookie.slice(cut + 1);

  const expected = b64urlEncode(crypto.createHmac('sha256', key).update(payloadStr).digest());
  const a = Buffer.from(macStr);
  const b = Buffer.from(expected);
  if (a.length !== b.length || !crypto.timingSafeEqual(a, b)) {
    return { ok: false, reason: 'bad-signature' };
  }

  let claims: { webid?: unknown; exp?: unknown; sid?: unknown };
  try {
    claims = JSON.parse(b64urlDecode(payloadStr).toString('utf-8'));
  } catch {
    return { ok: false, reason: 'malformed' };
  }
  if (typeof claims.webid !== 'string' || !claims.webid) return { ok: false, reason: 'malformed' };
  if (typeof claims.exp !== 'number' || !Number.isFinite(claims.exp)) return { ok: false, reason: 'malformed' };
  if (claims.exp <= now) return { ok: false, reason: 'expired' };

  return { ok: true, webid: claims.webid, exp: claims.exp };
}

/** Pull the cookie out of a Cookie header without a parser dependency. */
export function shareSessionFromHeader(cookieHeader: string | undefined): string | undefined {
  if (!cookieHeader) return undefined;
  for (const part of cookieHeader.split(';')) {
    const [k, ...rest] = part.trim().split('=');
    if (k === SHARE_COOKIE_NAME) return rest.join('=');
  }
  return undefined;
}

/**
 * Create a signed share session cookie (useful for testing and verification).
 */
export function mintShareSessionCookie(webid: string, key: Buffer, expSecs: number = Math.floor(Date.now() / 1000) + 3600): string {
  const payload = b64urlEncode(Buffer.from(JSON.stringify({ webid, exp: expSecs }), 'utf-8'));
  const mac = b64urlEncode(crypto.createHmac('sha256', key).update(payload).digest());
  return `${payload}.${mac}`;
}
