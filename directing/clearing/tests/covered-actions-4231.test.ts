// @test-type: unit
// #4231 — one call is one record: a start and an end joined on call_id.
// Each case below is a class of wrong line the survey found on Jeff's pane.
import { parseLogEntryForTest, resolveCalls, type StreamLine } from '../src/spine-tail';

const NOW = Date.parse('2026-09-28T12:00:00-0400');

const start = (ts: string, role: string, call: string, digest: string, session = 'sess0001') =>
  parseLogEntryForTest({ timestamp: ts, role, event: 'agent.action', phase: 'started', tool: 'Bash', call_id: call, session_id: session, digest } as never) as StreamLine;
const end = (ts: string, role: string, call: string, outcome = 'ok') =>
  parseLogEntryForTest({ timestamp: ts, role, event: 'agent.action', phase: 'ended', tool: 'Bash', call_id: call, outcome } as never) as StreamLine;
const turnEnded = (ts: string, role: string, session = 'sess0001') =>
  parseLogEntryForTest({ timestamp: ts, role, event: 'agent.turn.ended', session_id: session } as never) as StreamLine;

const texts = (lines: StreamLine[]) => lines.map((l) => `${l.role} ${l.text}`);

describe('#4231 a call shows once, as what it is', () => {
  it('a finished call is one line with its reason, never a bare "▸ Bash" beside it', () => {
    const out = resolveCalls([
      start('2026-09-28T11:59:00-0400', 'silas', 'toolu_1', 'Read the lib · bash: sed -n 1,9p lib.rs'),
      end('2026-09-28T11:59:02-0400', 'silas', 'toolu_1'),
    ], NOW);
    expect(texts(out)).toEqual(['silas Read the lib · bash: sed -n 1,9p lib.rs']);
  });

  it('a slow call (a commit, a pipeline) still joins its own end, however long it ran', () => {
    const out = resolveCalls([
      start('2026-09-28T11:40:00-0400', 'wren', 'toolu_2', 'mcp: werk-commit #4231 wren'),
      end('2026-09-28T11:52:00-0400', 'wren', 'toolu_2'),
    ], NOW);
    expect(texts(out)).toEqual(['wren mcp: werk-commit #4231 wren']);
  });

  it('two identical commands each join their own end (no stealing)', () => {
    const out = resolveCalls([
      start('2026-09-28T11:59:00-0400', 'silas', 'toolu_a', 'Poll · bash: cws 4396'),
      start('2026-09-28T11:59:10-0400', 'silas', 'toolu_b', 'Poll · bash: cws 4396'),
      end('2026-09-28T11:59:11-0400', 'silas', 'toolu_b'),
    ], NOW);
    // toolu_a never ended: it is still running; toolu_b is done
    expect(texts(out)).toEqual(['silas ⏳ Poll · bash: cws 4396 (60s)', 'silas Poll · bash: cws 4396']);
  });

  it('a failed call says so', () => {
    const out = resolveCalls([
      start('2026-09-28T11:59:00-0400', 'kade', 'toolu_3', 'Run bats · bash: bats x'),
      end('2026-09-28T11:59:30-0400', 'kade', 'toolu_3', 'error'),
    ], NOW);
    expect(texts(out)).toEqual(['kade Run bats · bash: bats x ✗']);
  });
});

describe('#4231 running means running', () => {
  it('a call with no end is one running line, timed from its own start — no beats', () => {
    const out = resolveCalls([start('2026-09-28T11:55:00-0400', 'kade', 'toolu_4', 'Run the suite · bash: cargo test')], NOW);
    expect(texts(out)).toEqual(['kade ⏳ Run the suite · bash: cargo test (5m)']);
  });

  it('a call whose turn ended without an end reads stopped, never running for 900s', () => {
    const out = resolveCalls([
      start('2026-09-28T11:40:00-0400', 'wren', 'toolu_5', 'skill: /cws'),
      turnEnded('2026-09-28T11:41:00-0400', 'wren'),
    ], NOW);
    expect(texts(out)).toEqual(['wren skill: /cws (stopped)']);
  });

  it('negative proof: another session ending its turn does not stop this call', () => {
    const out = resolveCalls([
      start('2026-09-28T11:59:00-0400', 'wren', 'toolu_6', 'Wait · bash: sleep 30', 'sessAAAA'),
      turnEnded('2026-09-28T11:59:30-0400', 'wren', 'sessBBBB'),
    ], NOW);
    expect(texts(out)).toEqual(['wren ⏳ Wait · bash: sleep 30 (60s)']);
  });

  it('end and turn records are joins, never lines', () => {
    const out = resolveCalls([end('2026-09-28T11:59:00-0400', 'wren', 'toolu_x'), turnEnded('2026-09-28T11:59:00-0400', 'wren')], NOW);
    expect(out).toEqual([]);
  });
});

describe('#4231 what reaches the room', () => {
  it('a start whose digest is machinery (a cards/nudge helper) is not shown, same rule as before', () => {
    expect(start('2026-09-28T11:59:00-0400', 'wren', 'toolu_7', 'bash: cards move 4231 Next')).toBeNull();
  });

  it('an old start with no call_id still shows, marked as a start', () => {
    const legacy = parseLogEntryForTest({ timestamp: '2026-09-28T11:59:00-0400', role: 'wren', event: 'agent.action', phase: 'started', tool: 'Read' } as never) as StreamLine;
    expect(texts(resolveCalls([legacy], NOW))).toEqual(['wren ▸ Read']);
  });

  it('the retired heartbeat renders nothing', () => {
    expect(parseLogEntryForTest({ timestamp: 't', role: 'kade', event: 'agent.activity', phase: 'running', tool: 'Bash', elapsed_s: 66 } as never)).toBeNull();
  });
});
