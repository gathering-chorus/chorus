/**
 * #3669 — one connection-locality classifier for BOTH transports.
 *
 * The 2026-07-23 research (card #3669) found the live hole: the HTTP gate
 * (`isLocal`) treats a request as remote when it carries Cloudflare tunnel
 * headers (`cf-connecting-ip` / `cf-ray`), but the Socket.IO gate classified
 * locality by `handshake.address` ALONE. cloudflared proxies a tunneled service
 * from 127.0.0.1, so a WebSocket arriving over `clearing.lightlifeurbangardens.com`
 * presented address 127.0.0.1 → was treated as local → the bridge token was
 * skipped entirely. That WS channel carries `jeff-message`, which injects raw
 * text into role terminal sessions: an unauthenticated command path over the
 * public tunnel.
 *
 * The fix is to make ONE classifier the single source of truth. A connection is
 * REMOTE (token required) if it is tunneled, regardless of source address. The
 * cf-* headers ride on the WS upgrade request exactly as they do on an HTTP
 * request, so the same check works for both.
 */

/** Cloudflare adds these to every proxied request, HTTP and WS-upgrade alike. */
export function isTunneled(headers: Record<string, unknown>): boolean {
  return Boolean(headers['cf-connecting-ip'] || headers['cf-ray']);
}

/**
 * Loopback only — the box's own agents and tools. #4412: the home LAN used to
 * be here too, which let any browser on the network in with no sign-in. Being
 * on the home network is not an identity (Jeff, 2026-10-10).
 */
export function isLocalAddress(ip: string): boolean {
  return ip === '127.0.0.1' || ip === '::1' || ip === '::ffff:127.0.0.1';
}

/**
 * The single locality decision for both transports. LOCAL (token-exempt) only
 * when the connection is NOT tunneled AND it comes from loopback. A tunneled
 * request is never local even from 127.0.0.1 — that is the whole #3669 bug.
 */
export function isLocalConnection(headers: Record<string, unknown>, ip: string): boolean {
  if (isTunneled(headers)) return false;
  return isLocalAddress(ip);
}

/** The door's cookie is scoped to this parent domain; only these Hosts can carry it. */
const COOKIE_DOMAIN = 'lightlifeurbangardens.com';

/** True when the browser's Host can receive the door's parent-domain cookie. */
export function hostCarriesDoorCookie(host: string): boolean {
  const name = host.trim().toLowerCase().replace(/:\d+$/, '');
  return name === COOKIE_DOMAIN || name.endsWith(`.${COOKIE_DOMAIN}`);
}

/**
 * #4412 — a home-network browser (Host an IP or a .local name) can never carry
 * the door's cookie, so it could never sign in here. Send it to the public
 * Clearing with the same path, where the door's cookie reaches. Returns null
 * for loopback, the tunnel, and any Host that can carry the cookie.
 */
export function lanRedirect(
  headers: Record<string, unknown>,
  ip: string,
  originalUrl: string,
  publicUrl: string,
): string | null {
  if (isTunneled(headers) || isLocalAddress(ip)) return null;
  if (hostCarriesDoorCookie(String(headers.host || ''))) return null;
  const safe = originalUrl.startsWith('/') && !originalUrl.startsWith('//') && !originalUrl.startsWith('/\\')
    ? originalUrl : '/';
  return `${publicUrl.replace(/\/+$/, '')}${safe}`;
}

/**
 * #3669 — reconstruct the PUBLIC url the client actually signed, for the DPoP
 * `htu` check.
 *
 * DPoP (RFC 9449) binds a token to the exact request URL. Behind the Cloudflare
 * tunnel the browser/agent signs `https://clearing.lightlifeurbangardens.com/…`,
 * but this Express process sees `http://localhost:3470/…`. A verifier that
 * compares against the local URL 401s every tunneled request. So we rebuild the
 * external URL from the proxy headers and hand THAT to the verifier.
 *
 * Scheme: X-Forwarded-Proto wins (Cloudflare sets it); else https when tunneled
 * (the public door is always TLS at the edge); else the fallback. Host: the Host
 * header, which cloudflared preserves by default (an operator overriding
 * httpHostHeader breaks this — documented, not defended). Path: originalUrl as
 * received; DPoP htu comparison ignores query + fragment, so the verifier
 * normalizes — we pass the full path and let it strip.
 */
export function externalRequestUrl(
  headers: Record<string, unknown>,
  originalUrl: string,
  fallbackProto = 'http',
): string {
  const xfProto = String(headers['x-forwarded-proto'] || '').split(',')[0].trim();
  const proto = xfProto || (isTunneled(headers) ? 'https' : fallbackProto);
  const host = String(headers['host'] || '').trim();
  return `${proto}://${host}${originalUrl}`;
}
