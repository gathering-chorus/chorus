/**
 * #2725 AC1 — decision core for GET /api/nudge/:role/pending.
 *
 * Declared + ENFORCED, role-lane scoped (Silas ruling 2026-08-23):
 * a role reads only its own pending; jeff reads all (role param 'all').
 * The surface is also declared in the model (APISurface, requiresScope
 * nudge-read) so the #3618 envelope gates it when enabled — this in-route
 * check is the enforcement that holds even with the envelope flag off.
 *
 * Refusals name their state: 401 authn-missing (no/invalid token) vs
 * 403 not-your-lane / no-role-held (verified, but not this lane).
 */
import { buildNudgeFold, type PendingNudge } from './nudge-fold';

export interface NudgePendingReq {
  role: string;          // :role path param — a role slug, or 'all' (jeff)
  authorization: string; // raw Authorization header ('' when absent)
}

export interface NudgePendingDeps {
  verify: (token: string) => Promise<{ ok: true; webId: string; scope: string[] } | { ok: false; reason?: string }>;
  /** roles-domain projection: webId → held role slug (holdsRole), null if none */
  roleForWebId: (webId: string) => Promise<string | null>;
  logPath: string;
}

export interface NudgePendingDecision {
  status: number;
  body: PendingNudge[] | { error: string; message: string };
}

export async function decideNudgePending(
  req: NudgePendingReq,
  deps: NudgePendingDeps,
): Promise<NudgePendingDecision> {
  const auth = req.authorization;
  const token = /^bearer /i.test(auth) ? auth.slice(7) : '';
  const result = token ? await deps.verify(token) : null;
  if (!result || !result.ok) {
    return {
      status: 401,
      body: { error: 'authn-missing', message: 'a valid Bearer identity token is required to read pending nudges' },
    };
  }

  const heldRole = await deps.roleForWebId(result.webId);
  if (!heldRole) {
    return {
      status: 403,
      body: { error: 'no-role-held', message: `identity verified but no chorus role is held by ${result.webId}` },
    };
  }

  if (heldRole === 'jeff') {
    const all = req.role === 'all';
    return { status: 200, body: buildNudgeFold(deps.logPath, all ? '' : req.role, { all }) };
  }

  if (req.role !== heldRole) {
    return {
      status: 403,
      body: { error: 'not-your-lane', message: `role '${heldRole}' may read only its own pending lane, not '${req.role}'` },
    };
  }

  return { status: 200, body: buildNudgeFold(deps.logPath, heldRole) };
}
