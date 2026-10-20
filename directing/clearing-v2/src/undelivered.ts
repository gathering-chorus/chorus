/* eslint-disable security/detect-non-literal-fs-filename -- the one file path is fixed by config (CLEARING_V2_UNDELIVERED_FILE or ~/.chorus), never by a request */
// @domain: messages
// #4649 — which of Jeff's messages did not reach which role. The page marks them
// "not delivered to <role>"; the server keeps the list so a reload, a second phone
// or a v2 restart shows the same marks (Silas 10-20 05:47: in memory only, a restart
// would turn a lost message back into silent green). Bounded; written on each failure.
import fs from 'fs';
import os from 'os';
import path from 'path';

export const DEFAULT_UNDELIVERED_FILE = path.join(os.homedir(), '.chorus', 'clearing-v2-undelivered.json');

export class UndeliveredLog {
  private readonly byId = new Map<string, string[]>();

  constructor(private readonly limit = 200, private readonly file?: string) {
    if (!file) return;
    try {
      const saved = JSON.parse(fs.readFileSync(file, 'utf8')) as Record<string, unknown>;
      for (const [id, roles] of Object.entries(saved)) {
        if (Array.isArray(roles)) this.byId.set(id, roles.filter((r): r is string => typeof r === 'string'));
      }
    } catch (e) {
      if ((e as NodeJS.ErrnoException).code !== 'ENOENT') console.error(`[clearing-v2] undelivered list unreadable, starting empty: ${(e as Error).message}`);
    }
  }

  record(id: string, role: string): string[] {
    const roles = this.byId.get(id) ?? [];
    if (!roles.includes(role)) roles.push(role);
    this.byId.delete(id);
    this.byId.set(id, roles);
    while (this.byId.size > this.limit) {
      const oldest = this.byId.keys().next().value as string;
      this.byId.delete(oldest);
    }
    this.save();
    return [...roles];
  }

  all(): Record<string, string[]> {
    return Object.fromEntries([...this.byId].map(([id, roles]) => [id, [...roles]]));
  }

  private save(): void {
    if (!this.file) return;
    try {
      fs.mkdirSync(path.dirname(this.file), { recursive: true });
      fs.writeFileSync(`${this.file}.tmp`, JSON.stringify(this.all()));
      fs.renameSync(`${this.file}.tmp`, this.file);
    } catch (e) {
      console.error(`[clearing-v2] undelivered list not saved: ${(e as Error).message}`);
    }
  }
}
