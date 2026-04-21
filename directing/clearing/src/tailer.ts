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

  private handleDemoStarted(parsed: any, role: string): void {
    const card = parsed.card || parsed.card_id || '';
    const title = parsed.title || '';
    this.router.ingest({
      from: role,
      text: `Demo ready: #${card}${title ? ` — ${title}` : ''}`,
      ts: parsed.timestamp || new Date().toISOString(),
      type: 'demo-ready',
    });
  }

  private handleCardAccepted(parsed: any, role: string): void {
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
  }

  private handleRoleStateChanged(parsed: any, role: string): void {
    this.emit('board-event', {
      type: 'role.state.changed',
      role, state: parsed.state, card: parsed.card || '', ts: parsed.timestamp,
    });
    if (parsed.state === 'blocked') {
      this.router.ingest({
        from: role,
        text: `BLOCKED: ${parsed.detail || 'no detail'}`,
        ts: parsed.timestamp || new Date().toISOString(),
        type: 'blocked',
      });
    }
  }

  private handleNudgeSent(parsed: any, role: string): void {
    const target = parsed.target?.split(',')[0] || '';
    const content = parsed.target?.match(/content=(.+)/)?.[1] || '';
    if (target !== 'jeff' || !content) return;
    this.router.ingest({
      from: role,
      text: content,
      ts: parsed.timestamp || new Date().toISOString(),
      type: 'role-response',
    });
  }

  private processLine(line: string): void {
    let parsed: any;
    try { parsed = JSON.parse(line); } catch { return; }

    const event = parsed.event || '';
    const role = parsed.role || '';

    switch (event) {
      case 'card.demo.started':
        return this.handleDemoStarted(parsed, role);
      case 'card.accepted':
        return this.handleCardAccepted(parsed, role);
      case 'card.pulled':
        this.emit('board-event', { type: 'card.pulled', card: parsed.card_id || parsed.card || '', role, ts: parsed.timestamp });
        return;
      case 'role.state.changed':
        return this.handleRoleStateChanged(parsed, role);
      case 'interaction.jdi.received':
        this.router.ingest({
          from: 'system',
          text: `JDI signal received by ${role}${parsed.card ? ` [#${parsed.card}]` : ''}`,
          ts: parsed.timestamp || new Date().toISOString(),
          type: 'role-response',
        });
        return;
      case 'role.nudge.sent':
        return this.handleNudgeSent(parsed, role);
    }
  }
}
