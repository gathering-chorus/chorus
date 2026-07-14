// #3635 — shared runtime for the Athena page family (products / value-stream /
// domains / product / domain). Every page fetches owl-api (:3360) LIVE on load;
// an unreachable API renders an honest failure state, never an empty-but-confident
// page (#3627 contract). Multi-valued fields arrive as string-or-array (ADR-047
// additive) — asArray() normalizes.
// #3644 — same-origin via the chorus-api /owl proxy: one base that works on the
// LAN and through any share/tunnel origin (the old `hostname:3360` broke off-LAN).
const OWL = '/owl';
const STEPS_RULED = ['shaping', 'directing', 'designing', 'building', 'proving', 'reflecting'];

async function fetchJSON(path) {
  const r = await fetch(OWL + path);
  if (!r.ok) throw new Error(`${r.status} on ${path}`);
  return r.json();
}

function fetchFailed(el, e) {
  el.innerHTML = `<div class="err"><strong>Cannot reach the model API</strong> (${OWL}) — ` +
    `this is a fetch failure, not an empty model. ${esc(String(e.message || e))}</div>`;
}

function asArray(v) {
  if (Array.isArray(v)) return v;
  if (v === undefined || v === null || v === '') return [];
  return [String(v)];
}

function esc(s) {
  return String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}

function stepLocal(v) {
  return String(v || '').split('#').pop().replace('value-stream-step-', '').toLowerCase();
}

function ownerLocal(v) {
  return String(v || '').split('#').pop().replace('role-', '');
}

function ownerDot(owner) {
  const o = ownerLocal(owner);
  return o ? `<span class="owner-dot ${esc(o)}"></span>${esc(o)}` : '<span class="empty">no owner</span>';
}

function stepBadge(step) {
  const s = stepLocal(step);
  const cls = STEPS_RULED.includes(s) ? s : '';
  return s ? `<span class="badge ${cls}">${esc(s[0].toUpperCase() + s.slice(1))}</span>` : '<span class="empty">no step</span>';
}

function domainChips(hasDomain) {
  const doms = asArray(hasDomain);
  if (!doms.length) return '<span class="empty">no domains declared</span>';
  return doms.map(d => `<a class="chip" href="domain.html?d=${encodeURIComponent(d)}">${esc(d)}</a>`).join('');
}

function srcNote(extra) {
  return `<p class="src-note">live from owl-api ${esc(OWL)} · every field maps 1:1 to a shape property · nothing fabricated${extra ? ' · ' + extra : ''}</p>`;
}
