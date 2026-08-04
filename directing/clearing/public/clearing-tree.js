/**
 * #3747 — prompt-tree view-model for the Clearing transcript.
 *
 * RENDERING LAYER ONLY (Jeff's pull constraint): pure functions from the
 * captured message stream to a display tree. Nothing here mutates, stores, or
 * re-routes a message — ingest, router classification, persistence and
 * delivery are untouched. Every visible message appears in exactly one tree.
 *
 * Shape (Jeff, 2026-08-04): each human prompt is a root; under it 1..n THOUGHT
 * nodes (intermediate turns, werk chrome — collapsed) + one CLOSING ANNOUNCE
 * per role (the node that shows when the tree is collapsed).
 *
 * UMD-lite: usable as a browser script (window.ClearingTree) and from jest.
 */
(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.ClearingTree = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  var HUMAN_PROMPT_TYPES = { 'jeff-input': true };
  var CHROME_TYPES = { 'demo-ready': true, 'accept-request': true, 'blocked': true, 'system': true, 'probe': true };

  /**
   * One treatment per message class — driven ONLY by (type, from), never by
   * arrival path, so the same message renders identically from socket, tailer
   * or spine (#3747 negative proof).
   */
  function classifyNode(msg) {
    var type = msg.type || '';
    if (type === 'system-error') return 'error';
    if (HUMAN_PROMPT_TYPES[type]) return 'prompt';
    if (CHROME_TYPES[type]) return 'chrome';
    if (type === 'pm-thinking') return 'thought';
    if (type === 'role-response' || type === 'role-to-role') return 'announce';
    return 'announce';
  }

  /**
   * Thread the stream into prompt trees. Threading is by TIMESTAMP order
   * against prompt timestamps — not arrival order — so a late-arriving message
   * for prompt A can never land under prompt B.
   * Messages before the first prompt go to a synthetic root (prompt: null):
   * rendering may fold them, but capture is retained — nothing is dropped.
   */
  function buildPromptTrees(messages) {
    var visible = (messages || []).filter(function (m) { return m && m.visible !== false; });
    var sorted = visible.slice().sort(function (a, b) {
      return String(a.ts).localeCompare(String(b.ts));
    });

    var trees = [];
    var current = null;
    for (var i = 0; i < sorted.length; i++) {
      var msg = sorted[i];
      var cls = classifyNode(msg);
      if (cls === 'prompt') {
        current = { prompt: msg, children: [] };
        trees.push(current);
        continue;
      }
      if (!current) {
        current = { prompt: null, children: [] };
        trees.push(current);
      }
      current.children.push({ msg: msg, cls: cls, closing: false });
    }

    // Per tree, per role: the LAST substantive message — announce OR thought —
    // is the closing announce; everything earlier folds. Jeff's spec verbatim
    // was "just show the latest response/thought": when a role's final word
    // arrives typed as thinking (wren's replies do), it is still the reply,
    // and folding it hid the conversation from the person having it
    // (live-demo catch, 2026-08-04).
    for (var t = 0; t < trees.length; t++) {
      var lastByRole = {};
      trees[t].children.forEach(function (child) {
        if (child.cls === 'announce' || child.cls === 'thought') lastByRole[child.msg.from] = child;
      });
      Object.keys(lastByRole).forEach(function (role) {
        lastByRole[role].closing = true;
      });
    }
    return trees;
  }

  var AGENT_ROLES = { wren: true, silas: true, kade: true };

  /**
   * #3746 — role filter (Mark's case). Prunes tree children whose sender is an
   * UNSELECTED agent role; empty selection = full room. Prompts, human senders
   * and system/error lines are never pruned — the filter narrows which agents
   * you're listening to, it can never hide what was said to you or by you.
   * Pure: returns new trees, never mutates the input (rendering only).
   */
  function filterTrees(trees, selected) {
    if (!selected || selected.size === 0) return trees;
    return (trees || []).filter(function (tree) {
      // Jeff's live-demo catch: a prompt @-addressed ONLY to unselected roles
      // is pruned WHOLE — otherwise the filtered view shows orphaned questions
      // whose answers were pruned. An @-less prompt survives any filter: its
      // send-time target is not recorded on the message, so hiding it would be
      // guessing, and the filter must never guess a human's words away.
      if (!tree.prompt) return true;
      var mentions = (String(tree.prompt.text || '').match(/@(wren|silas|kade)/gi) || [])
        .map(function (mn) { return mn.slice(1).toLowerCase(); });
      if (mentions.length === 0) return true;
      return mentions.some(function (r) { return selected.has(r); });
    }).map(function (tree) {
      return {
        prompt: tree.prompt,
        children: tree.children.filter(function (child) {
          var from = child.msg.from || '';
          if (!AGENT_ROLES[from]) return true; // humans, system, guests — never pruned
          if (child.cls === 'error') return true;
          return selected.has(from);
        }),
      };
    });
  }

  /**
   * #3746 — chrome compaction. Jeff: "the werk-demo styling and length of
   * messages is a lot." Werk/system traffic renders as ONE line: first
   * sentence-ish fragment, hard-capped — the full text lives behind the
   * tree's expander, never in the collapsed view.
   */
  var CHROME_MAX = 120;
  function chromeLabel(msg) {
    var text = String(msg.text || '').split('\n')[0];
    var cut = text.search(/\.\s/);
    if (cut > 0 && cut < CHROME_MAX) text = text.slice(0, cut);
    if (text.length > CHROME_MAX) text = text.slice(0, CHROME_MAX - 1) + '…';
    return text;
  }

  return {
    classifyNode: classifyNode,
    buildPromptTrees: buildPromptTrees,
    filterTrees: filterTrees,
    chromeLabel: chromeLabel,
  };
});
