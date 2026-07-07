// #3545 — generic entity renderer. PROJECTION: renders ANY Athena class from its
// collection + the entity's own fields, with `contains` children nested. Domain has its
// rich renderer (domain-renderer.js); this covers ValueStream, Product, Service, etc.
// Reads window.OWL_CLASS / window.OWL_COLLECTION injected by page_html. Never hand-edit
// the page — regenerate from the model.
(function () {
  // #3627 — collections come from owl-api (:3360, OWL_PORT injected by page_html).
  // The old base '/api/athena' was retired by #3603; fetching it 404'd and the
  // swallowed error rendered as "No instances in the model" — a lie. CORS on
  // owl-api (#3373) exists precisely for this :3340-page → :3360-API read.
  var API = location.protocol + '//' + location.hostname + ':' + (window.OWL_PORT || 3360);
  var CLASS = window.OWL_CLASS || '';
  var COLL = window.OWL_COLLECTION || '';
  var FETCH_FAILED = false;
  function get(u) {
    return fetch(API + u)
      .then(function (r) { if (!r.ok) { FETCH_FAILED = true; return null; } return r.json(); })
      .catch(function () { FETCH_FAILED = true; return null; });
  }
  function esc(s) { var d = document.createElement('div'); d.textContent = (s == null ? '' : String(s)); return d.innerHTML; }
  // scalar fields we don't surface as rows (identity / housekeeping)
  var HIDE = { name: 1, label: 1, iri: 1, type: 1, created: 1, creator: 1, modified: 1 };
  // display name for a contained ref ("value-stream-step-athena-model" -> "Model")
  function pretty(n) {
    var s = String(n).replace(/^value-stream-step-/, '');
    var seg = s.split('-'); var w = seg[seg.length - 1] || s;
    return w.charAt(0).toUpperCase() + w.slice(1);
  }
  function fieldRows(o) {
    var rows = '';
    Object.keys(o).forEach(function (k) {
      if (HIDE[k] || k.charAt(0) === '_') return;
      var v = o[k];
      if (v == null || v === '' || (Array.isArray(v) && !v.length)) return;
      if (Array.isArray(v)) v = v.join(', ');
      rows += '<tr><td class="er-k">' + esc(k) + '</td><td>' + esc(v) + '</td></tr>';
    });
    return rows;
  }
  function style() {
    var c = '.er-card{background:#fff;border:1px solid var(--border,#e2e8f0);border-radius:10px;padding:16px 18px;margin:0 0 12px}' +
      '.er-card h2{font-size:17px;margin:0 0 8px}' +
      '.er-kt{border-collapse:collapse;width:100%}' +
      '.er-kt td{border-top:1px solid #f1f5f9;padding:5px 8px;vertical-align:top;font-size:13px}' +
      '.er-k{color:#94a3b8;width:130px;text-transform:uppercase;font-size:10.5px;letter-spacing:.04em}' +
      '.er-row{display:flex;flex-wrap:wrap;align-items:center;gap:6px;margin-top:10px}' +
      '.er-chip{background:#eef2f7;border:1px solid #dbe3ec;border-radius:6px;padding:3px 9px;font-size:12px;font-weight:600}' +
      '.er-ar{color:#cbd5e1}';
    var s = document.createElement('style'); s.textContent = c; document.head.appendChild(s);
  }
  function render(items) {
    var title = document.getElementById('domain-title');
    // #3627 — never headline a count we didn't fetch: "(0)" on a dead API reads
    // as "your model is empty", which is the exact lie this card removes.
    if (title) title.textContent = (FETCH_FAILED && !items.length) ? CLASS : CLASS + ' (' + items.length + ')';
    var host = document.getElementById('content-sections');
    if (!host) return;
    Promise.all(items.map(function (it) {
      return get('/' + COLL + '/' + encodeURIComponent(it.name) + '/contains').then(function (c) {
        var kids = (c && (c.contains || c.data)) || [];
        return { item: it, kids: kids };
      });
    })).then(function (rows) {
      var html = '';
      rows.forEach(function (r) {
        var it = r.item;
        var chips = '';
        if (r.kids && r.kids.length) {
          chips = '<div class="er-row">' + r.kids.map(function (k) {
            return '<span class="er-chip">' + esc(typeof k === 'string' ? pretty(k) : (k.label || pretty(k.name))) + '</span>';
          }).join(' <span class="er-ar">&rarr;</span> ') + '</div>';
        }
        html += '<section class="er-card"><h2>' + esc(it.label || it.name) + '</h2>' +
          '<table class="er-kt">' + fieldRows(it) + '</table>' + chips + '</section>';
      });
      // #3627 — an unreachable API must never render as an empty model.
      host.innerHTML = html || (FETCH_FAILED
        ? '<p class="muted">Cannot reach the model API at ' + esc(API) + ' — this is a fetch failure, not an empty model.</p>'
        : '<p class="muted">No ' + esc(CLASS) + ' instances in the model.</p>');
    });
  }
  if (!COLL) return;
  style();
  get('/' + COLL).then(function (res) { render((res && res.data) || []); });
})();
