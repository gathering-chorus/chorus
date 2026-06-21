// #3522 — value-stream view, GRAPH-DERIVED from the model (retires the v1 bespoke
// /steps + hardcoded stream-map renderer). Fetches the v2 surfaces: /valuestreams (the
// streams) + /valuestreamsteps (steps with inStream + stageOrder, surfaced by the
// shape-driven projection). Groups steps by stream, orders by stageOrder — no hardcoded
// membership. Same-origin /api/athena (owl-api, proxied) for congruence with domain.html.
const ATHENA = '/api/athena';

async function load() {
  const c = document.getElementById('steps-container');
  try {
    const [vsRes, stepRes] = await Promise.all([
      fetch(`${ATHENA}/valuestreams`).then(r => r.json()),
      fetch(`${ATHENA}/valuestreamsteps`).then(r => r.json()),
    ]);
    const streams = (vsRes.data || []).filter(s => s.label);   // labeled streams only
    const allSteps = (stepRes.data || []);

    // graph-derived grouping: by inStream, ordered by stageOrder (no hardcoded map)
    const byStream = {};
    for (const s of allSteps) {
      if (!s.inStream) continue;
      (byStream[s.inStream] = byStream[s.inStream] || []).push(s);
    }
    for (const k in byStream) {
      byStream[k].sort((a, b) => (parseInt(a.stageOrder) || 0) - (parseInt(b.stageOrder) || 0));
    }

    let html = '';
    for (const stream of streams) {
      const steps = byStream[stream.name] || [];
      if (!steps.length) continue;
      const cells = steps.map(s =>
        `<div class="vs-step"><span class="vs-order">${s.stageOrder || ''}</span>` +
        `<span class="vs-label">${s.label || s.name}</span></div>`
      ).join('<span class="vs-arrow">&rarr;</span>');
      html += `<section class="vs-stream">` +
        `<h2>${stream.label} <span class="muted">(${steps.length} steps)</span></h2>` +
        `<div class="vs-row">${cells}</div></section>`;
    }
    c.innerHTML = html || '<p class="muted" style="padding:24px;">No value-stream steps in the model yet.</p>';
  } catch (e) {
    c.innerHTML = `<p class="muted" style="padding:24px;">Failed to load from Athena: ${e}</p>`;
  }
}
load();
