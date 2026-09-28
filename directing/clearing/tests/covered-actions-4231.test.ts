// @test-type: unit
// #4231 — a call that already has a reason line must not also show as a bare "▸ Bash".
import { dropCoveredActions } from '../src/spine-tail';

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
