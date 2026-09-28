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
