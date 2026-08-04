// @test-type: unit — pure view-model functions, no DOM, no live services.
/**
 * #3746 — role filtering (Mark's case: the room narrowed to the role he works
 * with) + chrome compaction (Jeff: "the werk-demo styling and length of
 * messages is a lot"). Pure view-model: filtering prunes trees, never breaks
 * threading (#3747), and never hides system/delivery lines or human messages.
 */

const { buildPromptTrees, filterTrees, chromeLabel } = require('../src/../public/clearing-tree.js');

const m = (from: string, type: string, text: string, ts: string) =>
  ({ from, type, text, ts, visible: true });

const stream = [
  m('jeff', 'jeff-input', 'prompt A', '2026-08-04T10:00:00Z'),
  m('wren', 'role-response', 'wren answer', '2026-08-04T10:00:10Z'),
  m('silas', 'role-response', 'silas answer', '2026-08-04T10:00:20Z'),
  m('system', 'system', 'delivery to kade failed: x', '2026-08-04T10:00:25Z'),
  m('marknakib', 'role-response', 'mark comment', '2026-08-04T10:00:30Z'),
];

describe('#3746 filterTrees', () => {
  test('empty selection = full room (default)', () => {
    const trees = filterTrees(buildPromptTrees(stream), new Set());
    expect(trees[0].children).toHaveLength(4);
  });

  test('single-role filter keeps that role, prompts, humans, and system lines', () => {
    const trees = filterTrees(buildPromptTrees(stream), new Set(['wren']));
    const texts = trees[0].children.map((c: any) => c.msg.text);
    expect(texts).toContain('wren answer');
    expect(texts).toContain('delivery to kade failed: x'); // system never pruned
    expect(texts).toContain('mark comment'); // humans never pruned
    expect(trees[0].prompt.text).toBe('prompt A'); // prompts always kept
  });

  test('NEGATIVE PROOF: with a filter active, an unselected role\'s message is NOT rendered', () => {
    const trees = filterTrees(buildPromptTrees(stream), new Set(['wren']));
    const texts = trees[0].children.map((c: any) => c.msg.text);
    expect(texts).not.toContain('silas answer');
  });

  test('multi-role selection (1..n) keeps every selected role', () => {
    const trees = filterTrees(buildPromptTrees(stream), new Set(['wren', 'silas']));
    const texts = trees[0].children.map((c: any) => c.msg.text);
    expect(texts).toContain('wren answer');
    expect(texts).toContain('silas answer');
  });

  test('filtering never mutates the input trees (rendering only)', () => {
    const trees = buildPromptTrees(stream);
    filterTrees(trees, new Set(['wren']));
    expect(trees[0].children).toHaveLength(4);
  });
});

describe('#3746 chromeLabel — werk traffic compacts to one line', () => {
  test('a five-sentence demo announce compacts to its first line, truncated', () => {
    const long = 'Demo ready for your GO — #3747 (round f0212b8bc02e). Peers reviewed; variant: http://x. ' +
      'Look at it, then run `werk-demo go 3747` to land it, or no/more to hold. Take however long you need.';
    const label = chromeLabel(m('silas', 'demo-ready', long, 't'));
    expect(label.length).toBeLessThanOrEqual(120);
    expect(label).toContain('#3747');
  });

  test('a short chrome line passes through intact', () => {
    expect(chromeLabel(m('jeff', 'accept-request', 'Accepted #3747', 't'))).toBe('Accepted #3747');
  });
});
