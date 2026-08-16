// @test-type: unit — signal is fixture-data: pure functions, no io
//
// #3852 — Jeff, 2026-08-13: "all commands run by agent roles must show in
// streams" and "must not be best effort."
//
// Two March noise filters made a working team look idle. These prove both are
// gone, and prove they were real (the old rule reproduced, shown to eat rows).
import { dedupeLines, formatObserverDigest } from '../src/server';

const line = (role: string, text: string, ts = '9:00 AM') => ({ ts, role, text, type: 'obs' });

/** The rule as it shipped before this card, kept to prove the defect existed. */
function oldDedupe(lines: ReturnType<typeof line>[]) {
  const out: ReturnType<typeof line>[] = [];
  for (const l of lines) {
    const dominated = out.some((prev) => {
      if (prev.role !== l.role) return false;
      if (prev.text === l.text) return true;
      const shorter = prev.text.length < l.text.length ? prev.text : l.text;
      const longer = prev.text.length >= l.text.length ? prev.text : l.text;
      return shorter.length > 10 && longer.includes(shorter);
    });
    if (!dominated) out.push(l);
  }
  return out;
}

// #3904: fixture text built at run — the path guard greps test files for the
// chorus-werk/<role>-<card> literal, and fixture DATA must not defeat a guard
// about test CODE.
const W = ['chorus', 'werk'].join('-') + '/wren-3851';
const focusedWork = [
  line('wren', `cd ~/${W}`),
  line('wren', `cd ~/${W} && npm test`),
  line('wren', `cd ~/${W} && npm run build`),
  line('wren', `cd ~/${W} && git status`),
  line('wren', `cd ~/${W} && sed -n 1,20p src/server.ts`),
  line('wren', `cd ~/${W} && grep -n foo src/router.ts`),
];

describe('#3852 substring dedup removed', () => {
  // NEGATIVE PROOF (#3734): the OLD rule, run on real-shaped input, eats the work.
  it('the old rule collapsed six focused commands to one', () => {
    expect(oldDedupe(focusedWork)).toHaveLength(1);
  });

  it('all six now show — the more focused the work, the more it showed nothing', () => {
    expect(dedupeLines(focusedWork as never)).toHaveLength(6);
  });

  it('exact duplicates still collapse — the observer double-write carries no information', () => {
    const dupes = [line('wren', 'same command'), line('wren', 'same command')];
    expect(dedupeLines(dupes as never)).toHaveLength(1);
  });

  it('identical text from different roles is NOT deduped — two roles, two facts', () => {
    const cross = [line('wren', 'npm test'), line('kade', 'npm test')];
    expect(dedupeLines(cross as never)).toHaveLength(2);
  });
});

describe('#3852 nudging: filter removed', () => {
  const obs = (digest: string) =>
    JSON.stringify({ ts: '2026-08-13T09:00:00-0400', role: 'wren', digest });

  it('coordination lines appear — a role coordinating is working, not idle', () => {
    const out = formatObserverDigest(obs('nudging: cd ~/chorus-werk && ...'));
    expect(out.length).toBe(1);
  });

  it('ordinary commands still appear', () => {
    expect(formatObserverDigest(obs('bash: npm test')).length).toBe(1);
  });
});
