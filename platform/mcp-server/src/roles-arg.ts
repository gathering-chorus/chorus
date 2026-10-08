// @domain: werk
/**
 * #4458 — a tool's role argument names a role the roles door lists, never one
 * of three names typed into each tool. Abby (abby-normal) owned #4456 and could
 * not pull it: every werk verb's schema said kade | wren | silas.
 *
 * The schema checks the shape of a role name; the roles door decides whether it
 * is one. `role` (and loom-gemba's `target`) must be an agent role; `owner` may
 * be any agent or human role, since Jeff owns cards too.
 */
import { z } from 'zod';
import { fetchRoleSets, type RoleSets } from './peers';

export const ROLE_PATTERN = '^[a-z][a-z0-9-]*$';
export const AgentRole = z.string().regex(new RegExp(ROLE_PATTERN), 'role must be a role name');

/** The first role argument the roles door does not list, as a refusal; or null. Pure. */
// Tools whose `role` is a log FILTER (it may be 'system'), not a caller role.
const ROLE_IS_A_FILTER = new Set(['chorus_logs_recent_errors', 'chorus_logs_query']);

export function unknownRoleError(tool: string, args: unknown, sets: RoleSets): string | null {
  if (ROLE_IS_A_FILTER.has(tool)) return null;
  const { role, owner, target } = (args ?? {}) as { role?: unknown; owner?: unknown; target?: unknown };
  const checks: Array<[string, unknown, string[]]> = [['role', role, sets.agents], ['owner', owner, sets.peers]];
  if (tool === 'loom-gemba') checks.push(['target', target, sets.agents]);
  for (const [key, v, allowed] of checks) {
    if (typeof v !== 'string' || v === '') continue;
    if (!allowed.includes(v.toLowerCase())) {
      return `Unknown ${key} '${v}' — the roles door lists ${allowed.join(' | ')}`;
    }
  }
  return null;
}

/** Refuse a call whose role argument the roles door does not list. Reads the door only when there is one to check. */
export async function refuseUnknownRoles(tool: string, args: unknown, read: () => Promise<RoleSets> = fetchRoleSets): Promise<void> {
  if (ROLE_IS_A_FILTER.has(tool)) return;
  const { role, owner, target } = (args ?? {}) as { role?: unknown; owner?: unknown; target?: unknown };
  const has = [role, owner, target].some((v) => typeof v === 'string' && v !== '');
  if (!has) return;
  const err = unknownRoleError(tool, args, await read());
  if (err) throw new Error(err);
}
