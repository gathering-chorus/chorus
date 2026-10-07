// #3757 — the shared model fold: schema → OWL → instances for one domain.
// ONE implementation used by BOTH altitudes (chorus:principle-no-competing-implementations):
//   /domains/<d>        (model view, full)    — domains-view.html
//   /athena/domain.html (ops view, compact)   — the what-is-this anchor
// Same endpoints, same markup, same honest states; divergence is impossible
// because there is nothing to diverge — the drift test (test-model-fold-drift)
// proves it stays that way.
//
// Endpoints (all same-origin, #3644/#3706):
//   GET /owl/domains                          — vocab edges (collection row)
//   GET /owl/ (discovery)                     — kind → collection
//   GET /api/athena/domain-schema/<d>         — merged ontology+shape schema
//   GET /api/athena/domain-owl/<d>            — the turtle (lazy, on open)
//   GET /owl/<collection>                     — instances per vocabulary class
//
// Honest-states contract (#3749/#3724, the two-states discipline): every failure
// names WHICH state it is — no-vocab, not-served, served-but-zero, endpoint-not-
// on-this-server, store-unreachable — and never renders as a silent blank.

(function () {
  const ESC = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

  const CSS = `
.mfold { font-size: .9rem; }
.mfold h2 { font-size: 1.05rem; margin: 18px 0 8px; }
.mfold h3 { font-size: .92rem; margin: 12px 0 6px; font-family: ui-monospace, Menlo, monospace; }
.mfold .mfnote { background: #fffbe8; border: 1px solid #e8d9a0; border-radius: 6px; padding: 10px 14px; margin: 8px 0; font-size: .85rem; }
.mfold .mferr { background: #fdeaea; border: 1px solid #e5b4b4; border-radius: 6px; padding: 10px 14px; margin: 8px 0; font-size: .85rem; }
.mfold .mfscroll { overflow-x: auto; }
.mfold table { width: 100%; border-collapse: collapse; font-size: .84rem; }
.mfold th { text-align: left; font-size: .7rem; text-transform: uppercase; letter-spacing: .04em; color: #6b7280; padding: 5px 8px; border-bottom: 1px solid #e0e0e0; }
.mfold td { padding: 5px 8px; border-bottom: 1px solid #f0f0f0; }
.mfold td code { font-size: .82rem; }
.mfold .mfreq { color: #b42318; } .mfold .mfopt { color: #6b7280; }
.mfold .mfsrc { display: inline-block; border-radius: 8px; padding: 0 8px; font-size: .7rem; background: #eef2f6; }
.mfold details.mfowl { border: 1px solid #e0e0e0; border-radius: 6px; margin: 10px 0; background: #fff; }
.mfold details.mfowl summary { padding: 8px 14px; cursor: pointer; font-weight: 600; font-size: .88rem; }
.mfold details.mfowl pre { margin: 0; padding: 12px; border-top: 1px solid #e0e0e0; overflow-x: auto; font-family: ui-monospace, Menlo, monospace; font-size: .78rem; line-height: 1.5; }
.mfold .mfgrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 8px; margin: 8px 0; }
.mfold a.mfcard { display: block; background: #fff; border: 1px solid #e0e0e0; border-radius: 6px; padding: 8px 12px; text-decoration: none; color: inherit; }
.mfold a.mfcard:hover { border-color: #0d6efd; }
.mfold a.mfcard .mfn { display: block; font-size: .88rem; }
.mfold a.mfcard .mfm { display: block; font-size: .74rem; color: #6b7280; font-family: ui-monospace, Menlo, monospace; }
.mfold .mfzoom { font-size: .8rem; }
.mfold .mfzoom a { color: #0d6efd; text-decoration: none; }
details.mfwrap { border: 1px solid #e0e0e0; border-radius: 8px; background: #fff; margin-bottom: 14px; }
details.mfwrap > summary { padding: 10px 16px; font-weight: 600; cursor: pointer; font-size: .95rem; }
details.mfwrap > .mfbody { padding: 0 16px 14px; }
`;

  function injectCss() {
    if (document.getElementById('mfold-css')) return;
    const s = document.createElement('style');
    s.id = 'mfold-css';
    s.textContent = CSS;
    document.head.appendChild(s);
  }

  async function getJSON(path) {
    const r = await fetch(path, { headers: { Accept: 'application/json' } });
    if (!r.ok) throw new Error(path + ' → HTTP ' + r.status);
    return r.json();
  }

  let _disc = null;
  async function collectionFor(kind) {
    _disc ??= await getJSON('/owl/');
    const hit = (_disc.primitives || []).find(p => p.kind === kind);
    return hit ? hit.collection : null;
  }

  // ---- schema (table form of the model's claims about each class) ----
  async function schemaHtml(name) {
    let d;
    try {
      const r = await fetch('/api/athena/domain-schema/' + encodeURIComponent(name));
      // Three failures, three messages (#3724's hard-won lesson): route-missing,
      // store-unreachable, and genuinely-empty must never look alike.
      if (r.status === 404) {
        return '<div class="mfnote"><strong>Schema endpoint not on this server.</strong> ' +
          'The page is current (served from disk); the API behind it is an older build. ' +
          'Nothing is wrong with the model.</div>';
      }
      d = await r.json();
      if (!r.ok || d.storeReachable === false) throw new Error(d.error || 'the store did not answer');
    } catch (e) {
      return '<div class="mferr"><strong>Could not read the model — ' + ESC(e.message) + '.</strong> ' +
        'The endpoint answered but the store did not. This is NOT an empty schema.</div>';
    }
    if (!d.classes.length) {
      return '<div class="mfnote"><strong>No classes.</strong> This domain declares no vocabulary, ' +
        'so there is nothing to describe — a fact about the model, not a missing page.</div>';
    }
    return d.classes.map(c =>
      '<h3>' + ESC(c.name) + '</h3>' +
      (c.properties.length
        ? '<div class="mfscroll"><table><thead><tr><th>Property</th><th>Type</th>' +
          '<th>Required</th><th>Declared in</th></tr></thead><tbody>' +
          c.properties.map(p => {
            const both = p.source.length > 1;
            return '<tr><td><code>' + ESC(p.property) + '</code></td>' +
              '<td>' + (p.type ? ESC(p.type) : '—') + '</td>' +
              '<td class="' + (p.required ? 'mfreq' : 'mfopt') + '">' + (p.required ? 'required' : 'optional') + '</td>' +
              '<td><span class="mfsrc">' + ESC(both ? 'both' : (p.source[0] || '?')) + '</span></td></tr>';
          }).join('') + '</tbody></table></div>'
        : '<div class="mfnote"><strong>No properties.</strong> Nothing in the ontology types a property ' +
          'over this class and no shape constrains it.</div>')
    ).join('');
  }

  // ---- instances (per vocabulary class, from the generated collections) ----
  async function instancesHtml(name, vocab) {
    if (!vocab.length) {
      return '<div class="mfnote"><strong>This domain declares no vocabulary.</strong> ' +
        'With no <code>definesVocabulary</code> edge there is no class to list instances of — ' +
        'the missing edge, not missing data, is the thing to fix.</div>';
    }
    const parts = [];
    for (const kind of vocab) {
      let coll = null;
      try { coll = await collectionFor(kind); } catch (_) { /* discovery failure reported per-kind below */ }
      if (!coll) {
        parts.push('<h3>' + ESC(kind) + '</h3><div class="mfnote"><strong>Not served.</strong> ' +
          'Declared as vocabulary but no collection is generated — it has no shape.</div>');
        continue;
      }
      try {
        const r = await getJSON('/owl' + coll.replace(/^\/v1/, ''));
        const items = (r.data || []).slice().sort((a, b) => String(a.name).localeCompare(String(b.name)));
        parts.push('<h3>' + ESC(kind) + '</h3>' + (items.length
          ? '<div class="mfgrid">' + items.map(i =>
              '<a class="mfcard" href="/domains/' + encodeURIComponent(name) + '/' + encodeURIComponent(i.name) + '">' +
              '<span class="mfn">' + ESC(i.label || i.name) + '</span>' +
              '<span class="mfm">' + ESC(i.name) + '</span></a>').join('') + '</div>'
          : '<div class="mfnote"><strong>Served, but zero instances live.</strong> The collection ' +
            'answered — the model genuinely holds none of these. Different from unreachable, ' +
            'different from unserved.</div>'));
      } catch (e) {
        parts.push('<h3>' + ESC(kind) + '</h3><div class="mferr"><strong>' + ESC(e.message) +
          '</strong> — could not read this collection. Reported rather than rendered as empty.</div>');
      }
    }
    return parts.join('');
  }

  // ---- OWL (lazy — fetched only when opened) ----
  function wireOwl(root, name) {
    const box = root.querySelector('details.mfowl');
    if (!box) return;
    let loaded = false;
    box.addEventListener('toggle', async () => {
      if (!box.open || loaded) return;
      loaded = true;
      const pre = box.querySelector('pre');
      try {
        const r = await fetch('/api/athena/domain-owl/' + encodeURIComponent(name));
        const body = await r.text();
        if (r.status === 404 || body.startsWith('{')) {
          pre.textContent = '# This endpoint is not on this server.\n' +
            '# The API behind this page is an older build; the response below is NOT this\n' +
            '# domain\'s OWL:\n#\n' + body.slice(0, 300);
          return;
        }
        pre.textContent = r.ok
          ? (body.trim() || '# no triples — this domain declares no vocabulary. Not an error, not hidden.')
          : body;
      } catch (e) {
        loaded = false;
        pre.textContent = '# could not reach the model: ' + e.message + '\n# This is NOT an empty domain.';
      }
    });
  }

  /**
   * Render the model fold for `name` into `el`.
   * opts.compact — wrap in a collapsed <details> (the ops page's what-is-this
   *   anchor) with a zoom-out link to the model view. Full mode renders open
   *   (the model view IS the descent, folding it there would hide the point).
   * Resolves when schema + instances have rendered (OWL stays lazy).
   */
  async function renderModelFold(el, name, opts) {
    injectCss();
    const compact = !!(opts && opts.compact);
    let vocab = [];
    let vocabErr = null;
    try {
      const doms = await getJSON('/owl/domains');
      const row = (doms.data || []).find(r => r.name === name);
      if (!row) vocabErr = '<div class="mferr"><strong>No domain named "' + ESC(name) + '" in the model.</strong> ' +
        'That is the graph\'s answer, not a fetch failure.</div>';
      else vocab = [].concat(row.definesVocabulary || []).filter(Boolean);
    } catch (e) {
      vocabErr = '<div class="mferr"><strong>Cannot reach the model API — ' + ESC(e.message) + '.</strong> ' +
        'This is a fetch failure, not an empty model.</div>';
    }

    const inner =
      '<div class="mfold" data-mfold-domain="' + ESC(name) + '">' +
      (vocabErr || (
        '<h2>Schema — what the model says each class has</h2><div data-mf="schema">…</div>' +
        '<details class="mfowl"><summary>The OWL for this domain</summary><pre>…</pre></details>' +
        '<h2>Instances — what lives in this domain</h2><div data-mf="instances">…</div>')) +
      '<p class="mfzoom">' + (compact
        ? 'zoom out: <a href="/domains/' + encodeURIComponent(name) + '">model view — the descent (ERD &rarr; domain &rarr; class &rarr; instance)</a>'
        : 'across: <a href="/athena/domain.html?d=' + encodeURIComponent(name) + '">ops view — cards, code, tests, logs</a>') +
      '</p></div>';

    el.innerHTML = compact
      ? '<details class="mfwrap"><summary>Model — schema &middot; OWL &middot; instances ' +
        '<span style="font-weight:400;color:#6b7280;font-size:.78rem">(the shared fold, #3757 — identical on the model view)</span>' +
        '</summary><div class="mfbody">' + inner + '</div></details>'
      : inner;

    if (!vocabErr) {
      const [sch, inst] = await Promise.all([schemaHtml(name), instancesHtml(name, vocab)]);
      el.querySelector('[data-mf="schema"]').innerHTML = sch;
      el.querySelector('[data-mf="instances"]').innerHTML = inst;
      wireOwl(el, name);
    }
    el.dataset.mfoldReady = '1';
  }

  window.renderModelFold = renderModelFold;
})();
