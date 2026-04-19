/**
 * sdk.ts filesystem-side tests (#2241 wave 3).
 *
 * Covers notifyOwnerIfDifferent, notifyPM, reconcileWorkflows, auditStart,
 * auditClose — every function that wrote to real role dirs or real
 * workflow manifest files in production. Uses __setTestPaths to redirect
 * all path I/O into a per-test temp dir so the run is hermetic and doesn't
 * pollute briefs/ / workflows/active/.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import type { BoardClient } from '../src/client';
import {
  __setTestPaths,
  __resetTestPaths,
  notifyOwnerIfDifferent,
  notifyPM,
  reconcileWorkflows,
  auditStart,
  auditClose,
} from '../src/sdk';

function mkTmp(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'cards-sdk-test-'));
}

function silence() {
  const origLog = console.log;
  const origErr = console.error;
  const logs: string[] = [];
  const errs: string[] = [];
  console.log = (...a) => logs.push(a.join(' '));
  console.error = (...a) => errs.push(a.join(' '));
  return {
    logs, errs,
    restore: () => { console.log = origLog; console.error = origErr; },
  };
}

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: Array<Record<string, unknown>> = [];

  async snapshot(): Promise<{ board: string; timestamp: string; tasks: Array<Record<string, unknown>> }> {
    this.calls.push({ method: 'snapshot', args: [] });
    return { board: this.boardName, timestamp: new Date().toISOString(), tasks: this.tasks };
  }
}

function asBoardClient(m: MockClient): BoardClient {
  return m as unknown as BoardClient;
}

describe('notifyOwnerIfDifferent', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkTmp();
    __setTestPaths({
      briefDirs: {
        silas: path.join(tmp, 'roles', 'silas', 'briefs'),
        kade: path.join(tmp, 'roles', 'kade', 'briefs'),
        wren: path.join(tmp, 'roles', 'wren', 'briefs'),
      },
    });
  });

  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  it('writes a brief file when owner differs from mover', () => {
    const cap = silence();
    try {
      notifyOwnerIfDifferent(42, 'Build a thing', 'Wren', 'moved-to-WIP', 'kade');
    } finally { cap.restore(); }
    const wrenDir = path.join(tmp, 'roles', 'wren', 'briefs');
    const files = fs.readdirSync(wrenDir);
    expect(files.length).toBe(1);
    expect(files[0]).toMatch(/card-42-moved-to-WIP/);
    const body = fs.readFileSync(path.join(wrenDir, files[0]), 'utf-8');
    expect(body).toMatch(/#42/);
    expect(body).toMatch(/by kade/);
  });

  it('no-ops when owner equals mover', () => {
    const cap = silence();
    try {
      notifyOwnerIfDifferent(42, 'x', 'Kade', 'moved', 'kade');
    } finally { cap.restore(); }
    const kadeDir = path.join(tmp, 'roles', 'kade', 'briefs');
    expect(fs.existsSync(kadeDir) ? fs.readdirSync(kadeDir).length : 0).toBe(0);
  });

  it('no-ops when owner is "jeff" (Jeff has no briefs dir)', () => {
    const cap = silence();
    try {
      notifyOwnerIfDifferent(42, 'x', 'jeff', 'moved', 'kade');
    } finally { cap.restore(); }
    // No side effect beyond the early return — nothing to assert beyond
    // no exception + no dir created for "jeff".
    expect(fs.existsSync(path.join(tmp, 'roles', 'jeff'))).toBe(false);
  });

  it('is idempotent: re-running with same inputs does not overwrite', () => {
    const cap = silence();
    try {
      notifyOwnerIfDifferent(42, 'x', 'Wren', 'moved', 'kade');
      notifyOwnerIfDifferent(42, 'DIFFERENT TITLE', 'Wren', 'moved', 'kade');
    } finally { cap.restore(); }
    const wrenDir = path.join(tmp, 'roles', 'wren', 'briefs');
    const files = fs.readdirSync(wrenDir);
    expect(files).toHaveLength(1);
    const body = fs.readFileSync(path.join(wrenDir, files[0]), 'utf-8');
    expect(body).toMatch(/x/); // original title, not "DIFFERENT TITLE"
    expect(body).not.toMatch(/DIFFERENT TITLE/);
  });

  it('unknown owner → no brief written, no throw', () => {
    const cap = silence();
    try {
      notifyOwnerIfDifferent(42, 'x', 'stranger', 'moved', 'kade');
    } finally { cap.restore(); }
    // No brief dirs populated for known roles either.
    ['silas', 'wren', 'kade'].forEach((r) => {
      const d = path.join(tmp, 'roles', r, 'briefs');
      expect(fs.existsSync(d) ? fs.readdirSync(d).length : 0).toBe(0);
    });
  });
});

describe('notifyPM', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkTmp();
    __setTestPaths({
      briefDirs: {
        silas: path.join(tmp, 'roles', 'silas', 'briefs'),
        kade: path.join(tmp, 'roles', 'kade', 'briefs'),
        wren: path.join(tmp, 'roles', 'wren', 'briefs'),
      },
    });
  });

  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  it('writes shipped-brief to wren/briefs when non-wren completes', () => {
    const cap = silence();
    try {
      notifyPM(99, 'refactor handler', 'Kade', 'kade');
    } finally { cap.restore(); }
    const wrenDir = path.join(tmp, 'roles', 'wren', 'briefs');
    const files = fs.readdirSync(wrenDir);
    expect(files.length).toBe(1);
    expect(files[0]).toMatch(/card-99-shipped/);
  });

  it('no-ops when wren is the completer', () => {
    const cap = silence();
    try {
      notifyPM(99, 'x', 'Wren', 'wren');
    } finally { cap.restore(); }
    const wrenDir = path.join(tmp, 'roles', 'wren', 'briefs');
    expect(fs.existsSync(wrenDir) ? fs.readdirSync(wrenDir).length : 0).toBe(0);
  });
});

describe('reconcileWorkflows', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkTmp();
    __setTestPaths({
      workflowsActiveDir: path.join(tmp, 'active'),
      workflowsArchiveDir: path.join(tmp, 'archive'),
    });
    fs.mkdirSync(path.join(tmp, 'active'), { recursive: true });
  });

  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  it('archives matching manifest, skips remaining steps', () => {
    const manifest = {
      id: 'WF-001', card: 42, status: 'active', updated: '2026-04-01',
      steps: [
        { status: 'completed', notes: '' },
        { status: 'pending', notes: '' },
        { status: 'ready', notes: '' },
      ],
      history: [],
    };
    fs.writeFileSync(path.join(tmp, 'active', 'WF-001.json'), JSON.stringify(manifest));
    const cap = silence();
    try {
      reconcileWorkflows(42, 'kade');
    } finally { cap.restore(); }
    // Active file removed, archived file created
    expect(fs.existsSync(path.join(tmp, 'active', 'WF-001.json'))).toBe(false);
    expect(fs.existsSync(path.join(tmp, 'archive', 'WF-001.json'))).toBe(true);
    const archived = JSON.parse(fs.readFileSync(path.join(tmp, 'archive', 'WF-001.json'), 'utf-8'));
    expect(archived.status).toBe('completed');
    expect(archived.steps[1].status).toBe('skipped');
    expect(archived.steps[2].status).toBe('skipped');
    expect(archived.history.some((h: { event: string }) => h.event === 'workflow_completed')).toBe(true);
  });

  it('leaves non-matching manifests alone', () => {
    const manifest = {
      id: 'WF-002', card: 999, status: 'active', updated: '2026-04-01',
      steps: [{ status: 'pending' }], history: [],
    };
    fs.writeFileSync(path.join(tmp, 'active', 'WF-002.json'), JSON.stringify(manifest));
    const cap = silence();
    try {
      reconcileWorkflows(42, 'kade');
    } finally { cap.restore(); }
    expect(fs.existsSync(path.join(tmp, 'active', 'WF-002.json'))).toBe(true);
    expect(fs.existsSync(path.join(tmp, 'archive', 'WF-002.json'))).toBe(false);
  });

  it('no-op when active dir does not exist', () => {
    fs.rmSync(path.join(tmp, 'active'), { recursive: true, force: true });
    const cap = silence();
    try {
      reconcileWorkflows(42, 'kade'); // should not throw
    } finally { cap.restore(); }
    expect(fs.existsSync(path.join(tmp, 'archive'))).toBe(false);
  });

  it('skips files that do not match WF-NNN.json pattern', () => {
    fs.writeFileSync(path.join(tmp, 'active', 'not-a-workflow.txt'), 'x');
    const cap = silence();
    try {
      reconcileWorkflows(42, 'kade');
    } finally { cap.restore(); }
    expect(fs.existsSync(path.join(tmp, 'active', 'not-a-workflow.txt'))).toBe(true);
  });
});

describe('auditStart', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkTmp();
    __setTestPaths({ snapshotDir: tmp });
  });

  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  function mkTask(overrides: Record<string, unknown>): Record<string, unknown> {
    const now = new Date().toISOString();
    return {
      index: 1, title: 't', description: '', status: 'Next',
      owner: 'Kade', priority: 'P2', domains: [], apiId: 100,
      created: now, updated: now, done: false,
      ...overrides,
    };
  }

  it('writes snapshot file and returns counts', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, owner: 'Kade', status: 'Next' }),
      mkTask({ index: 2, owner: 'Wren', status: 'Next' }),
    ];
    const cap = silence();
    let result;
    try {
      result = await auditStart(asBoardClient(mock), 'kade');
    } finally { cap.restore(); }
    expect(result.nowCount).toBe(0);
    const snapFile = path.join(tmp, 'board-snapshot-gathering-kade.json');
    expect(fs.existsSync(snapFile)).toBe(true);
  });

  it('detects stale Now tasks (updated > 48h ago)', async () => {
    const old = new Date(Date.now() - 72 * 3600_000).toISOString();
    const fresh = new Date().toISOString();
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, owner: 'Kade', status: 'WIP', updated: old }),
      mkTask({ index: 2, owner: 'Kade', status: 'WIP', updated: fresh }),
    ];
    const cap = silence();
    let result;
    try {
      result = await auditStart(asBoardClient(mock), 'kade');
    } finally { cap.restore(); }
    expect(result.nowCount).toBe(2);
    expect(result.staleNow).toBe(1);
  });

  it('detects stale Next tasks (updated > 7d ago)', async () => {
    const old = new Date(Date.now() - 10 * 24 * 3600_000).toISOString();
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, owner: 'Kade', status: 'Next', updated: old }),
    ];
    const cap = silence();
    let result;
    try {
      result = await auditStart(asBoardClient(mock), 'kade');
    } finally { cap.restore(); }
    expect(result.staleNext).toBe(1);
  });

  it('filters to the asked role (case-insensitive)', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, owner: 'Kade', status: 'Now' }),
      mkTask({ index: 2, owner: 'Wren', status: 'Now' }),
      mkTask({ index: 3, owner: 'Kade', status: 'Now' }),
    ];
    const cap = silence();
    let result;
    try {
      result = await auditStart(asBoardClient(mock), 'KADE');
    } finally { cap.restore(); }
    expect(result.nowCount).toBe(2);
  });

  it('renders SWAT bucket when role has SWAT tasks', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, owner: 'Kade', status: 'SWAT', title: 'urgent' })];
    const cap = silence();
    try { await auditStart(asBoardClient(mock), 'kade'); } finally { cap.restore(); }
    expect(cap.logs.some((l) => /SWAT \(1\)/.test(l))).toBe(true);
  });

  it('renders Harvesting bucket when role has Harvesting tasks', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, owner: 'Kade', status: 'Harvesting', title: 'photo-run' })];
    const cap = silence();
    try { await auditStart(asBoardClient(mock), 'kade'); } finally { cap.restore(); }
    expect(cap.logs.some((l) => /Harvesting \(1\)/.test(l))).toBe(true);
  });

  it('renders Blocked bucket when role has Blocked tasks', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, owner: 'Kade', status: 'Blocked', title: 'waiting' })];
    const cap = silence();
    try { await auditStart(asBoardClient(mock), 'kade'); } finally { cap.restore(); }
    expect(cap.logs.some((l) => /Blocked \(1\)/.test(l))).toBe(true);
  });

  it('prints "no active items" when role has neither Now nor Next', async () => {
    const mock = new MockClient();
    mock.tasks = [];
    const cap = silence();
    try {
      await auditStart(asBoardClient(mock), 'kade');
    } finally { cap.restore(); }
    expect(cap.logs.some((l) => /No active items/i.test(l))).toBe(true);
  });
});

describe('auditClose', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkTmp();
    __setTestPaths({ snapshotDir: tmp });
  });

  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  it('returns zero counts and prints diagnostic when no prior snapshot exists', async () => {
    const mock = new MockClient();
    const cap = silence();
    let result;
    try {
      result = await auditClose(asBoardClient(mock), 'kade');
    } finally { cap.restore(); }
    expect(result).toEqual({ newCards: 0, newlyDone: 0, retroactive: 0 });
    expect(cap.logs.some((l) => /No start-of-session snapshot/i.test(l))).toBe(true);
  });
});
