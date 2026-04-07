import fs from 'fs';
import { EventEmitter } from 'events';
import { MessageRouter } from './router';

const CHORUS_ROOT = process.env.CHORUS_ROOT || '/Users/jeffbridwell/CascadeProjects';
const CHORUS_LOG = `${CHORUS_ROOT}/platform/logs/chorus.log`;
const POLL_INTERVAL = 2000; // 2 seconds

/**
 * Tail the chorus log for jeff-facing events.
 * Converts relevant spine events into command channel messages.
 */
export class ChorusLogTailer extends EventEmitter {
  private router: MessageRouter;
  private lastSize = 0;
  private timer: NodeJS.Timeout | null = null;

  constructor(router: MessageRouter) {
    super();
    this.router = router;
  }

  start(): void {
    // Start from current end of file
    try {
      const stats = fs.statSync(CHORUS_LOG);
      this.lastSize = stats.size;
    } catch {
      this.lastSize = 0;
    }

    this.timer = setInterval(() => this.poll(), POLL_INTERVAL);
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer);
  }

  private poll(): void {
    let stats;
    try {
      stats = fs.statSync(CHORUS_LOG);
    } catch {
      return;
    }

    if (stats.size <= this.lastSize) return;

    // Read new bytes
    const fd = fs.openSync(CHORUS_LOG, 'r');
    const buf = Buffer.alloc(stats.size - this.lastSize);
    fs.readSync(fd, buf, 0, buf.length, this.lastSize);
    fs.closeSync(fd);
    this.lastSize = stats.size;

    const newLines = buf.toString('utf-8').split('\n').filter(Boolean);

    for (const line of newLines) {
      this.processLine(line);
    }
  }

  private processLine(line: string): void {
    let parsed;
    try {
      parsed = JSON.parse(line);
    } catch {
      return;
    }

    const event = parsed.event || '';
    const role = parsed.role || '';

    // Demo started — always surface
    if (event === 'card.demo.started') {
      const card = parsed.card || parsed.card_id || '';
      const title = parsed.title || '';
      this.router.ingest({
        from: role,
        text: `Demo ready: #${card}${title ? ` — ${title}` : ''}`,
        ts: parsed.timestamp || new Date().toISOString(),
        type: 'demo-ready',
      });
      return;
    }

    // Card accepted — surface + emit board event
    if (event === 'card.accepted') {
      const card = parsed.card_id || parsed.card || '';
      const title = parsed.title || '';
      const acceptor = parsed.acceptor || 'jeff';
      this.router.ingest({
        from: acceptor,
        text: `Accepted #${card}${title ? ` — ${title}` : ''}`,
        ts: parsed.timestamp || new Date().toISOString(),
        type: 'accept-request',
      });
      this.emit('board-event', { type: 'card.accepted', card, role: acceptor, builder: role, ts: parsed.timestamp });
      return;
    }

    // Card pulled — emit board event (#1681)
    if (event === 'card.pulled') {
      const card = parsed.card_id || parsed.card || '';
      this.emit('board-event', { type: 'card.pulled', card, role, ts: parsed.timestamp });
      return;
    }

    // Role state change — emit event + surface blocked (#1681)
    if (event === 'role.state.changed') {
      this.emit('board-event', {
        type: 'role.state.changed',
        role,
        state: parsed.state,
        card: parsed.card || '',
        ts: parsed.timestamp,
      });
      if (parsed.state === 'blocked') {
        this.router.ingest({
          from: role,
          text: `BLOCKED: ${parsed.detail || 'no detail'}`,
          ts: parsed.timestamp || new Date().toISOString(),
          type: 'blocked',
        });
      }
      return;
    }

    // JDI received — surface (Jeff said it, show confirmation)
    if (event === 'interaction.jdi.received') {
      this.router.ingest({
        from: 'system',
        text: `JDI signal received by ${role}${parsed.card ? ` [#${parsed.card}]` : ''}`,
        ts: parsed.timestamp || new Date().toISOString(),
        type: 'role-response',
      });
      return;
    }

    // Nudge sent — only surface nudges TO jeff
    if (event === 'role.nudge.sent') {
      const target = parsed.target?.split(',')[0] || '';
      const content = parsed.target?.match(/content=(.+)/)?.[1] || '';
      if (target === 'jeff' && content) {
        this.router.ingest({
          from: role,
          text: content,
          ts: parsed.timestamp || new Date().toISOString(),
          type: 'role-response',
        });
      }
      return;
    }

    // Session turns — handled by SessionTailer (reads JSONL directly for user/assistant distinction)
    // Kept here only as fallback if session tailer can't find session files
  }
}
