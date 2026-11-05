// @domain: properties
// #4889 (Kade 07:06) — the one TypeScript reader of a global property, shared by
// every package the way chorus-sdk/time is. Same rules as the Rust reader
// (properties-resolver global_value): the row must exist, be global scope and carry
// a value; a door that does not answer is an error. There is no default: a caller
// decides what an error means for it, and says so out loud.
'use strict';

/** The door every caller reads properties from: OWL_URL, else athena-make on the Library. */
function doorUrl() {
  return process.env.OWL_URL || 'http://127.0.0.1:3360';
}

/** One global property from a properties-door body. Pure.
 *  Returns { ok: true, value } or { ok: false, error, where: 'row' } (the row is
 *  missing, empty or not global; a door error is where: 'door'). */
function globalValueFromBody(body, key) {
  const rows = body && Array.isArray(body.data) ? body.data : null;
  if (!rows) return { ok: false, where: 'door', error: `${key}: malformed properties body` };
  const row = rows.find((r) => r && r.propertyKey === key);
  if (!row) return { ok: false, where: 'row', error: `no property row sets ${key} (add it to the graph; there is no default)` };
  if (row.propertyScope !== 'global') return { ok: false, where: 'row', error: `${key}: the row is not global scope` };
  const v = typeof row.propertyValue === 'string' ? row.propertyValue.trim() : '';
  if (!v) return { ok: false, where: 'row', error: `${key}: the row has no value` };
  return { ok: true, value: v };
}

/** Read one global property through the door. Never throws. */
async function readGlobal(key, opts = {}) {
  const fetchFn = opts.fetchFn || fetch;
  const url = `${opts.door || doorUrl()}/properties/properties?limit=1000`;
  let res;
  try {
    res = await fetchFn(url, { signal: AbortSignal.timeout(opts.timeoutMs || 5000) });
  } catch (e) {
    return { ok: false, where: 'door', error: `${key}: the properties door did not answer (${e && e.message ? e.message : e})` };
  }
  if (!res.ok) return { ok: false, where: 'door', error: `${key}: the properties door answered ${res.status}` };
  let body;
  try {
    body = await res.json();
  } catch {
    return { ok: false, where: 'door', error: `${key}: malformed properties body` };
  }
  return globalValueFromBody(body, key);
}

module.exports = { doorUrl, globalValueFromBody, readGlobal };
