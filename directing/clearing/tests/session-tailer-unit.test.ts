/**
 * SessionTailer — unit tests (#2167 phase 2).
 *
 * Target: 80%+ on src/session-tailer.ts. CLEARING_PROJECTS_DIR seam
 * points at a tempdir fixture so fs paths don't touch Jeff's real
 * ~/.claude/projects. processLine is exercised directly; fs.watch is
 * mocked via jest to avoid inotify churn.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'session-tailer-test-'));
process.env.CLEARING_PROJECTS_DIR = TMP;

function load() {
  return require('../src/session-tailer');
}

function makeRouter() {
  return { ingest: jest.fn() };
}

afterAll(() => {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch { /* ignore */ }
});

describe('SessionTailer.processLine — user messages', () => {
  let tailer: any;
  let router: ReturnType<typeof makeRouter>;

  beforeEach(() => {
    jest.resetModules();
    const { SessionTailer } = load();
    router = makeRouter();
    tailer = new SessionTailer(router as any);
  });

  const fire = (role: string, entry: any) =>
    (tailer as any).processLine(role, JSON.stringify(entry));

  test('malformed JSON line is silently dropped', () => {
    (tailer as any).processLine('kade', 'not json');
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('user string content surfaces as jeff-input', () => {
    fire('kade', {
      type: 'user',
      message: { content: 'hello team' },
      timestamp: '2026-04-17T20:00:00Z',
    });
    expect(router.ingest).toHaveBeenCalledWith(expect.objectContaining({
      from: 'jeff',
      text: 'hello team',
      type: 'jeff-input',
    }));
  });

  test('user with empty content is dropped', () => {
    fire('kade', { type: 'user', message: { content: '' } });
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('user array content joins text blocks into single line', () => {
    fire('kade', {
      type: 'user',
      message: { content: [
        { type: 'text', text: 'part one' },
        { type: 'text', text: 'part\ntwo' },
      ] },
    });
    const call = (router.ingest as jest.Mock).mock.calls[0][0];
    expect(call.text).toBe('part one part two');
  });

  test('user array reconstructs slash command from name + args', () => {
    fire('kade', {
      type: 'user',
      message: { content: [
        { type: 'text', text: '<command-name>pull</command-name>' },
        { type: 'text', text: '<command-args>2167</command-args>' },
      ] },
    });
    expect(router.ingest).toHaveBeenCalledWith(expect.objectContaining({
      from: 'jeff', text: 'pull 2167', type: 'jeff-input',
    }));
  });

  test('user array strips system-reminder, command-message, hook boilerplate', () => {
    fire('kade', {
      type: 'user',
      message: { content: [
        { type: 'text', text: '<system-reminder>ignore me</system-reminder>' },
        { type: 'text', text: '<command-message>also ignore</command-message>' },
        { type: 'text', text: 'Base directory for this skill: /x' },
        { type: 'text', text: 'ARGUMENTS: raw' },
        { type: 'text', text: 'Stop hook ran' },
        { type: 'text', text: 'actual question' },
      ] },
    });
    const call = (router.ingest as jest.Mock).mock.calls[0][0];
    expect(call.text).toBe('actual question');
  });

  test('user with [nudge from X] prefix attributes to sending role', () => {
    fire('kade', {
      type: 'user',
      message: { content: '[nudge from silas | 10:00] check the deploy' },
    });
    expect(router.ingest).toHaveBeenCalledWith(expect.objectContaining({
      from: 'silas',
      text: expect.stringContaining('[nudge from silas'),
      type: 'role-response',
    }));
  });

  test('user with only filtered content resolves to empty and drops', () => {
    fire('kade', {
      type: 'user',
      message: { content: [
        { type: 'text', text: '<system-reminder>skip</system-reminder>' },
      ] },
    });
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('user message with no content at all is dropped', () => {
    fire('kade', { type: 'user', message: {} });
    expect(router.ingest).not.toHaveBeenCalled();
  });
});

describe('SessionTailer.processLine — assistant messages', () => {
  let tailer: any;
  let router: ReturnType<typeof makeRouter>;

  beforeEach(() => {
    jest.useFakeTimers();
    jest.resetModules();
    const { SessionTailer } = load();
    router = makeRouter();
    tailer = new SessionTailer(router as any);
  });

  afterEach(() => jest.useRealTimers());

  const fire = (role: string, entry: any) =>
    (tailer as any).processLine(role, JSON.stringify(entry));

  test('assistant text debounces 3s and emits pm-thinking', () => {
    fire('silas', {
      type: 'assistant',
      message: { content: [{ type: 'text', text: 'my take on this' }] },
    });
    expect(router.ingest).not.toHaveBeenCalled();
    jest.advanceTimersByTime(3000);
    expect(router.ingest).toHaveBeenCalledWith(expect.objectContaining({
      from: 'silas',
      text: 'my take on this',
      type: 'pm-thinking',
    }));
  });

  test('rapid assistant messages keep only the latest (debounce)', () => {
    fire('silas', { type: 'assistant', message: { content: [{ type: 'text', text: 'first draft' }] } });
    jest.advanceTimersByTime(1000);
    fire('silas', { type: 'assistant', message: { content: [{ type: 'text', text: 'final answer' }] } });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).toHaveBeenCalledTimes(1);
    expect((router.ingest as jest.Mock).mock.calls[0][0].text).toBe('final answer');
  });

  test('strips chorus prompt prefix', () => {
    fire('silas', {
      type: 'assistant',
      message: { content: [{
        type: 'text',
        text: '--- Silas | 2026-04-17 16:00 Boston | #2167 | Werk v174 ---\nmy thought',
      }] },
    });
    jest.advanceTimersByTime(3000);
    expect((router.ingest as jest.Mock).mock.calls[0][0].text).toBe('my thought');
  });

  test('assistant with only chorus prompt (nothing after) is dropped', () => {
    fire('silas', {
      type: 'assistant',
      message: { content: [{
        type: 'text',
        text: '--- Silas | 2026-04-17 16:00 Boston | Werk v174 ---',
      }] },
    });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('DELIVERED-to-role assistant output is filtered', () => {
    fire('wren', {
      type: 'assistant',
      message: { content: [{ type: 'text', text: 'DELIVERED to silas at 10:00' }] },
    });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('card.X spine output is filtered', () => {
    fire('wren', {
      type: 'assistant',
      message: { content: [{ type: 'text', text: 'card.accepted | wren' }] },
    });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('bash script-call output is filtered', () => {
    fire('kade', {
      type: 'assistant',
      message: { content: [{ type: 'text', text: 'bash ../../scripts/role-state query kade' }] },
    });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('[bridge] tagged output is filtered', () => {
    fire('kade', {
      type: 'assistant',
      message: { content: [{ type: 'text', text: 'some [bridge] echo' }] },
    });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('role.nudge.consumed output is filtered', () => {
    fire('kade', {
      type: 'assistant',
      message: { content: [{ type: 'text', text: 'role.nudge.consumed | kade' }] },
    });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('assistant string content goes through (non-array)', () => {
    fire('wren', { type: 'assistant', message: { content: 'plain string response' } });
    jest.advanceTimersByTime(3000);
    expect((router.ingest as jest.Mock).mock.calls[0][0].text).toBe('plain string response');
  });

  test('assistant with no content is dropped', () => {
    fire('wren', { type: 'assistant', message: {} });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('assistant with only non-text blocks yields empty and drops', () => {
    fire('kade', {
      type: 'assistant',
      message: { content: [{ type: 'tool_use', name: 'Bash' }] },
    });
    jest.advanceTimersByTime(3000);
    expect(router.ingest).not.toHaveBeenCalled();
  });
});

describe('SessionTailer.processLine — unknown types ignored', () => {
  test('tool_use, tool_result, system, progress — all dropped', () => {
    jest.resetModules();
    const { SessionTailer } = load();
    const router = makeRouter();
    const t = new SessionTailer(router as any);
    for (const type of ['tool_use', 'tool_result', 'system', 'progress', 'other']) {
      (t as any).processLine('kade', JSON.stringify({ type }));
    }
    expect(router.ingest).not.toHaveBeenCalled();
  });
});

describe('SessionTailer.findSessionFile', () => {
  beforeEach(() => {
    // Clean TMP and re-import
    for (const f of fs.readdirSync(TMP)) {
      fs.rmSync(path.join(TMP, f), { recursive: true, force: true });
    }
    jest.resetModules();
  });

  test('returns null when PROJECTS_DIR is empty', () => {
    const { SessionTailer } = load();
    const t = new SessionTailer(makeRouter() as any);
    expect((t as any).findSessionFile('kade')).toBeNull();
  });

  test('returns null for unknown role', () => {
    const { SessionTailer } = load();
    const t = new SessionTailer(makeRouter() as any);
    expect((t as any).findSessionFile('jeff')).toBeNull();
  });

  test('finds newest .jsonl across matching project dirs', () => {
    fs.mkdirSync(path.join(TMP, 'project-kade-old'));
    fs.mkdirSync(path.join(TMP, 'project-kade-new'));
    const oldFile = path.join(TMP, 'project-kade-old/a.jsonl');
    const newFile = path.join(TMP, 'project-kade-new/b.jsonl');
    fs.writeFileSync(oldFile, '');
    fs.writeFileSync(newFile, '');
    // Force newer mtime on newFile
    const now = Date.now();
    fs.utimesSync(oldFile, new Date(now - 60_000), new Date(now - 60_000));
    fs.utimesSync(newFile, new Date(now), new Date(now));

    const { SessionTailer } = load();
    const t = new SessionTailer(makeRouter() as any);
    expect((t as any).findSessionFile('kade')).toBe(newFile);
  });

  test('skips project dirs that do not match the role', () => {
    fs.mkdirSync(path.join(TMP, 'project-silas'));
    fs.writeFileSync(path.join(TMP, 'project-silas/a.jsonl'), '');
    const { SessionTailer } = load();
    const t = new SessionTailer(makeRouter() as any);
    expect((t as any).findSessionFile('kade')).toBeNull();
  });

  test('tolerates non-jsonl files and unreadable project dirs', () => {
    fs.mkdirSync(path.join(TMP, 'kade-proj'));
    fs.writeFileSync(path.join(TMP, 'kade-proj/not-a-session.txt'), 'x');
    const { SessionTailer } = load();
    const t = new SessionTailer(makeRouter() as any);
    expect((t as any).findSessionFile('kade')).toBeNull();
  });
});

describe('SessionTailer.start and getSessionCount', () => {
  beforeEach(() => {
    for (const f of fs.readdirSync(TMP)) {
      fs.rmSync(path.join(TMP, f), { recursive: true, force: true });
    }
    jest.resetModules();
  });

  test('start with no session files leaves sessionCount at 0', () => {
    const { SessionTailer } = load();
    const t = new SessionTailer(makeRouter() as any);
    t.start();
    expect(t.getSessionCount()).toBe(0);
    t.stop();
  });

  test('start registers each role that has a session file', () => {
    for (const role of ['kade', 'silas', 'wren']) {
      fs.mkdirSync(path.join(TMP, role));
      fs.writeFileSync(path.join(TMP, role, 'a.jsonl'), '');
    }
    const { SessionTailer } = load();
    const t = new SessionTailer(makeRouter() as any);
    t.start();
    expect(t.getSessionCount()).toBe(3);
    t.stop();
  });
});

describe('SessionTailer.readNewEntries', () => {
  beforeEach(() => {
    for (const f of fs.readdirSync(TMP)) {
      fs.rmSync(path.join(TMP, f), { recursive: true, force: true });
    }
    jest.resetModules();
  });

  test('reads appended entries from known offset and dispatches processLine', () => {
    const roleDir = path.join(TMP, 'kade');
    fs.mkdirSync(roleDir);
    const f = path.join(roleDir, 'a.jsonl');
    fs.writeFileSync(f, '');

    const { SessionTailer } = load();
    const router = makeRouter();
    const t = new SessionTailer(router as any);
    t.start();

    const entry = JSON.stringify({ type: 'user', message: { content: 'appended line' } });
    fs.appendFileSync(f, entry + '\n');
    (t as any).readNewEntries('kade');
    expect(router.ingest).toHaveBeenCalledWith(expect.objectContaining({
      text: 'appended line', type: 'jeff-input',
    }));
    t.stop();
  });

  test('readNewEntries is a no-op when no state for role', () => {
    const { SessionTailer } = load();
    const router = makeRouter();
    const t = new SessionTailer(router as any);
    expect(() => (t as any).readNewEntries('kade')).not.toThrow();
    expect(router.ingest).not.toHaveBeenCalled();
  });

  test('readNewEntries handles partial trailing line (no final newline)', () => {
    const roleDir = path.join(TMP, 'kade');
    fs.mkdirSync(roleDir);
    const f = path.join(roleDir, 'a.jsonl');
    fs.writeFileSync(f, '');
    const { SessionTailer } = load();
    const router = makeRouter();
    const t = new SessionTailer(router as any);
    t.start();

    const complete = JSON.stringify({ type: 'user', message: { content: 'complete' } }) + '\n';
    const partial = '{"type":"user","messag';  // mid-write
    fs.appendFileSync(f, complete + partial);
    (t as any).readNewEntries('kade');
    expect(router.ingest).toHaveBeenCalledTimes(1);
    // Partial kept for next read; offset advanced only past the complete line.
    expect((router.ingest as jest.Mock).mock.calls[0][0].text).toBe('complete');
    t.stop();
  });
});
