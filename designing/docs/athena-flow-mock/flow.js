// #3378 — shared data layer for the flow mock.
// Domain data is LIVE from the generated Domain API (:3360, CORS via #3373).
// Product/Service/ValueStream data is STATIC EXEMPLAR — it becomes the spec
// for #3351's generated APIs; every exemplar field maps to a shape property.
const OWL_API = 'http://localhost:3360';

// The value stream + product placement (exemplar — #3351 mints these as
// ValueStream/ValueStreamStep instances; placement via atStep).
const STREAM = [
  { step: 'Shaping',   key: 'shaping',   products: [{ name: 'loom', label: 'Loom' }] },
  { step: 'Designing', key: 'designing', products: [{ name: 'athena', label: 'Athena' }] },
  { step: 'Directing', key: 'directing', products: [{ name: 'clearing', label: 'Clearing',
      children: [{ name: 'pulse', label: 'pulse / working-memory' }, { name: 'spine', label: 'spine' }] }] },
  { step: 'Building',  key: 'building',  products: [{ name: 'werk', label: 'Werk' }, { name: 'convergence', label: 'Convergence' }],
    // RECURSION (Jeff 2026-06-12): a step ENCAPSULATES a child value stream —
    // hasValueStream(Building) = werk's stream; its steps ARE the verb sequence.
    substream: { of: 'werk', steps: ['pull', 'commit', 'push', 'build', 'test', 'deploy', 'demo', 'merge', 'accept'] } },
  { step: 'Proving',   key: 'proving',   products: [{ name: 'borg', label: 'Borg' }] },
];

async function fetchDomains() {
  try {
    const r = await fetch(OWL_API + '/domains');
    if (!r.ok) return null;
    return (await r.json()).items || null;
  } catch (e) { return null; }
}

async function fetchDomain(name) {
  try {
    const r = await fetch(OWL_API + '/domains/' + encodeURIComponent(name));
    if (!r.ok) return null;
    return await r.json();
  } catch (e) { return null; }
}

async function fetchContains(name) {
  try {
    const r = await fetch(OWL_API + '/domains/' + encodeURIComponent(name) + '/contains');
    if (!r.ok) return null;
    return await r.json();
  } catch (e) { return null; }
}

function stepBadge(label) {
  const k = esc((label || '').toLowerCase());
  return '<span class="badge ' + k + '">' + esc(label) + '</span>';
}
function ownerDot(label) {
  const k = esc((label || '').toLowerCase());
  return '<span class="owner-dot ' + k + '"></span>' + esc(label || '?');
}
function esc(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
// Honest fold: renders ALWAYS — empty states show, never hide (Jeff's rule).
function fold(title, src, bodyHtml, emptyMsg) {
  return '<details class="fold" open><summary>' + title +
    '<span class="src">' + src + '</span></summary><div class="body">' +
    (bodyHtml || '<p class="empty">' + emptyMsg + '</p>') + '</div></details>';
}
