import type { VerifyResult } from '../es256-identity';

export interface AgentIdentityDeps {
  verify: (token: string) => Promise<VerifyResult>;
  roleForWebId: (webId: string) => Promise<string | null>;
}

/** Authentication only: operation authorization remains at each governed door.
 * Reuses the API's CSS verifier and model role lookup; never trusts a role claim
 * in a token, a caller header, or a WebID suffix. No credentials are returned. */
export async function verifyAgentIdentity(authorization: string, deps: AgentIdentityDeps): Promise<{
  status: number;
  body: { ok: true; principal: string; role: string; scopes: string[] } | { ok: false; error: string };
}> {
  const match = /^Bearer ([^\s]+)$/i.exec(authorization);
  if (!match) return { status: 401, body: { ok: false, error: 'authn-missing' } };
  try {
    const result = await deps.verify(match[1]);
    if (!result.ok) return { status: 401, body: { ok: false, error: result.reason } };
    const role = await deps.roleForWebId(result.webId);
    if (!role || !['wren', 'silas', 'kade', 'jeff'].includes(role)) {
      return { status: 403, body: { ok: false, error: 'role-unresolved' } };
    }
    return { status: 200, body: { ok: true, principal: result.webId, role, scopes: result.scope } };
  } catch {
    return { status: 503, body: { ok: false, error: 'identity-unavailable' } };
  }
}
