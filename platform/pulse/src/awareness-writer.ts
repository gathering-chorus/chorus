/**
 * awareness-writer.ts — #4744 slice 3c: pulse writes the tree's rows through pulse-api.
 *
 * Signed as principal-pulse (Silas 2026-10-26 10:02: pulse is the home, chorus-principal
 * is login identity), with the credential `platform/scripts/chorus-identity-token pulse`
 * makes from pulse's cred file — the same path pulse-api uses for its own door writes.
 *
 * Only a change is written. Every replace keeps a prior version in provenance, so a role's
 * state rewritten each 30-second tick would mint ~17,000 versions a day: a RoleState row
 * is written when its activity flips or its last call moves into a new 5-minute window,
 * a Turn when it changes (started, then ended), a Prompt once.
 *
 * Off unless PULSE_AWARENESS_WRITE=1: turning it on is a prod write, and that is Jeff's go.
 */
import { execFile } from 'child_process';
import * as path from 'path';
import { awarenessRows, type AwarenessRows, type Row } from './awareness-rows';
import { factsFromActorState, type SpineLine } from './pulse-tree';
import { cardStepRows, readCardSteps, readLiveSteps } from './card-steps';

export type Send = (method: 'POST' | 'PUT' | 'DELETE', path: string, body: Row, credential: string) => Promise<number>;
export interface WriteResult { made: number; updated: number; unchanged: number; failed: number; failures: string[]; settled: number }
type Kind = 'prompts' | 'roleStates' | 'turns' | 'cardSteps';

const WINDOW_MS = 5 * 60_000;

/** What must differ for a row to be written again. */
export function signature(kind: Kind, row: Row): string {
  if (kind === 'prompts') return String(row.name);
  if (kind === 'roleStates') return `${row.activity}|${Math.floor(Date.parse(String(row.observedAt)) / WINDOW_MS)}`;
  return JSON.stringify(row);
}

export class AwarenessWriter {
  private written = new Map<string, string>();
  private refused = new Map<string, string>();
  private retired = new Set<string>();
  constructor(private send: Send, private credential: () => Promise<string>, private base = '/pulse/awareness') {}

  /** Prompts first (a Turn links its Prompt), then role states, then turns. */
  async write(rows: AwarenessRows): Promise<WriteResult> {
    const r: WriteResult = { made: 0, updated: 0, unchanged: 0, failed: 0, failures: [], settled: 0 };
    let cred: string;
    try {
      cred = await this.credential();
    } catch (e) {
      r.failed = rows.prompts.length + rows.roleStates.length + rows.turns.length + (rows.cardSteps?.length ?? 0);
      r.failures.push(`credential: ${e instanceof Error ? e.message : String(e)}`);
      return r;
    }
    const order: Array<[Kind, string, Row[]]> = [
      ['prompts', 'prompts', rows.prompts], ['roleStates', 'rolestates', rows.roleStates], ['turns', 'turns', rows.turns],
      ['cardSteps', 'cardsteps', rows.cardSteps ?? []],
    ];
    for (const [kind, segment, list] of order) {
      for (const row of list) await this.writeRow(kind, `${this.base}/${segment}`, row, cred, r);
    }
    return r;
  }

  /** #4757 — role states moved to pulse's own names (observed-<role>); the role-named rows pulse
   *  wrote on 10-26 would otherwise sit beside them, stale, on the header. Each is deleted once;
   *  gone (404) counts as done, and a row pulse may not delete (wren's own) is left alone. */
  async retireRoleNamed(roles: string[]): Promise<string[]> {
    const out: string[] = [];
    let cred: string;
    try { cred = await this.credential(); } catch { return out; }
    for (const role of roles) {
      if (this.retired.has(role)) continue;
      const code = await this.send('DELETE', `${this.base}/rolestates/${role}`, { name: role }, cred);
      if (code === 0 || code >= 500) continue; // retried next tick
      this.retired.add(role);
      out.push(`${role}: ${code}`);
    }
    return out;
  }

  /** One row: skipped when unchanged; created, or replaced in place when it exists (409).
   *  #4757 — a row the door refused (4xx) is settled: not sent again until it changes. On 10-26
   *  the same ~400 refused rows were resent every 30 s for hours (27,133 refusals in 95 min). */
  private async writeRow(kind: Kind, collection: string, row: Row, cred: string, r: WriteResult): Promise<void> {
    const key = `${kind}/${String(row.name)}`;
    const sig = signature(kind, row);
    if (this.written.get(key) === sig) { r.unchanged++; return; }
    if (this.refused.get(key) === sig) { r.settled++; return; }
    let code = await this.send('POST', collection, row, cred);
    const replaced = code === 409;
    if (replaced) code = await this.send('PUT', `${collection}/${String(row.name)}`, row, cred);
    if (code < 200 || code >= 300) {
      r.failed++;
      r.failures.push(`${key}: HTTP ${code}`);
      if (code >= 400 && code < 500) this.refused.set(key, sig); // a 0 or 5xx is retried next tick
      return;
    }
    this.refused.delete(key);
    this.written.set(key, sig);
    if (replaced) r.updated++; else r.made++;
  }
}

/** #4757 — refusals counted by collection and code ("rolestates HTTP 503 ×6"), so the one
 *  failing collection is not hidden behind the first of four hundred. */
export function failureCounts(failures: string[]): string {
  const n = new Map<string, number>();
  for (const f of failures) {
    const k = f.replace(/\/[^:]*:/, ':');
    n.set(k, (n.get(k) ?? 0) + 1);
  }
  return [...n].map(([k, c]) => `${k} ×${c}`).join(', ');
}

// --- live wiring ---------------------------------------------------------------


type LineStore = { queryMessages: (o: { type?: string; since?: string; limit?: number }) => Array<{ id: number; from: string; to: string; content: string; created_at?: string; createdAt?: string }> };
type Emit = (event: string, fields: Record<string, string>) => Promise<void> | void;

const CRED_TTL_MS = 4 * 60_000;

/** The pulse credential from the identity script, cached a few minutes; never logged. */
export function pulseCredential(root = process.env.CHORUS_HOME || path.resolve(__dirname, '../../..')): () => Promise<string> {
  let cached: { value: string; at: number } | undefined;
  return () => {
    if (cached && Date.now() - cached.at < CRED_TTL_MS) return Promise.resolve(cached.value);
    return new Promise((resolve, reject) => {
      execFile(path.join(root, 'platform/scripts/chorus-identity-token'), ['pulse'], { timeout: 10_000 }, (err, out) => {
        const value = String(out).trim();
        if (err || !value) return reject(new Error(`identity script for pulse: ${err ? err.message.split('\n')[0] : 'empty output'}`));
        cached = { value, at: Date.now() };
        resolve(value);
      });
    });
  };
}

export function httpSend(base = process.env.PULSE_API_URL || 'http://127.0.0.1:3392'): Send {
  return async (method, p, body, credential) => {
    try {
      const res = await fetch(`${base}${p}`, {
        method,
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${credential}` },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(4000),
      });
      return res.status;
    } catch {
      return 0;
    }
  };
}

/** Every 30 s, after the spine facts refresh: the tree's rows, changes only. Off unless PULSE_AWARENESS_WRITE=1. */
export function startAwarenessWriter(
  store: LineStore,
  actorState: { getActorStates: () => unknown } | undefined,
  spine: { get: () => SpineLine[] },
  emit: Emit,
  writer = new AwarenessWriter(httpSend(), pulseCredential()),
): NodeJS.Timeout | undefined {
  if (process.env.PULSE_AWARENESS_WRITE !== '1') return undefined;
  // #4757 — each WIP card's step, read once a minute from /cws's own lines; a failed read
  // keeps the last rows and says so on the spine, never writes an empty board
  let cardSteps: Row[] = [];
  const steps = async () => {
    try {
      cardSteps = cardStepRows(await readCardSteps(), await readLiveSteps());
    } catch (e) {
      await emit('awareness.cardsteps.unread', { reason: (e instanceof Error ? e.message : String(e)).slice(0, 200) });
    }
  };
  const stepsTimer = setInterval(() => void steps(), 60_000);
  stepsTimer.unref();
  void steps();
  const tick = async () => {
    const now = new Date().toISOString();
    const since = new Date(Date.parse(now) - 6 * 3600_000).toISOString().replace('T', ' ').slice(0, 19);
    const utc = (t: string) => (/[zZ]|[+-]\d\d:?\d\d$/.test(t) ? new Date(t).toISOString() : new Date(t.replace(' ', 'T') + 'Z').toISOString());
    const lines = store.queryMessages({ type: 'jeff-input', since, limit: 500 }).map(m => ({
      id: String(m.id), from: m.from, to: m.to ? [m.to] : [], text: m.content, sentAt: utc(m.created_at ?? m.createdAt ?? ''),
    }));
    const { actors, lastCalls } = factsFromActorState(actorState?.getActorStates());
    const retired = await writer.retireRoleNamed(actors);
    if (retired.length) await emit('awareness.rolestates.retired', { rows: retired.join(', ') });
    const r = await writer.write({ ...awarenessRows({ now, actors, lines, spine: spine.get(), lastCalls }), cardSteps });
    if (r.made || r.updated || r.failed) {
      await emit('awareness.rows.written', {
        made: String(r.made), updated: String(r.updated), failed: String(r.failed), settled: String(r.settled),
        ...(r.failures.length ? { failures: failureCounts(r.failures) } : {}),
      });
    }
  };
  const timer = setInterval(() => void tick().catch(() => undefined), 30_000);
  timer.unref();
  void tick().catch(() => undefined);
  return timer;
}
