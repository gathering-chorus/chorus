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
import type { Request, Response, NextFunction } from 'express';

export interface GateRequest {
  headers: Record<string, unknown>;
  ip: string;
  method: string;
  path: string;
  principal: { id: string; name: string; webid: string } | null;
  signInUrl: string | undefined;
  /** true when this server cannot read the key sign-ins are signed with */
  keyUnreadable?: boolean;
}

export type GateDecision =
  | { allow: true }
  | { allow: false; kind: 'redirect'; location: string }
  | { allow: false; kind: 'refuse'; status: 401; body: { ok: false; error: 'not-signed-in'; detail: string; signInUrl?: string } }
  | { allow: false; kind: 'refuse'; status: 503; body: { ok: false; error: 'sign-in-unreadable'; detail: string } };

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

/** The gate as one middleware, in front of every page and API route. */
export function sessionGate(
  principalOf: (req: Request) => { id: string; name: string; webid: string } | null,
  signInUrl: string | undefined,
  keyUnreadable: () => boolean = () => false,
  /** #4966 — why this request has no signed-in person: 'no-cookie', 'expired', 'bad-signature', 'malformed', 'no-key' */
  whyNot: (req: Request) => string = () => 'unknown',
  log: (line: string) => void = (line) => console.log(line),
) {
  return (req: Request, res: Response, next: NextFunction): void => {
    const d = gateDecision({
      headers: req.headers as Record<string, unknown>,
      ip: req.socket?.remoteAddress || '',
      method: req.method,
      path: req.path,
      principal: principalOf(req),
      signInUrl,
      keyUnreadable: keyUnreadable(),
    });
    if (d.allow) {
      // a signed-in room must never be kept by the browser or the edge and shown after the sign-in is gone
      if (isTunneled(req.headers as Record<string, unknown>)) res.setHeader('Cache-Control', 'no-store');
      next();
      return;
    }
    // AC1's unnamed case (2026-11-15): the phone arrived with no sign-in 36 minutes after one, and
    // nothing recorded whether it sent no cookie or a cookie this server refused. Now it is recorded.
    log(refusalLine(d.kind === 'redirect' ? 302 : d.status, req.method, req.path, whyNot(req), String(req.headers['user-agent'] || '')));
    // never let a browser or the edge keep a copy of a refusal, or of the room, across a sign-in change
    res.setHeader('Cache-Control', 'no-store');
    if (d.kind === 'redirect') { res.redirect(302, d.location); return; }
    res.status(d.status).json(d.body);
  };
}
