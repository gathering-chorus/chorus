/* eslint-disable security/detect-non-literal-fs-filename -- the one file path is fixed by config (CLEARING_V2_HANDOFF_FILE or ~/.chorus), never by a request */
// @domain: messages
// #4652 — which of Jeff's messages the room has already handed to pulse. The relay
// replays #team's history on every connect, and Jeff's Buzz app posts are signed by
// him, so without this every restart would deliver them again. Posts older than the
// room's first start are history and are never handed off (Silas 10-20: never act
// on history). Bounded; written on each hand-off.
import fs from 'fs';
import os from 'os';
import path from 'path';

export const DEFAULT_HANDOFF_FILE = path.join(os.homedir(), '.chorus', 'clearing-v2-handoffs.json');

export class HandoffLog {
  private readonly ids: string[] = [];
  private readonly since: number;

  constructor(private readonly file?: string, nowSec = Math.floor(Date.now() / 1000), private readonly limit = 1000) {
    let saved: { since?: unknown; ids?: unknown } = {};
    if (file) {
      try {
        saved = JSON.parse(fs.readFileSync(file, 'utf8'));
      } catch (e) {
        if ((e as NodeJS.ErrnoException).code !== 'ENOENT') console.error(`[clearing-v2] hand-off log unreadable, starting from now: ${(e as Error).message}`);
      }
    }
    this.since = typeof saved.since === 'number' ? saved.since : nowSec;
    if (Array.isArray(saved.ids)) this.ids.push(...saved.ids.filter((x): x is string => typeof x === 'string'));
    if (file && typeof saved.since !== 'number') this.save();
  }

  /** True once per message id, and only for messages written since the room first started. */
  claim(id: string, createdAt: number): boolean {
    if (createdAt < this.since || this.ids.includes(id)) return false;
    this.ids.push(id);
    if (this.ids.length > this.limit) this.ids.splice(0, this.ids.length - this.limit);
    this.save();
    return true;
  }

  private save(): void {
    if (!this.file) return;
    try {
      fs.mkdirSync(path.dirname(this.file), { recursive: true });
      fs.writeFileSync(`${this.file}.tmp`, JSON.stringify({ since: this.since, ids: this.ids }));
      fs.renameSync(`${this.file}.tmp`, this.file);
    } catch (e) {
      console.error(`[clearing-v2] hand-off log not saved: ${(e as Error).message}`);
    }
  }
}
