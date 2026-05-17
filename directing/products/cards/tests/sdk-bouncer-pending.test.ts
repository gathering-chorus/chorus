/**
 * writePendingApprovalArtifacts — #2924 AC3 bridge.
 *
 * The bouncer writes two sibling pickup files when it refuses an agent
 * `cards add`:
 *   - <pending>/<role>-<stamp>.txt       human-readable [card-approval] block
 *   - <pending>/<role>-<stamp>.argv.json structured {title, opts} payload
 *
 * The .argv.json is the bridge to the chorus-hooks UserPromptSubmit
 * responder (AC3): on Jeff typing `approve`, the responder reads the
 * structured payload and replays `cards add` with DEPLOY_ROLE=jeff. The
 * .txt is kept for human inspection and as the existing fallback surface.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { writePendingApprovalArtifacts } from '../src/sdk';

describe('writePendingApprovalArtifacts (#2924 AC3 bridge)', () => {
  let tmpDir: string;
  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'wren-2924-'));
  });
  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  test('writes both .txt and .argv.json under the role-stamp basename', () => {
    const result = writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'wren',
      stamp: '2026-05-15T13-00-00-000Z',
      nudge: '[card-approval] wren → jeff\n\nfull block here',
      title: 'Test card title',
      cardOpts: {
        owner: 'wren',
        priority: 'P1',
        domain: 'chorus',
        type: 'fix',
        origin: 'reactive',
        description: '## Experience\n\nstub',
      },
    });

    expect(result.txtPath).toBe(`${tmpDir}/wren-2026-05-15T13-00-00-000Z.txt`);
    expect(result.argvPath).toBe(`${tmpDir}/wren-2026-05-15T13-00-00-000Z.argv.json`);
    expect(fs.existsSync(result.txtPath)).toBe(true);
    expect(fs.existsSync(result.argvPath)).toBe(true);
  });

  test('.txt contains the human-readable nudge verbatim', () => {
    const nudge = '[card-approval] wren → jeff\n\nVerbatim body line.';
    const result = writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'wren',
      stamp: 's',
      nudge,
      title: 't',
      cardOpts: { owner: 'wren' },
    });
    expect(fs.readFileSync(result.txtPath, 'utf-8')).toBe(nudge);
  });

  test('.argv.json carries title + opts as parseable JSON for replay', () => {
    const result = writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'silas',
      stamp: 's',
      nudge: 'n',
      title: 'Daemon-runtime deploy path',
      cardOpts: {
        owner: 'silas',
        priority: 'P1',
        domain: 'chorus',
        type: 'new',
        origin: 'reflective',
        sequence: 'werk',
        subproduct: 'werk',
        chunk: 'ops',
        description: '## Experience\n\nbody',
      },
    });
    const parsed = JSON.parse(fs.readFileSync(result.argvPath, 'utf-8'));
    expect(parsed.title).toBe('Daemon-runtime deploy path');
    expect(parsed.opts.owner).toBe('silas');
    expect(parsed.opts.priority).toBe('P1');
    expect(parsed.opts.type).toBe('new');
    expect(parsed.opts.subproduct).toBe('werk');
    expect(parsed.opts.chunk).toBe('ops');
    expect(parsed.opts.description).toContain('## Experience');
  });

  test('creates the pending directory if it does not exist', () => {
    const nested = path.join(tmpDir, 'a', 'b', 'c');
    expect(fs.existsSync(nested)).toBe(false);
    writePendingApprovalArtifacts({
      pendingDir: nested,
      role: 'wren',
      stamp: 's',
      nudge: 'n',
      title: 't',
      cardOpts: { owner: 'wren' },
    });
    expect(fs.existsSync(nested)).toBe(true);
  });
});

// ---- #2964 dedupe + retry refusal ----

import { findExistingPendingByTitle, PendingRetryTooSoonError } from '../src/sdk';

describe('writePendingApprovalArtifacts dedupe + retry refusal (#2964)', () => {
  let tmpDir: string;
  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'wren-2964-'));
  });
  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  test('agent retry within 30s with same title throws PendingRetryTooSoonError; no duplicate file written', () => {
    const writeArgs = {
      pendingDir: tmpDir,
      role: 'wren',
      nudge: 'n',
      title: 'Same title — retry test',
      cardOpts: { owner: 'wren' },
    };
    // First attempt: succeeds, one file on disk
    writePendingApprovalArtifacts({ ...writeArgs, stamp: 'first' });
    const filesAfterFirst = fs.readdirSync(tmpDir).filter((f) => f.endsWith('.argv.json'));
    expect(filesAfterFirst).toHaveLength(1);
    // Second attempt with same title, fresh stamp: bouncer should refuse.
    expect(() =>
      writePendingApprovalArtifacts({ ...writeArgs, stamp: 'second' }),
    ).toThrow(PendingRetryTooSoonError);
    // Still only one .argv.json file on disk — no duplicate.
    const filesAfterRetry = fs.readdirSync(tmpDir).filter((f) => f.endsWith('.argv.json'));
    expect(filesAfterRetry).toHaveLength(1);
  });

  test('within dedupe window (>30s, <10min) overwrites the existing payload in place', () => {
    const writeArgs = {
      pendingDir: tmpDir,
      role: 'wren',
      nudge: 'first-nudge',
      title: 'Same title — dedupe test',
      cardOpts: { owner: 'wren', priority: 'P3' },
    };
    const first = writePendingApprovalArtifacts({ ...writeArgs, stamp: 'first' });
    // Push the existing file's mtime back so retry-refusal does not fire,
    // but dedupe still does (older than 30s, younger than 10min).
    const oldMtime = Date.now() - 60_000; // 60s ago
    fs.utimesSync(first.argvPath, oldMtime / 1000, oldMtime / 1000);
    // Second attempt with a different stamp + nudge + opts — should overwrite.
    const second = writePendingApprovalArtifacts({
      ...writeArgs,
      stamp: 'second',
      nudge: 'second-nudge',
      cardOpts: { owner: 'wren', priority: 'P1' },
    });
    expect(second.argvPath).toBe(first.argvPath);
    expect(second.txtPath).toBe(first.txtPath);
    const all = fs.readdirSync(tmpDir).filter((f) => f.endsWith('.argv.json'));
    expect(all).toHaveLength(1);
    // The overwrite should reflect the second call's content.
    const parsed = JSON.parse(fs.readFileSync(second.argvPath, 'utf-8'));
    expect(parsed.opts.priority).toBe('P1');
    expect(fs.readFileSync(second.txtPath, 'utf-8')).toBe('second-nudge');
  });

  test('different titles for same role write distinct payloads — no false dedupe', () => {
    writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'wren',
      stamp: 'a',
      nudge: 'n',
      title: 'Title A',
      cardOpts: { owner: 'wren' },
    });
    writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'wren',
      stamp: 'b',
      nudge: 'n',
      title: 'Title B',
      cardOpts: { owner: 'wren' },
    });
    const all = fs.readdirSync(tmpDir).filter((f) => f.endsWith('.argv.json'));
    expect(all).toHaveLength(2);
  });

  test('different roles + same title write distinct payloads', () => {
    writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'wren',
      stamp: 'a',
      nudge: 'n',
      title: 'Shared title',
      cardOpts: { owner: 'wren' },
    });
    writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'silas',
      stamp: 'b',
      nudge: 'n',
      title: 'Shared title',
      cardOpts: { owner: 'silas' },
    });
    const wrenFiles = fs.readdirSync(tmpDir).filter((f) => f.startsWith('wren-'));
    const silasFiles = fs.readdirSync(tmpDir).filter((f) => f.startsWith('silas-'));
    expect(wrenFiles).toHaveLength(2); // .txt + .argv.json
    expect(silasFiles).toHaveLength(2);
  });

  test('findExistingPendingByTitle returns null on missing pending dir', () => {
    const missing = path.join(tmpDir, 'does-not-exist');
    expect(findExistingPendingByTitle(missing, 'wren', 'anything')).toBeNull();
  });

  test('findExistingPendingByTitle returns matching path + age', () => {
    const w = writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'wren',
      stamp: 'live',
      nudge: 'n',
      title: 'Searchable title',
      cardOpts: { owner: 'wren' },
    });
    const found = findExistingPendingByTitle(tmpDir, 'wren', 'Searchable title');
    expect(found).not.toBeNull();
    expect(found!.path).toBe(w.argvPath);
    expect(found!.ageMs).toBeGreaterThanOrEqual(0);
    expect(found!.ageMs).toBeLessThan(5_000); // just-written
  });

  test('findExistingPendingByTitle ignores other-role files with same title', () => {
    writePendingApprovalArtifacts({
      pendingDir: tmpDir,
      role: 'silas',
      stamp: 'live',
      nudge: 'n',
      title: 'Shared title',
      cardOpts: { owner: 'silas' },
    });
    expect(findExistingPendingByTitle(tmpDir, 'wren', 'Shared title')).toBeNull();
  });
});
