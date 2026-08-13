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

describe('#3852 folding decides by turn state, not by text', () => {
  // NEGATIVE PROOF (#3734): the old rule let SOME narration through — whichever
  // did not look like a tool call. That is the unreliability: no rule a person
  // could predict. Prose that would have passed the old heuristic must fold now.
  it('folds narration that the old text-heuristic would have shown', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'Looking at the second path now, this one is trickier.', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  it('folds narration that looks like a tool call too — same rule, no exceptions', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'bash: cd /tmp && ls', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  it('a finished reply is still a message — folding must not eat everything', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'Landed. Three paths capped.'));
    expect(r.getRecent(10).length).toBeGreaterThan(0);
  });

  it('folded is HIDDEN, not discarded — includeHidden still returns it', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'mid-turn note', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
    expect(r.getRecent(10, true)).toHaveLength(1);
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

  it('hidden rows do not count against the visible window', () => {
    const r = new MessageRouter();
    for (let i = 0; i < 20; i++) r.ingest(mk('wren', `thinking ${i}`, 'pm-thinking'));
    for (let i = 0; i < 5; i++) r.ingest(mk('jeff', `real ${i}`));
    const w = r.getRecentWindowed(300);
    expect(w.messages).toHaveLength(5);
    expect(w.total).toBe(5);
  });
});
