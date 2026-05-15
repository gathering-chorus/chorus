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
