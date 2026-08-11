/**
 * #3827 — the bind decision: may this browser's key speak as this person?
 *
 * Pure on purpose. The route around it does cookies and JSON; the DECISION is
 * here, where it can be examined without a server. Today's lesson was that the
 * dangerous failure is not a crash but a bind that succeeds when it should not
 * — and that is a decision, not plumbing.
 */

export type BindOutcome =
  | { ok: true; webid: string; pubkey: string }
  | { ok: false; status: 401 | 400; error: 'not-signed-in' | 'bad-pubkey'; detail: string };

/** A 32-byte Schnorr public key, hex. */
const PUBKEY_RE = /^[0-9a-f]{64}$/i;

/**
 * Decide a bind.
 *
 * IDENTITY IS CHECKED FIRST, deliberately. If the key were validated first, a
 * stranger sending a well-formed key would learn that their forged session was
 * the only thing standing between them and speaking as Jeff — and a route that
 * reports "bad key" to an unauthenticated caller has already told them their
 * session was accepted.
 *
 * `webid` must come from a SERVER-VERIFIED session (#3679). This function
 * cannot check that itself, which is exactly why the caller must never pass it
 * anything a client said about itself.
 */
export function decideBind(webid: string | null, pubkey: unknown): BindOutcome {
  if (!webid) {
    return {
      ok: false,
      status: 401,
      error: 'not-signed-in',
      detail: 'A key can only be bound to a signed-in WebID. Sign in, then join the room.',
    };
  }
  // Narrow BEFORE stringifying: String({}) is "[object Object]", which would
  // fail the regex for the right reason by accident. Reject non-strings on
  // their own terms so the check stays about the key's shape.
  const key = typeof pubkey === 'string' ? pubkey : '';
  if (!PUBKEY_RE.test(key)) {
    return {
      ok: false,
      status: 400,
      error: 'bad-pubkey',
      detail: 'Expected a 32-byte hex Schnorr public key — the PUBLIC half only.',
    };
  }
  return { ok: true, webid, pubkey: key.toLowerCase() };
}
