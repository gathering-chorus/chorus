/**
 * #3351 — unit coverage for the NEW domain-page builders added to
 * public/js/domain-renderer.js: THE SET catalog (setRowsHtml) and the
 * board-reachable Cards section (cardRowsHtml). Pure builders, node-required.
 * Cold-eyes (#3351 gate run) flagged these as untested — this closes that.
 */
// eslint-disable-next-line @typescript-eslint/no-var-requires
const R = require('../public/js/domain-renderer.js');

describe('#3351 domain-page new builders', () => {
  test('setRowsHtml renders THE SET — one clickable catalog row per domain', () => {
    const h = R.setRowsHtml([
      { name: 'messages', label: 'messages', step: 'Directing', owner: 'Wren', status: 'operating' },
      { name: 'heralds', label: 'heralds', step: 'Reflecting', owner: 'Silas', status: 'operating' },
    ]);
    expect(h).toContain('<table class="table">');
    expect(h).toContain('?id=messages');
    expect(h).toContain('?id=heralds');
    expect(h).toContain('Directing');
    expect(h).toContain('operating');
  });

  test('setRowsHtml escapes domain fields (no injection)', () => {
    const h = R.setRowsHtml([{ name: 'x', label: '<x>', step: 's', owner: 'o', status: 'st' }]);
    expect(h).toContain('&lt;x&gt;');
    expect(h).not.toContain('<x>');
  });

  test('cardRowsHtml links each card through to the board (derived + reachable)', () => {
    const h = R.cardRowsHtml([{ id: '3351', title: 'demo card', owner: 'wren', status: 'WIP', priority: 'P1' }]);
    expect(h).toContain('http://localhost:3456/tasks/3351');
    expect(h).toContain('#3351');
    expect(h).toContain('demo card');
    expect(h).toContain('WIP');
  });

  test('cardRowsHtml escapes titles and caps the list at 40 with a board link', () => {
    const small = R.cardRowsHtml([{ id: '1', title: '<x>', owner: 'wren', status: 'WIP', priority: 'P2' }]);
    expect(small).toContain('&lt;x&gt;');
    const many = [];
    for (let i = 0; i < 45; i = i + 1) many.push({ id: String(i), title: 't', owner: 'wren', status: 'WIP', priority: 'P2' });
    const h = R.cardRowsHtml(many);
    expect(h).toContain('and 5 more');
    expect(h).toContain('on the board');
  });
});
