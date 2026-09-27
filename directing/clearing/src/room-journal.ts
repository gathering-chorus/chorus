/* eslint-disable security/detect-non-literal-fs-filename -- the journal path is fixed at startup from CLEARING_JOURNAL (or the default under ~/.chorus/clearing) */
/**
 * #4363 — the room's full history, one line per message, keyed by its id.
 *
 * Replaces the /tmp/bridge-messages.json snapshot (200 rows, rewritten every
 * 10s, gone on reboot). Append-only: a message is written once, when the
 * router admits it. The API pages backwards from any id, so the whole
 * history is reachable, not just the last window.
 */
import fs from 'fs';
import path from 'path';
import type { ChannelMessage } from './router';

export class RoomJournal {
  constructor(private file: string) {
    fs.mkdirSync(path.dirname(file), { recursive: true });
  }

  append(msg: ChannelMessage): void {
    fs.appendFileSync(this.file, JSON.stringify(msg) + '\n');
  }

  private all(): ChannelMessage[] {
    let text: string;
    try { text = fs.readFileSync(this.file, 'utf8'); } catch { return []; }
    const out: ChannelMessage[] = [];
    for (const line of text.split('\n')) {
      if (!line) continue;
      try { out.push(JSON.parse(line) as ChannelMessage); } catch { /* a torn last line is skipped, never fatal */ }
    }
    return out;
  }

  total(): number { return this.all().length; }

  /** Up to `limit` messages older than `beforeId` (the newest when absent), oldest first. */
  page(beforeId: string | undefined, limit: number): ChannelMessage[] {
    const rows = this.all();
    const end = beforeId === undefined ? rows.length : rows.findIndex((m) => m.id === beforeId);
    if (end < 0) return [];
    return rows.slice(Math.max(0, end - limit), end);
  }
}
