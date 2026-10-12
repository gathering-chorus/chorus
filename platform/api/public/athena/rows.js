// #4081 — one page for any class the door serves: the rows of its collection,
// one column per field the door returns. The door serves exactly the fields the
// shape declares (ADR-048 exposure), so the columns ARE the shape: nothing here
// knows about any one class. Per-class pages are this page with a kind preset
// (schedule.html presets one). Reads the door only — no second API.
(async function () {
  const main = document.getElementById('main');
  const kind = window.ROWS_KIND || new URLSearchParams(location.search).get('kind') || '';
  if (!kind) { main.innerHTML = '<p class="empty">name a class: rows.html?kind=&lt;Class&gt;</p>'; return; }
  document.title = kind + ' — Athena';
  const h = document.getElementById('kind-title');
  if (h) h.textContent = window.ROWS_TITLE || kind;
  try {
    const disc = await fetchJSON('/');
    // discovery lists every served kind with its collection
    const text = JSON.stringify(disc);
    const m = text.match(new RegExp('"kind":\\s*"' + kind + '"[^}]*?"collection":\\s*"([^"]+)"'));
    if (!m) { main.innerHTML = '<div class="err">The door does not serve <strong>' + esc(kind) + '</strong>.</div>'; return; }
    const coll = m[1].replace(/^\/v1/, '');
    const env = await fetchJSON(coll + '?limit=1000');
    const rows = env.data || [];
    const hide = new Set(['status', 'iri']);
    const cols = [];
    rows.forEach(r => Object.keys(r).forEach(k => { if (!hide.has(k) && !cols.includes(k)) cols.push(k); }));
    const order = window.ROWS_COLUMNS || [];
    cols.sort((a, b) => {
      const ia = order.indexOf(a), ib = order.indexOf(b);
      return (ia < 0 ? 999 : ia) - (ib < 0 ? 999 : ib);
    });
    const sortKey = window.ROWS_SORT || 'name';
    rows.sort((a, b) => String(a[sortKey] || '').localeCompare(String(b[sortKey] || '')));
    const cell = v => Array.isArray(v) ? v.map(esc).join('<br>') : esc(v === undefined || v === null ? '' : String(v));
    main.innerHTML =
      '<p class="meta">' + rows.length + ' rows · from <code>' + esc(m[1]) + '</code> · columns are the fields the door serves for ' + esc(kind) + '</p>' +
      '<div class="table-scroll"><table class="rows"><thead><tr>' + cols.map(c => '<th>' + esc((window.ROWS_HEADERS || {})[c] || c) + '</th>').join('') + '</tr></thead><tbody>' +
      rows.map(r => '<tr>' + cols.map(c => '<td>' + cell(r[c]) + '</td>').join('') + '</tr>').join('') +
      '</tbody></table></div>';
  } catch (e) { fetchFailed(main, e); }
})();
