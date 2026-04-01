import { EventEmitter } from 'events';

export interface ChannelMessage {
  from: string;
  text: string;
  ts: string;
  type: 'jeff-input' | 'role-response' | 'demo-ready' | 'accept-request' | 'blocked' | 'role-to-role' | 'system-error' | 'pm-thinking' | 'probe';
  level?: string;
  visible: boolean;
}

const MAX_MESSAGES = 200;

export class MessageRouter extends EventEmitter {
  private messages: ChannelMessage[] = [];

  /** Ingest a raw message, classify it, and store */
  ingest(raw: { from: string; text: string; ts: string; type?: string; level?: string }): void {
    const classified = this.classify(raw);
    if (raw.level) classified.level = raw.level;

    // Dedup: skip if any recent message (last 10) has same from + text
    // Also catch @mention-stripped duplicates — Bridge sends "@silas do X",
    // session-tailer sees "do X" without the prefix (#1706)
    const recent = this.messages.slice(-10);
    const normText = classified.text.replace(/^@(wren|silas|kade)\s+/i, '').trim();
    for (const prev of recent) {
      if (prev.from !== classified.from) continue;
      if (prev.text === classified.text) return; // exact match
      // Fuzzy: one is substring of the other after stripping @mentions
      const prevNorm = prev.text.replace(/^@(wren|silas|kade)\s+/i, '').trim();
      if (normText && prevNorm && (normText === prevNorm || prev.text.includes(normText) || classified.text.includes(prevNorm))) {
        return; // @mention-stripped duplicate
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

    // Synthetic probe messages — hidden (#1933)
    if (raw.type === 'probe' || from === 'probe') {
      return { from, text, ts, type: 'probe', visible: false };
    }

    // Batch progress / chorus-query system messages — hidden from message stream (#1706)
    if (text.includes('[progress]') || text.includes('[batch]') || text.includes('[batch-complete]')) {
      return { from, text, ts, type: 'role-to-role', visible: false };
    }

    // Filter bridge-subscriber echo — these are duplicates of events already shown (#1700)
    if (text.startsWith('[bridge]')) {
      return { from, text, ts, type: 'role-to-role', visible: false };
    }

    // Filter system noise — suppress raw protocol artifacts
    if (isSystemNoise(text)) {
      return { from, text, ts, type: 'role-to-role', visible: false };
    }

    // PM thinking — role commentary appears in messages, tool calls filtered (#1720)
    if (raw.type === 'pm-thinking') {
      if (isToolCall(text)) {
        return { from, text, ts, type: 'pm-thinking', visible: false };
      }
      return { from, text, ts, type: 'pm-thinking', visible: true };
    }

    // Accept request / acceptance — always visible (check before jeff-input so acceptance gets styled)
    if (raw.type === 'accept-request' || text.includes('/acp') || text.includes('ready for accept') || text.includes('ready for Jeff') || text.match(/^Accepted #\d+/)) {
      const cleanText = from === 'jeff' ? stripSpineMetadata(text) : text;
      return { from, text: cleanText, ts, type: 'accept-request', visible: true };
    }

    // Jeff's input is always visible — strip spine metadata suffix
    if (from === 'jeff') {
      const cleanText = stripSpineMetadata(text);
      return { from, text: cleanText, ts, type: 'jeff-input', visible: true };
    }

    // System errors always visible
    if (raw.type === 'system-error') {
      return { from, text, ts, type: 'system-error', visible: true };
    }

    // Demo ready — always visible
    if (text.includes('[demo]') || text.toLowerCase().includes('demo ready')) {
      return { from, text, ts, type: 'demo-ready', visible: true };
    }

    // Note: accept-request check moved above jeff-input to ensure styling

    // Blocked — always visible
    if (text.includes('blocked') || text.includes('BLOCKED')) {
      return { from, text, ts, type: 'blocked', visible: true };
    }

    // Decision needed — always visible
    if (text.includes('[decision]') || text.includes('decision needed')) {
      return { from, text, ts, type: 'role-response', visible: true };
    }

    // Gemba observations — always visible
    if (text.includes('[gemba]')) {
      return { from, text: text.replace('[gemba] ', '👁 '), ts, type: 'role-response', visible: true };
    }

    // Role-to-role nudges — hidden (check BEFORE role-response to catch coordination noise)
    if (isRoleToRole(from, text)) {
      return { from, text, ts, type: 'role-to-role', visible: false };
    }

    // Role responding to Jeff — visible (must be explicitly tagged AND not caught by role-to-role)
    if (raw.type === 'role-response') {
      return { from, text, ts, type: 'role-response', visible: true };
    }

    // Default: HIDDEN — whitelist only. If not explicitly matched above, Jeff doesn't see it.
    return { from, text: stripSpineMetadata(text), ts, type: 'role-to-role', visible: false };
  }
}

/** Strip spine metadata suffix from messages (e.g., " | tools: none | 0.0s") */
function stripSpineMetadata(text: string): string {
  // Pattern: " | tools: X | N.Ns" or " | tools: X, Y | N.Ns"
  return text.replace(/\s*\|\s*tools:\s*[^|]*\|\s*[\d.]+s\s*$/, '').trim();
}

/** Whitelist filter — only show clean human-readable content */
function isSystemNoise(text: string): boolean {
  // Whitelist approach: if it contains ANY XML-like tag or system artifact, it's noise
  if (/<[a-z-]+>/i.test(text)) return true;       // Any XML tags
  if (text.includes('/Users/') || text.includes('/var/') || text.includes('/private/') || text.includes('/tmp/')) return true; // File paths
  if (text.includes('hook ')) return true;          // Hook output
  if (text.startsWith('Base directory')) return true;
  if (text.startsWith('ARGUMENTS:')) return true;
  if (text.startsWith('Stop hook')) return true;
  if (text.startsWith('→ ')) return true;           // Nudge delivery echo
  if (text.includes('[Request interrupted')) return true;
  if (text.includes('[Image: source:')) return true;
  if (text.includes('chorus-query')) return true;       // chorus-query.sh output (#1706)
  if (text.includes('[search]') && text.includes('results')) return true; // search progress
  return false;
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
  if (text.match(/^\s*[\[{].*[":]/) && text.match(/[}\]]\s*$/)) return true;
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

/** Check if a message is role-to-role (no Jeff involvement) */
function isRoleToRole(from: string, text: string): boolean {
  const roles = ['wren', 'silas', 'kade'];
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
