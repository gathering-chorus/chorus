// @test-type: unit
// #4231 — a call that already has a reason line must not also show as a bare "▸ Bash".
import { collapseBeats, dropCoveredActions, parseLogEntryForTest } from '../src/spine-tail';

const action = (ts: string, role: string, tool: string) =>
  ({ ts, role, type: 'action', text: `▸ ${tool}`, tool });
const obs = (ts: string, role: string, tool: string, text: string) =>
  ({ ts, role, type: 'obs', text, tool });

describe('#4231 bare action lines give way to the reason line', () => {
  it('drops the bare line when the same call has a reason line', () => {
    const out = dropCoveredActions([
      action('2026-09-28T12:02:01Z', 'silas', 'Bash'),
      obs('2026-09-28T08:02:04-0400', 'silas', 'Bash', 'Read handle_write_stamped · bash: sed -n 1,9p lib.rs'),
    ]);
    expect(out.map((l) => l.text)).toEqual(['Read handle_write_stamped · bash: sed -n 1,9p lib.rs']);
  });

  it('keeps a bare line no reason line covers — reads still show', () => {
    const out = dropCoveredActions([
      action('2026-09-28T12:02:01Z', 'silas', 'Read'),
      obs('2026-09-28T08:02:04-0400', 'silas', 'Bash', 'x · bash: ls'),
      action('2026-09-28T12:02:01Z', 'kade', 'Bash'),
      action('2026-09-28T12:05:00Z', 'silas', 'Bash'),
    ]);
    expect(out.filter((l) => l.type === 'action').map((l) => `${l.role} ${l.text}`))
      .toEqual(['silas ▸ Read', 'kade ▸ Bash', 'silas ▸ Bash']);
  });
});

describe('#4231 cover can reach further back than the rendered lines', () => {
  it('drops a bare line whose reason line is only in the deeper read', () => {
    const lines = [action('2026-09-28T12:09:50Z', 'wren', 'mcp__x')];
    const cover = [obs('2026-09-28T08:09:52-0400', 'wren', 'mcp__x', 'mcp: x → jeff')];
    expect(dropCoveredActions(lines, cover)).toEqual([]);
    expect(dropCoveredActions(lines)).toHaveLength(1);
  });
});

describe('#4231 a slow call pairs its start with its own finish', () => {
  it('drops the bare line when the reason line lands minutes later', () => {
    const out = dropCoveredActions([
      action('2026-09-28T12:09:10Z', 'wren', 'mcp__nudge'),
      obs('2026-09-28T08:09:52-0400', 'wren', 'mcp__nudge', 'mcp: nudge → jeff'),
    ]);
    expect(out.map((l) => l.type)).toEqual(['obs']);
  });

  it('one reason line covers one call, and a running call stays bare', () => {
    const out = dropCoveredActions([
      action('2026-09-28T12:00:00Z', 'wren', 'Bash'),
      obs('2026-09-28T08:00:05-0400', 'wren', 'Bash', 'a · bash: ls'),
      action('2026-09-28T12:00:10Z', 'wren', 'Bash'),
    ]);
    expect(out.filter((l) => l.type === 'action').map((l) => l.ts)).toEqual(['2026-09-28T12:00:10Z']);
  });
});

describe('#4231 a long call says what is running, once', () => {
  it('the start line and the heartbeat carry the reason · command', () => {
    const start = parseLogEntryForTest({ timestamp: 't', role: 'kade', event: 'agent.action', tool: 'Bash', digest: 'Run the suite · bash: bats x' } as never);
    const beat = parseLogEntryForTest({ timestamp: 't', role: 'kade', event: 'agent.activity', phase: 'running', tool: 'Bash', digest: 'Run the suite · bash: bats x', elapsed_s: 86 } as never);
    expect(start?.text).toBe('▸ Run the suite · bash: bats x');
    expect(beat?.text).toBe('⏳ running Run the suite · bash: bats x (86s)');
    // negative proof: no digest → the old tool-only text, never an empty line
    const bare = parseLogEntryForTest({ timestamp: 't', role: 'kade', event: 'agent.activity', phase: 'running', tool: 'Bash', elapsed_s: 66 } as never);
    expect(bare?.text).toBe('⏳ running Bash (66s)');
  });

  it('keeps only the newest heartbeat in a run of beats from one role', () => {
    const beat = (ts: string, role: string, s: number) => ({ ts, role, type: 'activity', text: `⏳ running Bash (${s}s)` });
    const werk = { ts: '3', role: 'silas', type: 'werk', text: '⚙ werk: build' };
    const out = collapseBeats([beat('1', 'kade', 66), werk, beat('4', 'kade', 86), beat('5', 'kade', 106),
      { ts: '6', role: 'kade', type: 'obs', text: 'done · bash: x' }, beat('7', 'kade', 61)]);
    expect(out.map((l) => l.text)).toEqual(['⚙ werk: build', '⏳ running Bash (106s)', 'done · bash: x', '⏳ running Bash (61s)']);
  });
});
