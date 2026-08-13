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

  /**
   * #3852 — the same window, plus what it left out.
   *
   * The room served 50 rows and said nothing about the rest. On a day where I
   * sent 134 replies, 5 were reachable and 129 were silently gone — which reads
   * exactly like messages being dropped, and is why Jeff kept saying he was
   * losing them. Nothing WAS dropped; the window never admitted it was a window.
   *
   * Truncation that does not announce itself is indistinguishable from data
   * loss, and the reader has no way to tell which one they are looking at.
   */
  getRecentWindowed(count: number, includeHidden = false): {
    messages: ChannelMessage[];
    total: number;
    withheld: number;
  } {
    const filtered = includeHidden
      ? this.messages
      : this.messages.filter((m) => m.visible);
    const messages = filtered.slice(-count);
    return { messages, total: filtered.length, withheld: filtered.length - messages.length };
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
  // #3862 — machinery, from any sender. These three shapes rendered as chat
  // bubbles on Jeff's phone today; the last one arrived tagged as HIS OWN
  // message, in his own colour, because an MCP refusal is echoed back on the
  // jeff-input path. Machinery is hidden by NAME here, so that the broad
  // isSystemNoise sweep below can be narrowed to plumbing.
  (r) => isMachineryEcho(r.text) ? { type: 'system-error', visible: false } : null,
  // #3862 — system noise applies to PLUMBING only. This rule used to run against
  // every sender, and one of its tests is "text contains a filesystem path", so
  // any reply that quoted a path silently disappeared from Jeff's room. A role's
  // own words are never "noise": what a role says to Jeff is the product.
  (r) => (!isRoleName(r.from) && !isJeff(r.from) && isSystemNoise(r.text))
    ? { type: ROLE_TO_ROLE, visible: false } : null,
  // #3852 — mid-turn thinking is ALWAYS folded. Jeff, 2026-08-13: the folding
  // "is also unreliable in how it works" — and he was right, because it decided
  // by CONTENT: a line was hidden only if it looked like a tool call or skill
  // output. Heuristics on text drift, so some narration showed and some didn't,
  // with no rule a person could predict.
  //
  // It decides by STATE now. The transcript already says which is which —
  // stop_reason 'end_turn' is a finished reply, anything else is mid-turn — and
  // the tailer already reads that field for the reply timer. We were guessing at
  // text while holding the answer.
  //
  // The cost of getting this wrong was measured: 18 of 50 room rows were
  // pm-thinking while Jeff's own messages were 12, so his room was mostly us
  // narrating at him.
  //
  // #3862 — and it was still wrong, in the direction that costs the most.
  // `stop_reason` is 'end_turn' only when a turn ends in plain text. A turn that
  // ends after tool calls does not carry it, so a FINISHED REPLY gets tagged
  // mid-turn and folded. Every reply written while doing work goes through
  // tools, so in practice this hid the answers and kept the narration.
  //
  // Measured in the live room at 17:22 Boston: 16 rows tagged pm-thinking,
  // hidden — including two direct answers to a question Jeff had just asked.
  //
  // The type is kept: it still tells the UI this row is lower-signal, and the
  // room can style or collapse it. But collapsing is a rendering decision, and
  // Jeff settled what the current one actually does — "the ui definitely does
  // not show collapsed messages that i can unfold - they literally are NOT
  // THERE". Until a row can be unfolded, hiding is deletion.
  (r) => r.type === 'pm-thinking' ? { type: 'pm-thinking', visible: true } : null,
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
  // #3862 — role-to-role nudges are VISIBLE. Jeff, 2026-08-13: "i dont want to
  // hide any fucking nudges" / "i dont know how we got here where we hide
  // nudges". The room is where the team talks; a nudge is the team talking.
  // The type is kept so the UI can render it as an aside rather than a reply.
  //
  // This rule also caught replies written TO Jeff that merely mentioned another
  // role by name, and filed them hidden. "Kade has streams. #3827 is mine." was
  // an answer to a question he asked, and he never saw it.
  (r) => isRoleToRole(r.from, r.text) ? { type: ROLE_TO_ROLE, visible: true } : null,
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

/**
 * #3862 — machinery that reached Jeff's room as conversation.
 *
 * Each pattern is a shape he was actually shown on his phone on 2026-08-13,
 * not a category we imagined. Kept narrow and named: a broad "looks technical"
 * sweep is what ate the replies in the first place, so this list may only grow
 * by adding a shape someone has SEEN in the room.
 */
function isMachineryEcho(text: string): boolean {
  return (
    /\[e2e-[a-z]+\]/i.test(text) ||                  // e2e test traffic acking itself
    /^API Error: \d{3}/.test(text) ||                // an upstream tool error, verbatim
    /\btype=is-error\b/.test(text) ||                // an MCP refusal echoed back
    /^word-cap: \d+ words/.test(text)                // the cap gate refusing a send
  );
}

/** #3743 — exact identity, never a prefix ('jeffrey' is not Jeff). */
function isJeff(from: string): boolean {
  return from.toLowerCase() === 'jeff';
}

// #3852 — isToolCall DELETED. It decided visibility by inspecting text, which is
// the unreliability Jeff named; folding now reads stop_reason instead. Left in
// #3852 — SKILL_OUTPUT_PATTERNS deleted with isSkillOutput.


// #3852 — isSkillOutput DELETED. It decided visibility by inspecting text, which is
// the unreliability Jeff named; folding now reads stop_reason instead. Left in
// place it would invite a rewire of the same guessing.


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
