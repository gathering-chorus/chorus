// @domain: messages

export type BindOutcome =
  | { ok: true; webid: string; pubkey: string }
  | { ok: false; status: 401 | 400; error: 'not-signed-in' | 'bad-pubkey'; detail: string };

const PUBKEY_RE = /^[0-9a-f]{64}$/i;

/**
 * Decide a bind: may this browser's key speak as this WebID? (#3827)
 */
export function decideBind(webid: string | null | undefined, pubkey: unknown): BindOutcome {
  if (!webid) {
    return {
      ok: false,
      status: 401,
      error: 'not-signed-in',
      detail: 'A key can only be bound to a signed-in WebID. Sign in, then join the room.',
    };
  }
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
