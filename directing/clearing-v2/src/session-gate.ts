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
}

export type GateDecision =
  | { allow: true }
  | { allow: false; kind: 'redirect'; location: string }
  | { allow: false; kind: 'refuse'; status: 401; body: { ok: false; error: 'not-signed-in'; detail: string; signInUrl?: string } };

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

/** The gate as one middleware, in front of every page and API route. */
export function sessionGate(
  principalOf: (req: Request) => { id: string; name: string; webid: string } | null,
  signInUrl: string | undefined,
) {
  return (req: Request, res: Response, next: NextFunction): void => {
    const d = gateDecision({
      headers: req.headers as Record<string, unknown>,
      ip: req.socket?.remoteAddress || '',
      method: req.method,
      path: req.path,
      principal: principalOf(req),
      signInUrl,
    });
    if (d.allow) { next(); return; }
    // never let a browser or the edge keep a copy of a refusal, or of the room, across a sign-in change
    res.setHeader('Cache-Control', 'no-store');
    if (d.kind === 'redirect') { res.redirect(302, d.location); return; }
    res.status(d.status).json(d.body);
  };
}
