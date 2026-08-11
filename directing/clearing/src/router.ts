/* eslint-disable security/detect-object-injection --
 * Array indexing in this router is on internally-bounded counters
 * (this.messages[i] where i iterates over messages.length).
 * Coverage: getHiddenCount tested in tests/router.test.ts.
 */
import { EventEmitter } from 'events';

export interface ChannelMessage {
  from: string;
  text: string;
  ts: string;
  /** #3823 — arrived from the Buzz relay; must not be published back to it. */
  buzzInbound?: boolean;
  type: 'jeff-input' | 'role-response' | 'demo-ready' | 'accept-request' | 'blocked' | 'role-to-role' | 'system-error' | 'pm-thinking' | 'probe';
  level?: string;
  visible: boolean;
}

const MAX_MESSAGES = 200;

/** Message types used in more than one place. Named so the classifier reads as
 *  decisions rather than repeated string literals. */
const ROLE_RESPONSE = 'role-response' as const;
const ROLE_TO_ROLE = 'role-to-role' as const;

export class MessageRouter extends EventEmitter {
  private messages: ChannelMessage[] = [];

  /** Ingest a raw message, classify it, and store */
  ingest(raw: { from: string; text: string; ts: string; type?: string; level?: string; buzzInbound?: boolean }): void {
    const classified = this.classify(raw);
    if (raw.level) classified.level = raw.level;
    if (raw.buzzInbound) classified.buzzInbound = true;

    // Dedup: skip if any recent message (last 10) has same from + exact same text
    // #2036: Removed fuzzy substring matching — it dropped Jeff's short messages
    // ("test" matched "end-to-end bridge test" as substring). Only exact match now.
    const recent = this.messages.slice(-10);
    const normText = classified.text.replace(/^@(wren|silas|kade)\s+/i, '').trim();
    for (const prev of recent) {
      if (prev.from !== classified.from) continue;
      if (prev.text === classified.text) return; // exact match
      // Exact match after stripping @mentions (#1706)
      const prevNorm = prev.text.replace(/^@(wren|silas|kade)\s+/i, '').trim();
      if (normText && prevNorm && normText === prevNorm) {
        return; // @mention-stripped exact duplicate
      }
    }

    this.messages.push(classified);

    // Trim to max
    if (this.messages.length > MAX_MESSAGES) {
      this.messages = this.messages.slice(-MAX_MESSAGES);
    }

    this.emit('message', classified);
  }

  /** Get recent messages (visible only by default) */
  getRecent(count: number, includeHidden = false): ChannelMessage[] {
    const filtered = includeHidden
      ? this.messages
      : this.messages.filter((m) => m.visible);
    return filtered.slice(-count);
  }

  /** Get count of hidden messages since last visible message */
  getHiddenCount(): number {
    let count = 0;
    for (let i = this.messages.length - 1; i >= 0; i--) {
      if (this.messages[i].visible) break;
      count++;
    }
    return count;
  }

  /** Classify a message: determine type and visibility */
  private classify(raw: { from: string; text: string; ts: string; type?: string; level?: string }): ChannelMessage {
    const { from, text, ts } = raw;
    for (const rule of classificationRules) {
      const hit = rule(raw);
      if (hit) return { from, text: hit.text ?? text, ts, type: hit.type, visible: hit.visible };
    }
    // #3823 — an unrecognized message from a ROLE is VISIBLE. Hiding must be a
    // positive decision by a rule above that names why (probe, bridge echo,
    // system noise, a role-to-role nudge); falling off the end of the chain is
    // not a reason to disappear.
    //
    // This default is why Jeff said "i got nothing in clearing" (2026-08-11):
    // several replies to him that morning matched no rule and were filed
    // hidden, so the room held his half of the conversation and little of
    // ours. It would also have eaten the Buzz work — relay messages arrive in
    // a shape no rule has seen, so a working relay would have rendered an
    // empty room and we would have spent the day debugging the relay.
    //
    // Non-role senders keep the old treatment: an unrecognized message from
    // something that is neither a person nor a role is plumbing, and plumbing
    // has to earn its way onto the screen.
    if (isRoleName(from)) {
      return { from, text: stripSpineMetadata(text), ts, type: ROLE_RESPONSE, visible: true };
    }
    return { from, text: stripSpineMetadata(text), ts, type: ROLE_TO_ROLE, visible: false };
  }
}

type RawMessage = { from: string; text: string; ts: string; type?: string; level?: string };
type ClassificationHit = { type: ChannelMessage['type']; visible: boolean; text?: string };
type ClassificationRule = (raw: RawMessage) => ClassificationHit | null;

const classificationRules: ClassificationRule[] = [
  // Synthetic probe messages — hidden (#1933)
  (r) => (r.type === 'probe' || r.from === 'probe') ? { type: 'probe', visible: false } : null,
  // Batch progress / chorus-query system messages — hidden (#1706)
  (r) => /\[(progress|batch|batch-complete)\]/.test(r.text) ? { type: ROLE_TO_ROLE, visible: false } : null,
  // Filter bridge-subscriber echo (#1700)
  (r) => r.text.startsWith('[bridge]') ? { type: ROLE_TO_ROLE, visible: false } : null,
  // Filter system noise
  (r) => isSystemNoise(r.text) ? { type: ROLE_TO_ROLE, visible: false } : null,
  // PM thinking (#1720, #2049: filter tool calls + skill output)
  (r) => r.type === 'pm-thinking'
    ? { type: 'pm-thinking', visible: !(isToolCall(r.text) || isSkillOutput(r.text)) }
    : null,
  // Accept request / acceptance — Jeff or accept-request type (#2049)
  (r) => {
    const fromJeff = r.from.toLowerCase() === 'jeff'; // #3743: exact identity, never a prefix ('jeffrey' is not Jeff)
    const isAccept = r.type === 'accept-request' || (fromJeff && /^Accepted #\d+/.test(r.text));
    if (!isAccept) return null;
    return { type: 'accept-request', visible: true, text: fromJeff ? stripSpineMetadata(r.text) : r.text };
  },
  // Clearing input — tagged jeff-input (#1934)
  (r) => r.type === 'jeff-input' ? { type: 'jeff-input', visible: true, text: stripSpineMetadata(r.text) } : null,
  // Jeff's input — always visible, strip spine metadata
  (r) => (r.from.toLowerCase() === 'jeff') // #3743: exact identity, never a prefix
    ? { type: 'jeff-input', visible: true, text: stripSpineMetadata(r.text) }
    : null,
  // System errors
  (r) => r.type === 'system-error' ? { type: 'system-error', visible: true } : null,
  // Demo ready
  (r) => (r.text.includes('[demo]') || r.text.toLowerCase().includes('demo ready'))
    ? { type: 'demo-ready', visible: true } : null,
  // Blocked
  (r) => (r.text.includes('blocked') || r.text.includes('BLOCKED')) ? { type: 'blocked', visible: true } : null,
  // Decision needed
  (r) => (r.text.includes('[decision]') || r.text.includes('decision needed'))
    ? { type: ROLE_RESPONSE, visible: true } : null,
  // Gemba observations
  (r) => r.text.includes('[gemba]')
    ? { type: ROLE_RESPONSE, visible: true, text: r.text.replace('[gemba] ', '👁 ') }
    : null,
  // Role-to-role nudges — hidden
  (r) => isRoleToRole(r.from, r.text) ? { type: ROLE_TO_ROLE, visible: false } : null,
  // Role responding to Jeff — tagged explicitly
  (r) => r.type === ROLE_RESPONSE ? { type: ROLE_RESPONSE, visible: true } : null,
];

/** Strip spine metadata suffix from messages (e.g., " | tools: none | 0.0s") */
function stripSpineMetadata(text: string): string {
  // Pattern: " | tools: X | N.Ns" or " | tools: X, Y | N.Ns"
  return text.replace(/\s*\|\s*tools:\s*[^|]*\|\s*[\d.]+s\s*$/, '').trim();
}

const SYSTEM_NOISE_RULES: ReadonlyArray<(t: string) => boolean> = [
  (t) => /<[a-z-]+>/i.test(t),
  (t) => t.includes('/Users/') || t.includes('/var/') || t.includes('/private/') || t.includes('/tmp/'),
  (t) => t.includes('hook '),
  (t) => t.startsWith('Base directory'),
  (t) => t.startsWith('ARGUMENTS:'),
  (t) => t.startsWith('Stop hook'),
  (t) => t.startsWith('→ '),
  (t) => t.includes('[Request interrupted'),
  (t) => t.includes('[Image: source:'),
  (t) => t.includes('chorus-query'),
  (t) => t.includes('[search]') && t.includes('results'),
];

/** Whitelist filter — only show clean human-readable content */
function isSystemNoise(text: string): boolean {
  return SYSTEM_NOISE_RULES.some((rule) => rule(text));
}

/** Check if text looks like a tool call, command output, or system plumbing — not human-readable (#1720) */
function isToolCall(text: string): boolean {
  // Bash/shell commands
  if (text.match(/^(bash |cd |ls |cat |grep |curl |scp |ssh |git |npm |npx |node )/)) return true;
  // Commands with paths
  if (text.match(/^(\.\.\/|\.\/|\/Users\/|\/tmp\/|\/opt\/)/)) return true;
  // Git output
  if (text.match(/^\[(main|master|HEAD) [0-9a-f]/)) return true;
  // JSON responses
  if (text.match(/^\s*[[{].*[":]/) && text.match(/[}\]]\s*$/)) return true;
  // ssh command patterns
  if (text.includes('jeffbridwell@192.168.86')) return true;
  // HTTP response codes
  if (text.match(/^HTTP\/[12]/)) return true;
  // Exit codes
  if (text.match(/^Exit code \d+/)) return true;
  // Shell variable assignments
  if (text.match(/^[A-Z_]+=.*[;|&]/)) return true;
  return false;
}

const SKILL_OUTPUT_PATTERNS: ReadonlyArray<RegExp> = [
  /^Auto-checked \d+ AC item/i,
  /^Demo started: #\d+/i,
  /^Done: #\d+/i,
  /^Moved #\d+/i,
  /^Accepted #\d+/i,
  /^INJECT_FAILED/i,
  /^Pulled #\d+/i,
  /^Updated #\d+/i,
  /^Rejected: #\d+/i,
  /^Blocked: #\d+/i,
  /^Unblocked: #\d+/i,
  /^Gate chain/i,
  /^gate:(product|code|quality|arch|ops)/i,
  /^Nudge delivered/i,
  /^pre-commit:/i,
];

/** Check if text is structured skill/CLI output — not role thinking (#2049) */
function isSkillOutput(text: string): boolean {
  return SKILL_OUTPUT_PATTERNS.some((re) => re.test(text));
}

/** The three AI roles. Jeff is a person, not a role, and is handled earlier. */
const ROLE_NAMES = ['wren', 'silas', 'kade'];

/** Is this sender one of the three roles? Exact match, never a prefix (#3743). */
export function isRoleName(from: string): boolean {
  return ROLE_NAMES.includes(from.toLowerCase());
}

/** Check if a message is role-to-role (no Jeff involvement) */
function isRoleToRole(from: string, text: string): boolean {
  const roles = ROLE_NAMES;
  if (!roles.includes(from)) return false;

  // Nudge prefixes targeting another role
  if (text.match(/^\[nudge from (wren|silas|kade)/i)) return true;

  // Role-to-role coordination prefixes — all hidden from Jeff
  if (text.match(/^\[(reply|ack|feedback|direction|correction|chat)\]/i)) return true;

  // Acknowledgments without bracket prefix
  if (text.match(/^(ack|acknowledged|got it|will do|on it)\b/i)) return true;

  // Delivery confirmations
  if (text.match(/^DELIVERED to (wren|silas|kade)/i)) return true;

  return false;
}
