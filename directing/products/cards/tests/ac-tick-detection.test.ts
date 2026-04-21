/**
 * AC tick detection tests (#2193 wave 1).
 *
 * Pure-function detector that compares old vs new card description and
 * reports how many AC items flipped from `- [ ]` to `- [x]` in the update.
 */

import { countAcDiff } from '../src/ac-tick-detection';

describe('countAcDiff', () => {
  it('reports zero delta when descriptions are identical', () => {
    const d = '## AC\n- [ ] first\n- [x] second\n';
    const r = countAcDiff(d, d);
    expect(r.tickedCount).toBe(0);
    expect(r.totalChecked).toBe(1);
    expect(r.totalAc).toBe(2);
  });

  it('reports one tick when an item flips [ ] → [x]', () => {
    const before = '- [ ] one\n- [ ] two\n- [ ] three\n';
    const after = '- [ ] one\n- [x] two\n- [ ] three\n';
    const r = countAcDiff(before, after);
    expect(r.tickedCount).toBe(1);
    expect(r.totalChecked).toBe(1);
    expect(r.totalAc).toBe(3);
  });

  it('counts multiple ticks in the same update', () => {
    const before = '- [ ] a\n- [ ] b\n- [ ] c\n';
    const after = '- [x] a\n- [x] b\n- [ ] c\n';
    const r = countAcDiff(before, after);
    expect(r.tickedCount).toBe(2);
    expect(r.totalChecked).toBe(2);
    expect(r.totalAc).toBe(3);
  });

  it('does NOT count un-ticks (x → space) as positive ticks', () => {
    const before = '- [x] a\n- [x] b\n';
    const after = '- [ ] a\n- [x] b\n';
    const r = countAcDiff(before, after);
    expect(r.tickedCount).toBe(0);
    expect(r.totalChecked).toBe(1);
    expect(r.totalAc).toBe(2);
  });

  it('handles new AC items added in same update (count toward total, not ticked)', () => {
    const before = '- [ ] a\n';
    const after = '- [x] a\n- [ ] b\n';
    const r = countAcDiff(before, after);
    expect(r.tickedCount).toBe(1);
    expect(r.totalChecked).toBe(1);
    expect(r.totalAc).toBe(2);
  });

  it('ignores lines that are not AC-shaped', () => {
    const before = '# Title\nSome prose.\n- [ ] ac1\n';
    const after = '# Title\nDifferent prose.\n- [x] ac1\n';
    const r = countAcDiff(before, after);
    expect(r.tickedCount).toBe(1);
    expect(r.totalAc).toBe(1);
  });

  it('empty before → all checked items count as new ticks', () => {
    const before = '';
    const after = '- [x] a\n- [x] b\n';
    const r = countAcDiff(before, after);
    expect(r.tickedCount).toBe(2);
    expect(r.totalChecked).toBe(2);
    expect(r.totalAc).toBe(2);
  });

  it('tolerates indentation variations in the checkbox marker', () => {
    const before = '  - [ ] indented\n- [ ] flush\n';
    const after = '  - [x] indented\n- [ ] flush\n';
    const r = countAcDiff(before, after);
    expect(r.tickedCount).toBe(1);
    expect(r.totalAc).toBe(2);
  });
});
