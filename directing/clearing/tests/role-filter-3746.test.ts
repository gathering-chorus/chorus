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

describe('#3746 filterTrees — prompt targeting (Jeff live-demo catch)', () => {
  const targeted = [
    m('jeff', 'jeff-input', '@silas is 3745 done', '2026-08-04T11:00:00Z'),
    m('silas', 'role-response', 'silas reply', '2026-08-04T11:00:10Z'),
    m('jeff', 'jeff-input', '@wren status', '2026-08-04T11:01:00Z'),
    m('wren', 'role-response', 'wren reply', '2026-08-04T11:01:10Z'),
    m('jeff', 'jeff-input', 'untargeted thought', '2026-08-04T11:02:00Z'),
  ];

  test('NEGATIVE PROOF: a prompt @-addressed only to an unselected role is pruned WHOLE — no orphaned questions', () => {
    const trees = filterTrees(buildPromptTrees(targeted), new Set(['wren']));
    const prompts = trees.map((t: any) => t.prompt && t.prompt.text);
    expect(prompts).not.toContain('@silas is 3745 done');
    expect(prompts).toContain('@wren status');
  });

  test('an @-less prompt survives any filter (its target is unknowable after the fact)', () => {
    const trees = filterTrees(buildPromptTrees(targeted), new Set(['wren']));
    expect(trees.map((t: any) => t.prompt && t.prompt.text)).toContain('untargeted thought');
  });
});

describe('#3746 closing = last substantive message, thought or announce (Jeff JX catch)', () => {
  test('a role whose final word is a THOUGHT still shows it as the closing node', () => {
    const stream2 = [
      m('jeff', 'jeff-input', 'talk to me', '2026-08-04T12:00:00Z'),
      m('wren', 'pm-thinking', 'working on it', '2026-08-04T12:00:10Z'),
      m('wren', 'pm-thinking', 'here is the answer', '2026-08-04T12:00:20Z'),
    ];
    const [tree] = buildPromptTrees(stream2);
    const closing = tree.children.filter((c: any) => c.closing);
    expect(closing).toHaveLength(1);
    expect(closing[0].msg.text).toBe('here is the answer');
  });
});
