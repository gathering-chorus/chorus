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

/** Loopback + Jeff's home LAN — the address ranges that skip the token. */
export function isLocalAddress(ip: string): boolean {
  if (!ip) return false;
  if (ip === '127.0.0.1' || ip === '::1' || ip === '::ffff:127.0.0.1') return true;
  if (ip.startsWith('192.168.86.') || ip.startsWith('::ffff:192.168.86.')) return true;
  return false;
}

/**
 * The single locality decision for both transports. LOCAL (token-exempt) only
 * when the connection is NOT tunneled AND its source address is local. A
 * tunneled request is never local even from 127.0.0.1 — that is the whole bug.
 */
export function isLocalConnection(headers: Record<string, unknown>, ip: string): boolean {
  if (isTunneled(headers)) return false;
  return isLocalAddress(ip);
}
