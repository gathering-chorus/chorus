import { AsyncLocalStorage } from 'node:async_hooks';

export type ChorusRole = 'wren' | 'silas' | 'kade' | 'jeff';
export type AgentIdentity =
  | { mode: 'legacy-claude'; role: string }
  | { mode: 'verified'; role: ChorusRole; principal: string; scopes: string[]; token: string; sessionId?: string };

export type IdentityMode = 'strict' | 'legacy-claude';
export interface AgentIdentityRequest {
  authorization?: string;
  sessionId?: string;
  role?: string;
  userAgent?: string;
}
export interface VerifiedPrincipal { principal: string; role: ChorusRole; scopes: string[] }
export interface AgentAuthDeps {
  mode: IdentityMode;
  verify: (token: string) => Promise<VerifiedPrincipal>;
  session?: (sessionId: string, token: string) => Promise<{ session_id: string; principal: string; role: string; state: string }>;
  legacyRole?: string;
}
export class AgentAuthError extends Error {
  constructor(public readonly status: number, public readonly reason: string) {
    super(reason);
  }
}
const roles: readonly string[] = ['wren', 'silas', 'kade', 'jeff'];

export function identityMode(value: string | undefined): IdentityMode {
  if (value === undefined || value === 'legacy-claude') return 'legacy-claude';
  if (value === 'strict') return 'strict';
  throw new Error('CHORUS_MCP_IDENTITY_MODE must be strict or legacy-claude');
}

/** An enrolled session or any supplied credential always selects verification.
 * Legacy is an explicit deployment compatibility mode, never an auth fallback. */
export async function authenticateAgentRequest(request: AgentIdentityRequest, deps: AgentAuthDeps): Promise<AgentIdentity> {
  const mustVerify = deps.mode === 'strict' || request.sessionId !== undefined || request.authorization !== undefined;
  if (!mustVerify) {
    const role = request.role ?? deps.legacyRole ?? 'unknown';
    return { mode: 'legacy-claude', role: roles.includes(role) ? role : 'unknown' };
  }
  if (request.sessionId !== undefined && !/^[A-Za-z0-9_-]{1,200}$/.test(request.sessionId)) {
    throw new AgentAuthError(400, 'invalid-session-id');
  }
  const match = /^Bearer ([^\s]+)$/i.exec(request.authorization ?? '');
  if (!match) throw new AgentAuthError(401, 'authn-missing');
  let principal: VerifiedPrincipal;
  try { principal = await deps.verify(match[1]); }
  catch (err) {
    if (err instanceof AgentAuthError) throw err;
    throw new AgentAuthError(503, 'identity-unavailable');
  }
  if (!roles.includes(principal.role) || !principal.principal) throw new AgentAuthError(403, 'role-unresolved');
  if (request.role !== undefined && request.role !== principal.role) throw new AgentAuthError(403, 'role-mismatch');
  if (request.sessionId) {
    if (!deps.session) throw new AgentAuthError(503, 'session-verification-unavailable');
    let session: Awaited<ReturnType<NonNullable<AgentAuthDeps['session']>>>;
    try { session = await deps.session(request.sessionId, match[1]); }
    catch (err) {
      if (err instanceof AgentAuthError) throw err;
      throw new AgentAuthError(503, 'session-verification-unavailable');
    }
    if (session.session_id !== request.sessionId || session.principal !== principal.principal || session.role !== principal.role
      || !['idle', 'running', 'awaiting_approval'].includes(session.state)) throw new AgentAuthError(403, 'session-identity-mismatch');
  }
  return { mode: 'verified', ...principal, token: match[1], ...(request.sessionId ? { sessionId: request.sessionId } : {}) };
}

/** One trust implementation: the Chorus API verifies CSS ES256 and resolves the
 * role/scopes from its model. Redirects are refused so credentials cannot leak. */
export function apiIdentityVerifier(apiBase: string, fetchImpl: typeof fetch = fetch): AgentAuthDeps['verify'] {
  const endpoint = new URL('/api/chorus/identity/verify', apiBase);
  if (endpoint.protocol !== 'https:' && !(endpoint.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(endpoint.hostname))) {
    throw new Error('Identity verification requires HTTPS or loopback HTTP');
  }
  return async (token) => {
    const response = await fetchImpl(endpoint, {
      method: 'POST', headers: { Authorization: `Bearer ${token}` },
      redirect: 'error', signal: AbortSignal.timeout(5000),
    });
    if (!response.ok) throw new AgentAuthError(response.status === 401 || response.status === 403 ? response.status : 503, response.status === 401 ? 'identity-invalid' : response.status === 403 ? 'role-unresolved' : 'identity-unavailable');
    const data = await response.json() as Partial<VerifiedPrincipal> & { ok?: boolean };
    if (data.ok !== true || typeof data.principal !== 'string' || !roles.includes(data.role ?? '') || !Array.isArray(data.scopes) || !data.scopes.every((s) => typeof s === 'string')) {
      throw new AgentAuthError(503, 'identity-response-invalid');
    }
    return { principal: data.principal, role: data.role as ChorusRole, scopes: data.scopes };
  };
}

/** The API authorizes access and the caller additionally checks exact identity:
 * a human may inspect any session but cannot claim another session as itself. */
export function apiSessionVerifier(apiBase: string, fetchImpl: typeof fetch = fetch): NonNullable<AgentAuthDeps['session']> {
  apiIdentityVerifier(apiBase, fetchImpl); // validate the same trusted origin rules
  return async (sessionId, token) => {
    const response = await fetchImpl(new URL(`/api/chorus/agent-sessions/${encodeURIComponent(sessionId)}`, apiBase), {
      headers: { Authorization: `Bearer ${token}` }, redirect: 'error', signal: AbortSignal.timeout(5000),
    });
    if (!response.ok) throw new AgentAuthError([401, 403, 404].includes(response.status) ? 403 : 503, 'session-verification-refused');
    const data = await response.json() as Record<string, unknown>;
    if (!['session_id', 'principal', 'role', 'state'].every(key => typeof data[key] === 'string')) throw new AgentAuthError(503, 'session-response-invalid');
    return { session_id: data.session_id as string, principal: data.principal as string, role: data.role as string, state: data.state as string };
  };
}

const requestIdentity = new AsyncLocalStorage<AgentIdentity>();
export function withAgentIdentity<T>(identity: AgentIdentity, fn: () => T): T { return requestIdentity.run(identity, fn); }
export function currentAgentIdentity(): AgentIdentity | undefined { return requestIdentity.getStore(); }

/** HTTP daemon startup must not retain a launching shell's session credentials.
 * Stdio is a per-session process and deliberately does not call this. */
export function clearDaemonIdentityEnvironment(env: NodeJS.ProcessEnv): void {
  for (const key of ['CHORUS_IDENTITY_TOKEN', 'CHORUS_SESSION_TOKEN_FILE', 'CHORUS_SESSION_ID', 'CHORUS_ACTOR_WEBID']) delete env[key];
}

/** Never inherit a shared daemon's credential, including in the legacy lane. */
export function requestEnvironment(base: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  const env = { ...base };
  clearDaemonIdentityEnvironment(env);
  const identity = requestIdentity.getStore();
  if (identity) {
    env.CHORUS_ROLE = identity.role;
    env.DEPLOY_ROLE = identity.role;
  }
  if (identity?.mode === 'verified') {
    env.CHORUS_IDENTITY_TOKEN = identity.token;
    env.CHORUS_ACTOR_WEBID = identity.principal;
    if (identity.sessionId) env.CHORUS_SESSION_ID = identity.sessionId;
  }
  return env;
}

// Read filters can name another role. Mutation role arguments denote the actor.
const roleReadTools = new Set(['chorus_commit_status', 'chorus_wip', 'chorus_sup', 'chorus_priorities_readout', 'chorus_migration_readout', 'chorus_logs_recent_errors', 'chorus_logs_for_card', 'chorus_logs_for_trace', 'chorus_logs_for_branch']);
export function authorizeAgentTool(identity: AgentIdentity, tool: string, args: Record<string, unknown> = {}): void {
  if (identity.mode !== 'verified') return;
  if (tool === 'chorus_card_add_jeff' && identity.role !== 'jeff') throw new AgentAuthError(403, 'human-authorization-required');
  const acceptsCard = tool === 'werk-accept' || tool === 'chorus_cards_done' || (tool === 'chorus_werk' && args.go === true);
  if (acceptsCard && !['jeff', 'wren'].includes(identity.role)) throw new AgentAuthError(403, 'acceptance-authorization-required');
  if (typeof args.role === 'string' && args.role !== identity.role && identity.role !== 'jeff' && !roleReadTools.has(tool) && !acceptsCard) {
    throw new AgentAuthError(403, 'actor-role-mismatch');
  }
  if (args.go === true && args.accepter !== identity.role) throw new AgentAuthError(403, 'accepter-identity-mismatch');
}
