/**
 * #3775 — verify the guard's session cookie (DEC-2209 clause 2).
 *
 * The common door (chorus.lightlifeurbangardens.com) mints `chorus_share_session`
 * scoped to the parent domain, so it reaches this subdomain. This module is the
 * Clearing's half of the contract Silas published 2026-08-07:
 *
 *   cookie  = <payload>.<mac>
 *   payload = base64url(JSON {"webid": string, "exp": unix-seconds}), unpadded
 *   mac     = base64url(HMAC-SHA256(key, PAYLOAD_STRING)), unpadded
 *
 * The MAC covers the base64url payload STRING, not the raw JSON bytes — signing
 * the bytes yields a verifier that rejects every valid cookie and looks like a
 * key problem while being a spec problem.
 *
 * HONEST GAP (clause 2, stated not hidden): this is a SECOND implementation of
 * one contract — the guard verifies in Python, this verifies in TypeScript, and
 * clause 2 asks for one shared verifier. #3685's crate subsumes both when it
 * lands. Until then the drift guard is the cross-implementation fixture
 * (tests/fixtures/share-session-vectors.json): one artifact, both verifiers,
 * same verdicts, so a divergence is a red test rather than an outage.
 *
 * WHAT THE COOKIE DOES NOT CARRY: no role, no scope, no grant. A session is a
 * signed statement of WHO. Authorization is asked separately, per request —
 * which is what makes revocation real (an allow-set removal bites on the next
 * click, not at the next sign-in).
 */
import crypto from 'crypto';
import fs from 'fs';
import path from 'path';

export const SHARE_COOKIE_NAME = 'chorus_share_session';

/** Guard state file — same default as the guard, override for tests. */
const STATE_FILE = process.env.SHARE_STATE_FILE
  || path.join(process.env.HOME || '', '.chorus', 'share-oidc.json');

export type SessionVerdict =
  | { ok: true; webid: string; exp: number }
  /** malformed / bad signature / expired — this is NOBODY, not a denied person.
   *  Callers send these to the door (302), never to a 403 (Silas, #3785 rationale). */
  | { ok: false; reason: 'malformed' | 'bad-signature' | 'expired' | 'no-key' };

function b64urlDecode(s: string): Buffer {
  return Buffer.from(s.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
}

function b64urlEncode(b: Buffer): string {
  return b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/** Raw HMAC key bytes from the guard's state file. Null when unreadable —
 *  which fails every session CLOSED: losing the key must mean signed out, never
 *  trusted-by-default (DEC-2209 clause 5). */
let keyWarned = false;
/** Test seam — the warn-once latch is real behaviour (one line per outage, not
 *  per request), so a test that wants to observe the warning must clear it. */
export function _resetKeyWarn(): void { keyWarned = false; }
export function readCookieKey(stateFile: string = STATE_FILE): Buffer | null {
  try {
    /* The path is ours and never a request's: it defaults to STATE_FILE, a module
       constant, and is overridden only by tests pointing at a tmp file. This becomes
       unsafe the day a caller passes something derived from a header, query or body —
       if you are here because you want to do that, read the constant instead. */
    // eslint-disable-next-line security/detect-non-literal-fs-filename
    const raw = JSON.parse(fs.readFileSync(stateFile, 'utf-8'));
    if (typeof raw.cookie_key !== 'string' || !raw.cookie_key) {
      if (!keyWarned) {
        keyWarned = true;
        console.error(`share-session: state file <${stateFile}> has no cookie_key — EVERY guard session will fail closed. This is not "the cookie never arrived".`);
      }
      return null;
    }
    keyWarned = false;
    return b64urlDecode(raw.cookie_key);
  } catch (e) {
    // #3775 (Silas's ask): say WHICH path failed. An unreadable key refuses every
    // guard session, which from outside is indistinguishable from the guard not
    // sending a cookie at all — and sends whoever debugs it to the wrong half of
    // the system. Warn once per outage, not per request.
    if (!keyWarned) {
      keyWarned = true;
      console.error(`share-session: cannot read cookie key at <${stateFile}> (${(e as Error).message}) — EVERY guard session fails closed until this is readable. Look here, not at the guard.`);
    }
    return null;
  }
}

/**
 * Verify signature + expiry. Does NOT consult the allow-set: who-you-are and
 * may-you-enter are separate questions asked separately, every request.
 * `now` is injectable so the expiry legs are provable without sleeping.
 */
export function verifyShareSession(
  cookie: string | undefined,
  key: Buffer | null,
  now: number = Math.floor(Date.now() / 1000),
): SessionVerdict {
  if (!key) return { ok: false, reason: 'no-key' };
  if (!cookie || typeof cookie !== 'string') return { ok: false, reason: 'malformed' };

  // 1. split on the LAST '.' — base64url never emits one, but be strict anyway.
  const cut = cookie.lastIndexOf('.');
  if (cut <= 0 || cut === cookie.length - 1) return { ok: false, reason: 'malformed' };
  const payloadStr = cookie.slice(0, cut);
  const macStr = cookie.slice(cut + 1);

  // 2. recompute over the PAYLOAD STRING, compare in constant time. Never ===:
  //    a timing oracle leaks a valid signature byte by byte.
  const expected = b64urlEncode(crypto.createHmac('sha256', key).update(payloadStr).digest());
  const a = Buffer.from(macStr);
  const b = Buffer.from(expected);
  if (a.length !== b.length || !crypto.timingSafeEqual(a, b)) {
    return { ok: false, reason: 'bad-signature' };
  }

  // 3. decode + expiry. Absent or unparseable exp REFUSES — a session with no
  //    stated end is not a long session, it is an unverifiable claim.
  let claims: { webid?: unknown; exp?: unknown };
  try {
    claims = JSON.parse(b64urlDecode(payloadStr).toString('utf-8'));
  } catch {
    return { ok: false, reason: 'malformed' };
  }
  if (typeof claims.webid !== 'string' || !claims.webid) return { ok: false, reason: 'malformed' };
  if (typeof claims.exp !== 'number' || !Number.isFinite(claims.exp)) return { ok: false, reason: 'malformed' };
  if (claims.exp <= now) return { ok: false, reason: 'expired' };

  // 4. the allow-set check is the CALLER's next step (isWebIdAllowed), per
  //    request — deliberately not folded in here so the two questions stay two.
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
