/**
 * #2436 — one decision about who may talk to chorus-api cross-origin.
 *
 * chorus-api binds 0.0.0.0:3340, so `Access-Control-Allow-Origin: *` on a block
 * that also allows POST/PUT/DELETE means any page in any browser on the network
 * can write to it. That was live on `/api/athena/*` (GET/POST/PUT/DELETE) and on
 * `/api/chorus/open`, which asks the machine to open a file.
 *
 * The narrowing #2431 did was a single hardcoded `http://localhost:3000`. That is
 * right about methods and wrong about reach: the werk variants and the demo pages
 * are served on other ports, and Jeff reads them from his phone over the LAN, so
 * one hardcoded origin silently excludes the people who actually use it.
 *
 * The allow-list below is derived from the TOPOLOGY, not from observed traffic,
 * and that is deliberate: the spine records no browser Origin for these routes
 * (the only `origin=` values in the log are call-source tags — mcp, cli, local),
 * so there is no measured list to narrow to. Anything claiming otherwise would be
 * inventing data. It follows that this change is proven by probe, not by log.
 *
 * So: echo the caller's Origin only when it is unambiguously this machine — a
 * loopback host, or a private-network address on the same LAN — on any port.
 * Everything else gets no Access-Control-Allow-Origin header at all, which is
 * what makes a browser refuse the response.
 *
 * Server-to-server callers (the land posting rows, the verbs, curl) send no
 * Origin and are unaffected: CORS is a browser rule, not an auth check. This
 * closes a browser-CSRF surface; it is not, and must not be read as, authz.
 *
 * #2041 — all of this goes away when Athena is hosted alongside its consumers.
 */

/** Hosts that are always this machine. */
const LOOPBACK = new Set(['localhost', '127.0.0.1', '::1']);

/**
 * Private-network IPv4 ranges (RFC 1918) plus link-local. The LAN the two Macs
 * and Jeff's phone share is 192.168.86.0/24; a range test avoids pinning a
 * DHCP-volatile address (ADR-012 records that the Library's IP moves).
 */
function isPrivateIPv4(host: string): boolean {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(host);
  if (!m) return false;
  const octets = [Number(m[1]), Number(m[2]), Number(m[3]), Number(m[4])];
  if (octets.some((n) => n > 255)) return false;
  const [a, b] = octets;
  if (a === 10) return true;
  if (a === 192 && b === 168) return true;
  if (a === 172 && b >= 16 && b <= 31) return true;
  if (a === 169 && b === 254) return true; // link-local
  return false;
}

/**
 * The value to echo in Access-Control-Allow-Origin, or null when the caller must
 * not be granted one.
 *
 * null covers three cases that all mean "send no header":
 *   - no Origin at all (a server-to-server call — nothing to grant)
 *   - an Origin this machine does not recognise (the case this card closes)
 *   - an Origin that does not parse as a URL
 */
export function corsAllowOrigin(origin: string | undefined | null): string | null {
  if (!origin) return null;
  let url: URL;
  try {
    url = new URL(origin);
  } catch {
    return null;
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
  // URL normalises an IPv6 host to bracketed form; strip for the loopback test.
  const host = url.hostname.replace(/^\[|\]$/g, '');
  if (LOOPBACK.has(host)) return origin;
  if (isPrivateIPv4(host)) return origin;
  return null;
}
