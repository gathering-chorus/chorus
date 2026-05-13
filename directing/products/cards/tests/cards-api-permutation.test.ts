/**
 * Data-driven API permutation tests for cards CLI (#2245).
 *
 * Iterates tests/fixtures/cards-api-matrix.json. Each row specifies a
 * CLI command + args, expected stdout pattern, and optional expected
 * MockClient method call. Adding a new case = editing the matrix only.
 *
 * Hermetic — no real Vikunja connection, no network.
 */

import * as path from 'path';
import * as fs from 'fs';
import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

interface FixtureState { tasks: BoardTask[]; }
interface MatrixCase {
  id: string;
  args: string[];
  stdoutMatch?: string | null;
  expectCall?: string;
}
interface Matrix { cases: MatrixCase[]; }

const FIXTURE_DIR = path.join(__dirname, 'fixtures');
const state: FixtureState = JSON.parse(
  fs.readFileSync(path.join(FIXTURE_DIR, 'cards-api-state.json'), 'utf8')
);
const matrix: Matrix = JSON.parse(
  fs.readFileSync(path.join(FIXTURE_DIR, 'cards-api-matrix.json'), 'utf8')
);

class FixtureClient {
  calls: Array<{ method: string; args: unknown[] }> = [];
  private tasks: BoardTask[];
  constructor(tasks: BoardTask[]) { this.tasks = [...tasks]; }
  private record(method: string, args: unknown[]): void { this.calls.push({ method, args }); }
  async list(): Promise<BoardTask[]> { this.record('list', []); return this.tasks; }
  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    this.record('listGrouped', []);
    const map = new Map<string, BoardTask[]>();
    for (const t of this.tasks) {
      const s = (t as { status?: string }).status ?? 'Later';
      if (!map.has(s)) map.set(s, []);
      map.get(s)!.push(t);
    }
    return map;
  }
  async mine(role: string): Promise<BoardTask[]> { this.record('mine', [role]); return []; }
  async now(role: string): Promise<BoardTask[]> { this.record('now', [role]); return []; }
  async view(index: number): Promise<BoardTask> {
    this.record('view', [index]);
    const found = this.tasks.find((t) => t.index === index);
    if (!found) throw new Error(`Task ${index} not found`);
    return found;
  }
  async add(title: string, opts?: unknown): Promise<BoardTask> {
    this.record('add', [title, opts]);
    return { index: 9999, title, description: '', status: 'Later', owner: 'Wren', priority: 'P2', domains: [], apiId: 10000 } as unknown as BoardTask;
  }
  async move(index: number, status: string): Promise<void> { this.record('move', [index, status]); }
  async done(index: number): Promise<void> { this.record('done', [index]); }
  async demo(index: number): Promise<void> { this.record('demo', [index]); }
  async reject(index: number, reason: string): Promise<void> { this.record('reject', [index, reason]); }
  async block(index: number, reason: string): Promise<void> { this.record('block', [index, reason]); }
  async unblock(index: number): Promise<void> { this.record('unblock', [index]); }
  async comment(index: number, text: string): Promise<void> { this.record('comment', [index, text]); }
  async comments(_index: number): Promise<Array<{ author: string; text: string }>> { this.record('comments', [_index]); return [{ author: 'wren', text: 'demo:preflight-pass ac=1/1 — wren' }]; }
  async update(index: number, fields: unknown): Promise<void> { this.record('update', [index, fields]); }
  async tag(index: number, category: string, value: string): Promise<void> { this.record('tag', [index, category, value]); }
  async untag(index: number, label: string): Promise<void> { this.record('untag', [index, label]); }
  async reassignOwner(index: number, role: string): Promise<{ oldOwner: string; newOwner: string }> { this.record('reassignOwner', [index, role]); return { oldOwner: 'Wren', newOwner: role }; }
  async bulkMove(ids: number[], status: string): Promise<void> { this.record('move', [ids, status]); }
  async fetchBuckets(): Promise<Array<{ id: number; title: string; limit: number; taskCount: number }>> { this.record('fetchBuckets', []); return []; }
  async fetchBucketsWithLimits(): Promise<Array<{ id: number; title: string; limit: number; taskCount: number }>> { this.record('fetchBucketsWithLimits', []); return []; }
  async listLabels(): Promise<Array<{ id: number; title: string }>> { this.record('listLabels', []); return [{ id: 1, title: 'P1' }]; }
  async createLabel(title: string): Promise<{ id: number; title: string }> { this.record('createLabel', [title]); return { id: 999, title }; }
}

function captureConsole(): { logs: string[]; errs: string[]; restore: () => void } {
  const logs: string[] = [];
  const errs: string[] = [];
  const origLog = console.log;
  const origErr = console.error;
  console.log = (...a) => { logs.push(a.join(' ')); };
  console.error = (...a) => { errs.push(a.join(' ')); };
  return { logs, errs, restore: () => { console.log = origLog; console.error = origErr; } };
}

describe('cards CLI — permutation matrix (#2245)', () => {
  let origExit: typeof process.exit;
  beforeAll(() => {
    origExit = process.exit;
    process.exit = ((code?: number) => { throw new Error(`exit(${code})`); }) as typeof process.exit;
  });
  afterAll(() => { process.exit = origExit; });

  describe.each(matrix.cases.map((c) => [c.id, c] as [string, MatrixCase]))(
    '%s',
    (_id, tc) => {
      it('runs without throwing', async () => {
        const client = new FixtureClient(state.tasks);
        const factory = () => client as unknown as BoardClient;
        const cap = captureConsole();

        try {
          await runCli(['node', 'cards', ...tc.args], factory);
        } catch {
          // process.exit() throws in test env — acceptable
        } finally {
          cap.restore();
        }

        const stdout = cap.logs.join('\n');

        /* eslint-disable jest/no-conditional-expect -- table-driven test, fields are optional per row */
        if (tc.stdoutMatch) expect(stdout).toMatch(tc.stdoutMatch);
        if (tc.expectCall) {
          const called = client.calls.some((c) => c.method === tc.expectCall);
          expect(called).toBe(true);
        }
        /* eslint-enable jest/no-conditional-expect */
      });
    }
  );
});
