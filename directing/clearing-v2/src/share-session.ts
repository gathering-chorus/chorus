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
import type { Request, Response, NextFunction } from 'express';
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

  return { ok: true, webid: claims.webid, exp: claims.exp, ...sidOf(claims.sid) };
}

/** #4500 — the guard's session id, when the cookie carries one. */
function sidOf(sid: unknown): { sid?: string } {
  return typeof sid === 'string' && sid ? { sid } : {};
}

/** #4500 — how often the Clearing asks the guard to renew one session. */
export const RENEW_EVERY_MS = 10 * 60 * 1000;

/**
 * #4966 — the same relay v1 has had since #4500 (directing/clearing/src/share-session.ts).
 * Measured 2026-11-15: the tunnel sends Clearing v2's traffic straight to this server, so the
 * sign-in guard never saw Jeff use the page and never renewed his sign-in. This server relays his
 * cookie to the guard (loopback) at most once per RENEW_EVERY_MS per session and hands the
 * renewed Set-Cookie back to his browser. The guard holds his tokens and his Session row; this
 * server only carries the cookie. Returns the Set-Cookie value, or null (not due, not renewed,
 * or the guard unreachable — which never refuses the request).
 */
export async function relayRenew(
  cookieHeader: string | undefined,
  sid: string | undefined,
  lastAsked: Map<string, number>,
  guardUrl: string,
  nowMs: number = Date.now(),
  fetchImpl: typeof fetch = fetch,
): Promise<string | null> {
  if (!sid || !cookieHeader) return null;
  const last = lastAsked.get(sid);
  if (last !== undefined && nowMs - last < RENEW_EVERY_MS) return null;
  lastAsked.set(sid, nowMs);
  try {
    const r = await fetchImpl(`${guardUrl.replace(/\/$/, '')}/_auth/renew`, {
      headers: { cookie: cookieHeader }, signal: AbortSignal.timeout(1500),
    });
    return r.status === 204 ? r.headers.get('set-cookie') : null;
  } catch {
    return null;
  }
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

// ---------------------------------------------------------------------------------------
// The gate: who may be served at all. Kept in this file with the cookie it reads (#4966).
// ---------------------------------------------------------------------------------------
/**
 * #4966 — no sign-in, no room (Jeff 2026-11-14 20:18: "i should be forced to login / not see
 * your messsages hut have to login yo reply").
 *
 * Measured 2026-11-15: the tunnel sends lightlifeurbangardens.com/clearing-v2/* straight to this
 * server, past the sign-in guard, and the pages and read APIs asked for no sign-in. So the room
 * always looked signed in, and only send, attach and record found out otherwise.
 *
 * One decision for every request. A request that came through the tunnel, or from any address but
 * this machine, needs a signed-in person: a page is sent to the sign-in page (nothing of the room
 * is rendered), anything else gets 401 not-signed-in. This machine's own callers (probes, pulse,
 * the roles' tools) and /health pass. Being on the home network is not an identity (Jeff
 * 2026-10-10, #4412): another machine on Wi-Fi is asked to sign in like anyone else.
 */

export interface GateRequest {
  headers: Record<string, unknown>;
  ip: string;
  method: string;
  path: string;
  principal: { id: string; name: string; webid: string } | null;
  signInUrl: string | undefined;
  /** true when this server cannot read the key sign-ins are signed with */
  keyUnreadable?: boolean;
  /** true when the request carries a sign-in cookie at all (valid or not) */
  cookiePresent?: boolean;
  /** true when this server sent this browser to sign in moments ago (the bounce marker) */
  bounced?: boolean;
}

export type GateDecision =
  | { allow: true }
  | { allow: false; kind: 'redirect'; location: string }
  | { allow: false; kind: 'refuse'; status: 401; body: { ok: false; error: 'not-signed-in'; detail: string; signInUrl?: string } }
  | { allow: false; kind: 'refuse'; status: 503; body: { ok: false; error: 'sign-in-unreadable' | 'sign-in-not-accepted'; detail: string } };

/** Cloudflare sets these on every request it forwards; the tunnel itself connects from 127.0.0.1. */
export function isTunneled(headers: Record<string, unknown>): boolean {
  return Boolean(headers['cf-connecting-ip'] || headers['cf-ray']);
}

/** This machine only: loopback, in either family, with or without the IPv4-mapped prefix. */
export function isThisMachine(ip: string): boolean {
  const a = (ip || '').replace(/^::ffff:/i, '');
  return a === '127.0.0.1' || a === '::1';
}

/** A page is what a person navigates to; everything under /api/ and /uploads/ is a call. */
function isPage(method: string, path: string): boolean {
  return method === 'GET' && !path.startsWith('/api/') && !path.startsWith('/uploads/');
}

export function gateDecision(r: GateRequest): GateDecision {
  if (r.path === '/health') return { allow: true };
  if (!isTunneled(r.headers) && isThisMachine(r.ip)) return { allow: true };
  if (r.principal) return { allow: true };
  // The guard sends a good session straight back here. A server that cannot read the key cannot
  // tell a good sign-in from none, so it says so and never redirects: the browser would bounce
  // between this server and the guard for ever.
  if (r.keyUnreadable) {
    return {
      allow: false,
      kind: 'refuse',
      status: 503,
      body: { ok: false, error: 'sign-in-unreadable', detail: 'This server cannot read the sign-in key, so it cannot tell who is signed in. Nothing is shown until it can.' },
    };
  }
  // The guard sends a session IT accepts straight back here. If this server still refuses that
  // cookie (any disagreement between the two: a different key, a different clock), a second
  // redirect would start a loop. A browser we sent to sign in moments ago that returns with a
  // cookie we still refuse is told so, and is not redirected again.
  if (r.bounced && r.cookiePresent && isPage(r.method, r.path)) {
    return {
      allow: false,
      kind: 'refuse',
      status: 503,
      body: { ok: false, error: 'sign-in-not-accepted', detail: 'This page was sent to sign in moments ago and came back with a sign-in it cannot use. Wait half a minute and open it again; if this repeats right after signing in, the fault is here.' },
    };
  }
  if (isPage(r.method, r.path) && r.signInUrl) return { allow: false, kind: 'redirect', location: r.signInUrl };
  return {
    allow: false,
    kind: 'refuse',
    status: 401,
    body: r.signInUrl
      ? { ok: false, error: 'not-signed-in', detail: `Not signed in here. Sign in at ${r.signInUrl} to read and send.`, signInUrl: r.signInUrl }
      : { ok: false, error: 'not-signed-in', detail: 'Not signed in: sign in through the common door to read and send.' },
  };
}

/** One line per refused request: when, what, why, and which browser. Never the cookie. */
export function refusalLine(status: number, method: string, path: string, why: string, ua: string, now: Date = new Date()): string {
  return `[clearing-v2] gate.refused ${now.toISOString()} status=${status} why=${why} ${method} ${path} ua="${ua.replace(/"/g, "'").slice(0, 120)}"`;
}

/** The marker this server sets when it sends a browser to sign in; it lasts half a minute. */
export const BOUNCE_COOKIE = 'cv2_signin_bounce';
const BOUNCE_SET = `${BOUNCE_COOKIE}=1; Path=/; Max-Age=30; HttpOnly; SameSite=Lax`;
const BOUNCE_CLEAR = `${BOUNCE_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax`;

/** At most `max` refusal lines a minute reach the log; the rest are counted and said once. This
 *  endpoint is public: anyone can ask it, and a log line each time would flood the service log. */
export function refusalLogLimiter(max: number, log: (line: string) => void, now: () => number = Date.now) {
  let windowStart = 0;
  let written = 0;
  let dropped = 0;
  return (line: string): void => {
    const t = now();
    if (t - windowStart >= 60_000) {
      if (dropped > 0) log(`[clearing-v2] gate.refused.suppressed count=${dropped} (over ${max} a minute)`);
      windowStart = t; written = 0; dropped = 0;
    }
    if (written < max) { written += 1; log(line); } else { dropped += 1; }
  };
}

/** The gate as one middleware, in front of every page and API route. */
export function sessionGate(
  principalOf: (req: Request) => { id: string; name: string; webid: string } | null,
  signInUrl: string | undefined,
  keyUnreadable: () => boolean = () => false,
  /** #4966 — why this request has no signed-in person: 'no-cookie', 'expired', 'bad-signature', 'malformed', 'no-key' */
  whyNot: (req: Request) => string = () => 'unknown',
  log: (line: string) => void = (line) => console.log(line),
) {
  const limited = refusalLogLimiter(60, log);
  return (req: Request, res: Response, next: NextFunction): void => {
    const cookies = String(req.headers.cookie || '');
    const bounced = cookies.split(';').some((c) => c.trim() === `${BOUNCE_COOKIE}=1`);
    const d = gateDecision({
      headers: req.headers as Record<string, unknown>,
      ip: req.socket.remoteAddress || '',
      method: req.method,
      path: req.path,
      principal: principalOf(req),
      signInUrl,
      keyUnreadable: keyUnreadable(),
      cookiePresent: shareSessionFromHeader(cookies) !== undefined,
      bounced,
    });
    if (d.allow) {
      if (bounced) res.append('Set-Cookie', BOUNCE_CLEAR);
      // a signed-in room must never be kept by the browser or the edge and shown after the sign-in is gone
      if (isTunneled(req.headers as Record<string, unknown>)) res.setHeader('Cache-Control', 'no-store');
      next();
      return;
    }
    // AC1's unnamed case (2026-11-15): the phone arrived with no sign-in 36 minutes after one, and
    // nothing recorded whether it sent no cookie or a cookie this server refused. Now it is recorded.
    limited(refusalLine(d.kind === 'redirect' ? 302 : d.status, req.method, req.path, whyNot(req), String(req.headers['user-agent'] || '')));
    // never let a browser or the edge keep a copy of a refusal, or of the room, across a sign-in change
    res.setHeader('Cache-Control', 'no-store');
    if (d.kind === 'redirect') { res.append('Set-Cookie', BOUNCE_SET); res.redirect(302, d.location); return; }
    // the marker is spent: the next visit is sent to sign in afresh, it is not refused again
    if (bounced) res.append('Set-Cookie', BOUNCE_CLEAR);
    res.status(d.status).json(d.body);
  };
}
