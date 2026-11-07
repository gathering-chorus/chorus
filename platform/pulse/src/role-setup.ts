/**
 * role-setup.ts — #4919 AC4: pulse checks every role's setup from outside the role.
 *
 * Pulse runs as system, so it can see what a role cannot report about itself: on
 * 2026-11-07 the lockout broke the roles' own hooks (Wren: 1 of ~20 replies got out),
 * and a 660 key stops the role's own replies from publishing at all. The in-role POST
 * (platform/scripts/role-post) shows the refusal in the role's terminal; this shows it
 * to Jeff — on the pulse roles line, and as one line in #team on each change.
 * A read that fails is a failure that says so, never a pass (#3734).
 */
import { execFile } from 'child_process';
import { promises as fs } from 'fs';
import { promisify } from 'util';

export interface SetupReads {
  principals: () => Promise<Array<{ name: string; hostAccount: string }>>;
  /** the role's nostr.json mode bits, or null when there is no key */
  keyMode: (role: string) => Promise<number | null>;
  /** true when the role's settings.json differs from git */
  settingsDrift: (role: string) => Promise<boolean>;
}

const why = (e: unknown) => (e instanceof Error ? e.message : String(e)).split('\n')[0].slice(0, 120);
const octal = (m: number) => '0' + (m & 0o777).toString(8).padStart(3, '0');

export async function checkRoleSetup(role: string, r: SetupReads): Promise<string[]> {
  const fails: string[] = [];
  const account = `chorus-${role}`;
  try {
    const ps = await r.principals();
    if (!ps.some(p => p.hostAccount === account)) fails.push(`identity: no principal runs as ${account}`);
  } catch (e) {
    fails.push(`identity: roles door unreadable (${why(e)})`);
  }
  try {
    const m = await r.keyMode(role);
    if (m === null) fails.push('key: nostr.json missing');
    else if ((m & 0o777) !== 0o600) fails.push(`key: nostr.json mode ${octal(m)}, want 0600`);
  } catch (e) {
    fails.push(`key: unreadable (${why(e)})`);
  }
  try {
    if (await r.settingsDrift(role)) fails.push('settings: differs from git');
  } catch (e) {
    fails.push(`settings: unmeasured (${why(e)})`);
  }
  return fails;
}

/** Ticks over the roles; posts one line per change (refused ↔ ok). A failed post is retried. */
export class RoleSetupWatch {
  private told = new Map<string, string>(); // role → last state told to Jeff ('' = ok)
  private now = new Map<string, string[]>();
  /** the last post that failed, per role — the next tick retries it; kept so the failure is visible */
  readonly postErrors = new Map<string, string>();
  constructor(private roles: string[], private reads: SetupReads, private post: (line: string) => Promise<void>) {}

  async tick(): Promise<void> {
    for (const role of this.roles) {
      const fails = await checkRoleSetup(role, this.reads);
      this.now.set(role, fails);
      const key = fails.join('; ');
      const prev = this.told.get(role);
      if (prev === undefined && key === '') { this.told.set(role, ''); continue; }
      if (prev === key) continue;
      const line = key ? `role setup broken (${role}): ${key}` : `role setup ok (${role}): fixed`;
      try {
        await this.post(line);
        this.told.set(role, key);
        this.postErrors.delete(role);
      } catch (e) {
        // not told: kept as a post error, and the next tick tries again
        this.postErrors.set(role, why(e));
      }
    }
  }

  state(): Record<string, string[]> {
    return Object.fromEntries(this.now);
  }
}

const run = promisify(execFile);

/** The live reads: the roles door, the identity dir, and git in canonical. */
export function liveReads(
  door = process.env.ATHENA_MAKE_URL || 'http://localhost:3360',
  identityDir = process.env.CHORUS_IDENTITY_ROOT || '/Users/jeffbridwell/.chorus/identity',
  canonical = process.env.CHORUS_HOME || '/Users/jeffbridwell/CascadeProjects/chorus',
): SetupReads {
  return {
    principals: async () => {
      const res = await fetch(`${door}/v1/identity/principals?limit=500`, { signal: AbortSignal.timeout(4000) });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const body = await res.json() as { data?: Array<{ name?: unknown; hostAccount?: unknown }> };
      const str = (v: unknown) => (typeof v === 'string' ? v : '');
      return (body.data ?? []).map(p => ({ name: str(p.name), hostAccount: str(p.hostAccount) }));
    },
    keyMode: async role => {
      // role comes from PULSE_SETUP_ROLES, never from a request; refuse anything that is not a role name
      if (!/^[a-z][a-z0-9-]*$/.test(role)) throw new Error(`not a role name: ${role}`);
      // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is identityDir + a checked role name
      try { return (await fs.stat(`${identityDir}/${role}/nostr.json`)).mode; }
      catch (e) { if ((e as NodeJS.ErrnoException).code === 'ENOENT') return null; throw e; }
    },
    settingsDrift: async role => {
      const rel = [`roles/${role}/.claude/settings.json`, `roles/${role}/.gemini/settings.json`];
      const { stdout } = await run('git', ['-C', canonical, 'status', '--porcelain', '--', ...rel], { timeout: 4000 });
      return stdout.trim() !== '';
    },
  };
}
