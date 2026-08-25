// @test-type: unit — temp dirs only; no ~/.chorus, no live sessions, no spine
//
// #4004 — session-registry's uncovered lines were precisely its safety net:
// every read swallows failure on purpose, because "delivery must never be
// blocked by a bad marker file." Untested, that promise is a comment. These
// drive the failure paths: a registry dir that does not exist, a corrupt turn
// marker, a malformed session file, and an unwritable spine log.
import {
  readRegistry,
  readTurnState,
  defaultSweepEmit,
  pidAlive,
} from './session-registry';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

let dir: string;
beforeEach(() => { dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sr4004-')); });
afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

describe('#4004 session-registry never blocks delivery', () => {
  it('a registry directory that does not exist reads as empty, not an exception', () => {
    expect(readRegistry(path.join(dir, 'no-such-dir'))).toEqual([]);
  });

  it('a malformed session file is skipped and the good ones still load', () => {
    fs.writeFileSync(path.join(dir, 'broken.json'), '{ not json');
    fs.writeFileSync(path.join(dir, 'silas-123.json'), JSON.stringify({
      role: 'silas', pid: 123, tty: '/dev/ttys001',
    }));
    const regs = readRegistry(dir);
    expect(regs.map(r => r.role)).toContain('silas');
    expect(regs).toHaveLength(1);
  });

  it('a missing turn marker reads as idle — a role with no marker still gets nudges', () => {
    expect(readTurnState('silas', dir)).toEqual({ busy: false });
  });

  it('a corrupt turn marker reads as idle, never as permanently busy', () => {
    fs.writeFileSync(path.join(dir, 'silas.turn.json'), 'not json at all');
    expect(readTurnState('silas', dir)).toEqual({ busy: false });
  });

  it('a marker with the wrong shape reads as idle (busy must be a boolean)', () => {
    fs.writeFileSync(path.join(dir, 'silas.turn.json'), JSON.stringify({ busy: 'yes' }));
    expect(readTurnState('silas', dir)).toEqual({ busy: false });
  });

  it('NEGATIVE PROOF: a well-formed busy marker IS read as busy (#3734)', () => {
    fs.writeFileSync(path.join(dir, 'silas.turn.json'), JSON.stringify({ busy: true }));
    expect(readTurnState('silas', dir).busy).toBe(true);
  });

  it('an unwritable spine path does not throw — the sweep emit is best-effort', () => {
    const prev = process.env.CHORUS_SPINE_LOG;
    process.env.CHORUS_SPINE_LOG = path.join(dir, 'no', 'such', 'dir', 'chorus.log');
    expect(() => defaultSweepEmit('sessions.swept', { removed: '1' })).not.toThrow();
    if (prev === undefined) delete process.env.CHORUS_SPINE_LOG;
    else process.env.CHORUS_SPINE_LOG = prev;
  });

  it('a writable spine path receives the canonical JSON line', () => {
    const prev = process.env.CHORUS_SPINE_LOG;
    const log = path.join(dir, 'chorus.log');
    process.env.CHORUS_SPINE_LOG = log;
    defaultSweepEmit('sessions.swept', { removed: '2' });
    const written = JSON.parse(fs.readFileSync(log, 'utf8').trim());
    expect(written).toMatchObject({ event: 'sessions.swept', role: 'pulse', removed: '2' });
    if (prev === undefined) delete process.env.CHORUS_SPINE_LOG;
    else process.env.CHORUS_SPINE_LOG = prev;
  });

  it('pidAlive answers false for an impossible pid rather than throwing', () => {
    expect(pidAlive(999_999_99)).toBe(false);
  });
});
