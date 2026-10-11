/**
 * #4412 — sign-in on the LAN. Jeff's call (2026-10-10 17:24, "make the right
 * long term call"): being on the home network is not an identity. A browser
 * reaching the Clearing over the LAN signs in like anywhere else, so its
 * sign-in writes his Session and roles can start under him.
 *
 * Two things the old flow could not do from a phone: its only non-tunnel
 * callback was http://localhost:3470, which on a phone is the phone. So the
 * callback follows the Host the browser used (the .local name or the IP), and
 * the client document lists every LAN origin derived at boot.
 *
 * Loopback stays trusted: that is the box's own services, not a person.
 */

import { bonjourHost } from './lan-url';

/**
 * Every origin a LAN browser may use: the .local name and the current IP. The
 * name goes through bonjourHost, the same rule the startup URLs use, so a
 * hostname like jeffs-mini-m1-3.lan becomes jeffs-mini-m1-3.local, never
 * jeffs-mini-m1-3.lan.local (seen on the #4412 demo).
 */
export function lanOrigins(localHostName: string | null, lanIp: string | null, port: number): string[] {
  const out: string[] = [];
  const host = bonjourHost(localHostName);
  if (host) out.push(`http://${host}:${port}`);
  if (lanIp) out.push(`http://${lanIp}:${port}`);
  return out;
}

/**
 * The callback for this request: the tunnel's when tunneled, the LAN origin the
 * browser came in on when it is one we list, else localhost (the box itself).
 * A Host we do not list never becomes a redirect target.
 */
export function callbackFor(
  host: string | undefined,
  tunneled: boolean,
  origins: string[],
  port: number,
  tunnelCallback: string,
): string {
  if (tunneled) return tunnelCallback;
  const origin = origins.find((o) => host && o === `http://${host}`);
  return origin ? `${origin}/auth/callback` : `http://localhost:${port}/auth/callback`;
}

export function isLoopback(ip: string): boolean {
  return ip === '127.0.0.1' || ip === '::1' || ip === '::ffff:127.0.0.1';
}

/**
 * What a LAN request without a session gets. Off unless the flag is on, so it
 * ships dark and flips after one phone sign-in is proven (Wren, 17:57: a
 * broken callback would lock him out). Loopback is the box's own services and
 * passes. A person's page load signs in; anything else (an API read, a write)
 * is refused with a 401, so no LAN data stays open without a sign-in (Wren,
 * 18:00).
 *
 * Flip checklist: (1) Jeff signs in once from his phone over the LAN and his
 * Session row appears; (2) the Clearing log shows no non-loopback LAN caller
 * that is a service rather than a person (Bedroom); (3) LAN_ORIGINS is derived
 * at boot, so after a DHCP move the IP callback matches only after a restart.
 */
export function lanAccess(r: {
  flagOn: boolean;
  local: boolean;
  ip: string;
  method: string;
  accept: string | undefined;
  authed: boolean;
}): 'pass' | 'sign-in' | 'refuse' {
  if (!r.flagOn || !r.local || r.authed) return 'pass';
  if (isLoopback(r.ip)) return 'pass';
  if (r.method === 'GET' && (r.accept || '').includes('text/html')) return 'sign-in';
  return 'refuse';
}
