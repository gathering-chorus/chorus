// @domain: messages
/**
 * #4637 AC1/AC2 — only real actors send.
 *
 * Measured 2026-10-24 (04:00Z to 00:38Z): of 961 messages, 285 came from
 * senders that are not actors — `system` 253 (alerts, failed pipelines, the
 * pair heartbeat, deep-health), `pulse` 2, and Borg Queen 30 (an actor since
 * Jeff's yes, 10-24 20:51). Jeff (10-23 09:54): only real actors send.
 *
 * The actors are the roles door's agent and human roles (peers.ts). A machine
 * sender (system, pulse, an alert source) is not refused, or its alert would
 * go silent: the message is attributed to the role that owns it — the
 * caller's `owner`, else the role it is addressed to — and marked as an alert,
 * never a nudge from "system". Any other sender (bridge, unattributed,
 * chorus-mcp, empty) is refused by name and nothing is stored.
 */

/** Senders that are machines raising an alert on a role's behalf. */
export const MACHINE_SENDERS = ['system', 'pulse', 'alert', 'grafana', 'herald', 'sensor', 'ops'] as const;

export type Sender =
  | { kind: 'actor'; from: string }
  | { kind: 'alert'; from: string; source: string }
  | { kind: 'refused'; status: number; body: { error: string; message: string } };

export function attributeSender(from: unknown, to: unknown, owner: unknown, actors: readonly string[]): Sender {
  const f = typeof from === 'string' ? from.trim() : '';
  const t = typeof to === 'string' ? to.trim() : '';
  const isActor = (x: string) => x !== '' && (x === 'jeff' || actors.includes(x));
  if (isActor(f)) return { kind: 'actor', from: f };
  if ((MACHINE_SENDERS as readonly string[]).includes(f)) {
    const o = typeof owner === 'string' ? owner.trim() : '';
    const who = isActor(o) ? o : isActor(t) ? t : '';
    if (who) return { kind: 'alert', from: who, source: f };
    return refused(`an alert from '${f}' names no owning role (owner '${o}', to '${t}'); nothing was stored`);
  }
  return refused(`'${f || '(none)'}' is not an actor (${['jeff', ...actors].join(', ')}); a message needs a real sender, so nothing was stored`);
}

function refused(message: string): Sender {
  return { kind: 'refused', status: 422, body: { error: 'not-an-actor', message } };
}

/** The alert's own header: whose it is and where it came from, never "nudge from system". */
export function markAlert(owner: string, source: string, content: string, tsBoston: string): string {
  const body = content.replace(/^\[nudge from [^\]]*\]\s*/, '');
  return `[alert for ${owner} · from ${source} | ${tsBoston} Boston] ${body}`;
}

/**
 * #4637 AC3 — every message carries where it was written: a device (the
 * Library machine, a phone, a desktop browser) and an app (Clearing v2, a
 * terminal, Antigravity, VS Code, or an alert source). A caller that knows
 * says so (`where`, e.g. Clearing v2 from the browser's user agent); a role's
 * own nudge is written on the Library in its runtime's app. A value that is
 * not of this shape is not trusted; the default is named, never blank.
 */
const WHERE = /^(library|bedroom|phone|desktop|unknown)\/(clearing|clearing-v2|terminal|antigravity|vscode|alert|pipeline)$/;
/** Roles whose sessions run in Antigravity (the Gemini runtime), not a Claude terminal. */
const ANTIGRAVITY_ROLES = new Set(['abby-normal', 'borg-queen']);

export function originFor(kind: 'nudge' | 'alert' | 'jeff-input', from: string, where?: unknown): string {
  if (typeof where === 'string' && WHERE.test(where)) return where;
  if (kind === 'alert') return 'library/alert';
  if (kind === 'jeff-input') return 'unknown/clearing';
  return `library/${ANTIGRAVITY_ROLES.has(from) ? 'antigravity' : 'terminal'}`;
}
