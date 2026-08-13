// @test-type: unit — signal is fixture-data: in-memory router, no io
//
// #3852 — Jeff, 2026-08-13: "i frequently see messages in claude code that are
// not showing in clearing even folded" and "this is why i want the folding
// behavior it is also unreliable in how it works."
//
// Both were true, and they were two different defects.
import { MessageRouter } from '../src/router';

const mk = (from: string, text: string, type?: string) => ({
  from, text, ts: new Date().toISOString(), type,
});

// #3862 — #3852 tagged mid-turn rows by turn state, which was right, and then
// removed them from the room, which was not. `stop_reason` is 'end_turn' only
// when a turn ends in plain text, so a finished reply written after tool calls
// carries the same tag as narration. The rule could not separate the two states
// it existed to separate, and it deleted the wrong one.
//
// Jeff, 2026-08-13, on what folding actually did: "the ui definitely does not
// show collapsed messages that i can unfold - they literally are NOT THERE".
describe('#3862 mid-turn rows are typed, not removed', () => {
  it('keeps narration in the room, typed pm-thinking', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'Looking at the second path now, this one is trickier.', 'pm-thinking'));
    const [m] = r.getRecent(10);
    expect(m.type).toBe('pm-thinking');
    expect(m.visible).toBe(true);
  });

  it('keeps a row that looks like a tool call — same rule, no exceptions', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'bash: cd /tmp && ls', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(1);
  });

  it('a finished reply is still a message', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'Landed. Three paths capped.'));
    expect(r.getRecent(10).length).toBeGreaterThan(0);
  });

  // NEGATIVE PROOF (#3734): this suite must still be able to observe a hidden
  // row, or it cannot tell "nothing is hidden" from "hiding is broken". Probes
  // are the remaining hidden class — 129 of 200 rows in the live room.
  it('hiding still exists for probes, so this suite can still see the difference', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'mid-turn note', 'pm-thinking'));
    r.ingest(mk('probe', 'heartbeat', 'probe'));
    expect(r.getRecent(10)).toHaveLength(1);
    expect(r.getRecent(10, true)).toHaveLength(2);
  });
});

describe('#3852 the window admits what it withheld', () => {
  const fill = (n: number) => {
    const r = new MessageRouter();
    for (let i = 0; i < n; i++) r.ingest(mk('jeff', `message ${i}`));
    return r;
  };

  // NEGATIVE PROOF: silent truncation is indistinguishable from data loss.
  // 134 replies were sent in one morning and 5 were reachable; nothing was
  // dropped, but the room never said it was a window — so it read as loss.
  it('reports total and withheld when it truncates', () => {
    const w = fill(120).getRecentWindowed(50);
    expect(w.messages).toHaveLength(50);
    expect(w.total).toBe(120);
    expect(w.withheld).toBe(70);
  });

  it('withheld is 0 when nothing is cut — the number is meaningful, not decorative', () => {
    const w = fill(10).getRecentWindowed(50);
    expect(w.messages).toHaveLength(10);
    expect(w.withheld).toBe(0);
  });

  it('a day of work fits: 134 messages are all reachable at the new default', () => {
    const w = fill(134).getRecentWindowed(300);
    expect(w.messages).toHaveLength(134);
    expect(w.withheld).toBe(0);
  });

  // #3862 — the hidden class is probes now, not pm-thinking.
  it('hidden rows do not count against the visible window', () => {
    const r = new MessageRouter();
    for (let i = 0; i < 20; i++) r.ingest(mk('probe', `probe ${i}`, 'probe'));
    for (let i = 0; i < 5; i++) r.ingest(mk('jeff', `real ${i}`));
    const w = r.getRecentWindowed(300);
    expect(w.messages).toHaveLength(5);
    expect(w.total).toBe(5);
  });
});
