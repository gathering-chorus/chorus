/**
 * cmdView domain-context + cmdChunk context-doc (#2241 wave 13).
 *
 * These branches read real .md files from predictable paths. We drop
 * temp fixtures there, run the command, then clean up.
 */

import * as fs from 'fs';
import * as path from 'path';
import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

const SRC_DIR = path.resolve(__dirname, '..', 'src');
const DOMAIN_CONTEXT_DIR = path.resolve(SRC_DIR, '..', '..', 'domain-context');
const CHUNKS_DIR = path.resolve(SRC_DIR, '..', '..', '..', 'product-manager', 'chunks');

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: BoardTask[] = [];
  byIndex: Map<number, BoardTask> = new Map();

  async list() { this.calls.push({ method: 'list', args: [] }); return this.tasks; }
  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    const m = new Map<string, BoardTask[]>();
    for (const t of this.tasks) {
      const s = (t as { status?: string }).status ?? 'Later';
      if (!m.has(s)) m.set(s, []);
      m.get(s)!.push(t);
    }
    return m;
  }
  async view(index: number) {
    this.calls.push({ method: 'view', args: [index] });
    const t = this.byIndex.get(index);
    if (!t) throw new Error(`no task ${index}`);
    return t;
  }
  async comments(_i: number) { return []; }
  async mine(r: string) { this.calls.push({ method: 'mine', args: [r] }); return []; }
  async now(r: string) { this.calls.push({ method: 'now', args: [r] }); return []; }
}

function factory(mock: MockClient) { return () => mock as unknown as BoardClient; }

function silence() {
  const origLog = console.log;
  const origErr = console.error;
  const logs: string[] = [];
  const errs: string[] = [];
  console.log = (...a) => logs.push(a.join(' '));
  console.error = (...a) => errs.push(a.join(' '));
  return { logs, errs, restore: () => { console.log = origLog; console.error = origErr; } };
}

function mkTask(overrides: Partial<BoardTask>): BoardTask {
  return {
    index: 1, title: 'task', description: '', status: 'Next',
    owner: 'Kade', priority: 'P2', domains: [], apiId: 1000, done: false,
    created: '2026-04-19T10:00:00Z', updated: '2026-04-19T10:00:00Z',
    ...overrides,
  } as unknown as BoardTask;
}

describe('cmdView — domain-context rendering', () => {
  const testDomain = `test-context-${process.pid}-${Date.now()}`;
  const fixtureFile = path.join(DOMAIN_CONTEXT_DIR, `domain-context-${testDomain}.md`);
  let createdDir = false;

  beforeAll(() => {
    if (!fs.existsSync(DOMAIN_CONTEXT_DIR)) {
      fs.mkdirSync(DOMAIN_CONTEXT_DIR, { recursive: true });
      createdDir = true;
    }
    fs.writeFileSync(fixtureFile,
      '# Test Context\n\n' +
      '## Overview\nSome narrative.\n\n' +
      '## Constraints\n' +
      '- Must preserve envelope shape\n' +
      '- Hermetic tests only\n' +
      '- No real Fuseki queries\n' +
      '## Other\nignored\n'
    );
  });

  afterAll(() => {
    try { fs.unlinkSync(fixtureFile); } catch { /* ignore */ }
    if (createdDir) {
      try { fs.rmdirSync(DOMAIN_CONTEXT_DIR); } catch { /* ignore */ }
    }
  });

  it('renders Domain Radius section with constraints when context file exists', async () => {
    const mock = new MockClient();
    mock.byIndex.set(42, mkTask({
      index: 42, title: 'ctx test', domains: [`domain:${testDomain}`],
    }));
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'view', '42'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Domain Radius/);
    expect(joined).toMatch(/Constraints \(3\)/);
    expect(joined).toMatch(/Must preserve envelope shape/);
  });

  it('renders missing-context summary when file absent for a domain', async () => {
    const mock = new MockClient();
    mock.byIndex.set(43, mkTask({
      index: 43, title: 'missing-ctx',
      domains: [`domain:nonexistent-domain-${Date.now()}`],
    }));
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'view', '43'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/missing context/);
  });
});

describe('cmdChunk — chunk context doc rendering', () => {
  const testChunkDoc = path.join(CHUNKS_DIR, 'ops.md');
  let createdDir = false;
  let existedBefore = false;

  beforeAll(() => {
    if (!fs.existsSync(CHUNKS_DIR)) {
      fs.mkdirSync(CHUNKS_DIR, { recursive: true });
      createdDir = true;
    }
    existedBefore = fs.existsSync(testChunkDoc);
    if (!existedBefore) {
      fs.writeFileSync(testChunkDoc,
        '# Ops Chunk\n\n' +
        'Overview of the ops chunk.\n\n' +
        '## Focus\n' +
        'first section\n\n' +
        '## Priorities\n' +
        'second section\n\n' +
        '## Other\n' +
        'should be cut\n'
      );
    }
  });

  afterAll(() => {
    if (!existedBefore) {
      try { fs.unlinkSync(testChunkDoc); } catch { /* ignore */ }
    }
    if (createdDir) {
      try { fs.rmdirSync(CHUNKS_DIR); } catch { /* ignore */ }
    }
  });

  it('chunk ops renders the context doc summary when file exists', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Next', title: 'ops-card', domains: ['chunk:ops'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'chunk', 'ops'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/ops-card/);
    // Context doc summary (if fixture was ours) shows header + first couple sections
    // eslint-disable-next-line jest/no-conditional-expect -- assert fixture content only when this test created it
    if (!existedBefore) expect(joined).toMatch(/# Ops Chunk|Overview of the ops chunk/);
  });
});
