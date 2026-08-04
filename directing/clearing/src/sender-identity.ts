/**
 * #3743 — who is speaking, from the login that already happened.
 *
 * The old resolver (resolveJeffMessageSender) read a client-supplied field or a
 * hand-set bridge_name cookie and defaulted to 'jeff' — the session WebID the
 * login had verified was never consulted, so an authenticated Mark rendered as
 * Jeff and had Jeff's authority. This module inverts the precedence:
 *
 *   1. verified session WebID → Principal        (authenticated; name is the
 *      principal's, client fields can never override it)
 *   2. no session + local connection             (machine possession — today's
 *      behaviour: fromField/bridge_name fallback, jeff-authority)
 *   3. no session + remote                       (guest: display-name fallback,
 *      NO jeff-authority)
 *
 * jeffAuthority — may this sender's input travel the /api/jeff-input path and
 * carry Jeff's authority at receiving sessions — is true ONLY for the
 * jeff-principal (case 1) or local possession (case 2). An allowed WebID with
 * no Principal mapping is NOT silently jeff: named by webid, no authority.
 */

export interface SenderIdentity {
  name: string;
  principal?: string;
  webid?: string;
  authenticated: boolean;
  jeffAuthority: boolean;
}

export interface ResolveSenderOpts {
  cookieHeader: string;
  fromField?: string;
  isLocal: boolean;
  /** Verified session from the cookie header (verifyCookie + freshness), or null. */
  verifySession: (cookieHeader: string) => { webid?: string; fresh: boolean } | null;
  /** WebID → Principal from the security graph (TTL-cached beside the allow-set). */
  principalFor: (webid: string) => Promise<{ id: string; name: string } | null>;
}

const JEFF_PRINCIPAL = 'principal-jeff';

function guestName(cookieHeader: string, fromField?: string): string {
  const m = cookieHeader.match(/bridge_name=([^;]+)/);
  const bridgeName = m ? decodeURIComponent(m[1]) : '';
  return fromField || bridgeName || '';
}

export async function resolveSenderIdentity(opts: ResolveSenderOpts): Promise<SenderIdentity> {
  const sess = opts.verifySession(opts.cookieHeader);
  if (sess?.webid && sess.fresh) {
    const p = await opts.principalFor(sess.webid);
    if (p) {
      return {
        name: p.name,
        principal: p.id,
        webid: sess.webid,
        authenticated: true,
        jeffAuthority: p.id === JEFF_PRINCIPAL,
      };
    }
    // Allowed-but-unmapped is a finding, not a promotion: never default to jeff.
    return {
      name: sess.webid,
      webid: sess.webid,
      authenticated: true,
      jeffAuthority: false,
    };
  }
  const fallback = guestName(opts.cookieHeader, opts.fromField);
  if (opts.isLocal) {
    return { name: fallback || 'jeff', authenticated: false, jeffAuthority: true };
  }
  return { name: fallback || 'guest', authenticated: false, jeffAuthority: false };
}
