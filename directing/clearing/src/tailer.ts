/* eslint-disable security/detect-non-literal-fs-filename --
 * fs reads on CHORUS_LOG, derived from CHORUS_ROOT env constant.
 */
import fs from 'fs';
import { EventEmitter } from 'events';
import { MessageRouter } from './router';

// #2725 — repointed to the LIVE spine. The spine moved to ~/.chorus/chorus.log on
// 2026-05-04 (#3819 class); this tailer kept reading the dead 84KB repo-local copy,
// so nudge bubbles silently never rendered in Jeff's Clearing.
// CHORUS_HOME is ambiguous by convention (the repo in shell envs, ~/.chorus to
// services), so resolve to the FIRST CANDIDATE THAT EXISTS — a path that doesn't
// exist cannot be the never-rotated spine. CHORUS_LOG_FILE (#3615 membrane seam)
// wins outright when set.
function resolveSpinePath(): string {
  const candidates = [
    process.env.CHORUS_LOG_FILE,
    process.env.CHORUS_HOME ? `${process.env.CHORUS_HOME}/chorus.log` : undefined,
    `${process.env.HOME}/.chorus/chorus.log`,
  ].filter((p): p is string => !!p);
  for (const p of candidates) {
    try { fs.statSync(p); return p; } catch { /* next candidate */ }
  }
  return candidates[candidates.length - 1];
}
const CHORUS_LOG = resolveSpinePath();
const POLL_INTERVAL = 2000; // 2 seconds

/** Spine log entry — all fields optional since entries vary by event type. */
interface SpineEntry {
  timestamp?: string;
  event?: string;
  role?: string;
  card?: string | number;
  card_id?: string | number;
  title?: string;
  acceptor?: string;
  state?: string;
  detail?: string;
  from?: string;
  target?: string;
}

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

    // #3606 — unref. This poll timer is the handle that hung the nightly: with it
    // holding Node's loop open, `jest --coverage` (which the nightly runs WITHOUT
    // --forceExit) waited forever, and clearing's coverage step stalled 97+ minutes
    // at suite 1 of ~232 — the whole run produced no data.
    //
    // The timer is background log-tailing: it should never be the reason a process
    // stays alive. `unref()` says exactly that — keep polling while something else
    // holds the loop, never hold it open alone. stop() below remains the explicit
    // teardown for callers that own the lifecycle.
    //
    // Sibling unrefs landed in #3604 for the save/broadcast intervals in server.ts;
    // this one was missed because tailer.ts is started indirectly.
    this.timer = setInterval(() => this.poll(), POLL_INTERVAL);
    this.timer.unref();
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

  private handleDemoStarted(parsed: SpineEntry, role: string): void {
    const card = parsed.card || parsed.card_id || '';
    const title = parsed.title || '';
    this.router.ingest({
      from: role,
      text: `Demo ready: #${card}${title ? ` — ${title}` : ''}`,
      ts: parsed.timestamp || new Date().toISOString(),
      type: 'demo-ready',
    });
  }

  private handleCardAccepted(parsed: SpineEntry, role: string): void {
    const card = parsed.card_id || parsed.card || '';
    const title = parsed.title || '';
    // #3743: an ABSENT acceptor is a refusal to attribute, not a default to the
    // highest authority in the system. 'unattributed' renders honestly and can
    // never satisfy a jeff-authority predicate.
    const acceptor = parsed.acceptor || 'unattributed';
    this.router.ingest({
      from: acceptor,
      text: `Accepted #${card}${title ? ` — ${title}` : ''}`,
      ts: parsed.timestamp || new Date().toISOString(),
      type: 'accept-request',
    });
    this.emit('board-event', { type: 'card.accepted', card, role: acceptor, builder: role, ts: parsed.timestamp });
  }

  private handleRoleStateChanged(parsed: SpineEntry, role: string): void {
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

  // #2435 — canonical event is nudge.emitted. For nudge.emitted, chorus-log
  // packs the first kv ("from=<sender>") as the JSON field, so target + content
  // live inside entry.from. For back-compat during parallel-run the older
  // role.nudge.sent packed them under entry.target; accept both.
  private handleNudgeSent(parsed: SpineEntry, role: string): void {
    // #2725 — the LIVE spine's nudge.emitted (mcp-server appendChorusLog) packs
    // the kv string under `payload`; the older chorus-log CLI shape packed it
    // under `from`/`target`. Repointing to the live file without reading the
    // live field would be the same silent-stale defect one layer down.
    const packed: string = (parsed as { payload?: string }).payload || parsed.from || parsed.target || '';
    // On nudge.emitted: "from" value starts with "<sender>,to=<target>,..."; the
    // target role is after "to=". On role.nudge.sent: "target" value starts with
    // "<target>,chars=..." — first segment is the target.
    const target = packed.match(/(?:^|,)to=([^,]+)/)?.[1]
                || packed.split(',')[0]
                || '';
    const content = packed.match(/content=(.+)/)?.[1] || '';
    if (target !== 'jeff' || !content) return;
    this.router.ingest({
      from: role,
      text: content,
      ts: parsed.timestamp || new Date().toISOString(),
      type: 'role-response',
    });
  }

  private processLine(line: string): void {
    let parsed: SpineEntry;
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
      case 'nudge.emitted':
        return this.handleNudgeSent(parsed, role);
    }
  }
}
