// @test-type: unit — pure view-model functions, no DOM, no live services.
/**
 * #3747 — prompt-tree view-model. RENDERING EXERCISE ONLY (Jeff's pull
 * constraint): these functions derive a tree from the message stream the room
 * already captures — they never mutate, filter out, or re-classify what is
 * stored. Shape per Jeff: each human prompt is a root; under it 1..n THOUGHT
 * nodes (collapsed) + one CLOSING ANNOUNCE per role (visible).
 */

const { classifyNode, buildPromptTrees } = require('../public/clearing-tree.js');

const m = (from: string, type: string, text: string, ts: string) =>
  ({ from, type, text, ts, visible: true });

describe('#3747 classifyNode — one treatment per message class', () => {
  test('human prompts, role announces, thoughts, werk chrome each classify distinctly', () => {
    expect(classifyNode(m('jeff', 'jeff-input', 'do the thing', 't'))).toBe('prompt');
    expect(classifyNode(m('marknakib', 'jeff-input', 'hi', 't'))).toBe('prompt');
    expect(classifyNode(m('wren', 'role-response', 'done', 't'))).toBe('announce');
    expect(classifyNode(m('wren', 'pm-thinking', 'hmm', 't'))).toBe('thought');
    expect(classifyNode(m('silas', 'demo-ready', 'Demo ready: #1', 't'))).toBe('chrome');
    expect(classifyNode(m('jeff', 'accept-request', 'Accepted #1', 't'))).toBe('chrome');
    expect(classifyNode(m('kade', 'blocked', 'BLOCKED on x', 't'))).toBe('chrome');
    expect(classifyNode(m('system', 'system-error', 'boom', 't'))).toBe('error');
  });

  test('NEGATIVE PROOF: the same type classifies identically regardless of arrival path metadata', () => {
    const viaSocket = { from: 'silas', type: 'demo-ready', text: 'Demo ready: #9', ts: 't', visible: true };
    const viaTailer = { from: 'silas', type: 'demo-ready', text: 'Demo ready: #9', ts: 't', visible: true, source: 'tailer', trace: 'x' };
    expect(classifyNode(viaSocket)).toBe(classifyNode(viaTailer));
  });
});

describe('#3747 buildPromptTrees — 1..n thoughts + 1 closing announce', () => {
  const stream = [
    m('jeff', 'jeff-input', 'prompt A', '2026-08-04T10:00:00Z'),
    m('wren', 'pm-thinking', 'thinking 1', '2026-08-04T10:00:10Z'),
    m('wren', 'role-response', 'interim answer', '2026-08-04T10:00:20Z'),
    m('silas', 'demo-ready', 'Demo ready: #7', '2026-08-04T10:00:25Z'),
    m('wren', 'role-response', 'final answer A', '2026-08-04T10:00:30Z'),
    m('jeff', 'jeff-input', 'prompt B', '2026-08-04T10:01:00Z'),
    m('kade', 'role-response', 'answer B', '2026-08-04T10:01:10Z'),
  ];

  test('messages thread under the most recent preceding prompt', () => {
    const trees = buildPromptTrees(stream);
    expect(trees).toHaveLength(2);
    expect(trees[0].prompt.text).toBe('prompt A');
    expect(trees[0].children).toHaveLength(4);
    expect(trees[1].children).toHaveLength(1);
  });

  test('the LAST announce per role is the closing announce; earlier ones fold as thoughts', () => {
    const [a] = buildPromptTrees(stream);
    const closing = a.children.filter((c: any) => c.closing);
    expect(closing).toHaveLength(1);
    expect(closing[0].msg.text).toBe('final answer A');
    const folded = a.children.filter((c: any) => !c.closing);
    expect(folded.map((c: any) => c.msg.text)).toEqual(['thinking 1', 'interim answer', 'Demo ready: #7']);
  });

  test('NEGATIVE PROOF: a message belonging to prompt A can never land under prompt B (interleaved arrival)', () => {
    const outOfOrder = [stream[0], stream[5], stream[4], stream[6]]; // A, B, then A's late answer arrives after B
    const trees = buildPromptTrees(outOfOrder);
    expect(trees[0].children.map((c: any) => c.msg.text)).toContain('final answer A');
    expect(trees[1].children.map((c: any) => c.msg.text)).not.toContain('final answer A');
  });

  test('messages before any prompt go to a synthetic root, never dropped (capture retained)', () => {
    const trees = buildPromptTrees([m('silas', 'role-response', 'orphan', '2026-08-04T09:59:00Z'), ...stream]);
    expect(trees[0].prompt).toBeNull();
    expect(trees[0].children[0].msg.text).toBe('orphan');
    const total = trees.reduce((n: number, t: any) => n + t.children.length + (t.prompt ? 1 : 0), 0);
    expect(total).toBe(8); // every input message appears exactly once
  });

  test('invisible messages stay out of the tree (same rule the flat view had)', () => {
    const trees = buildPromptTrees([...stream, { ...m('probe', 'probe', 'x', '2026-08-04T10:02:00Z'), visible: false }]);
    expect(trees.reduce((n: number, t: any) => n + t.children.length, 0)).toBe(5);
  });
});
