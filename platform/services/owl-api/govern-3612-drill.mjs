// #3612 GOVERN live drill — refusal-only, zero mutation by construction.
// Proves the uniform write gate against a RUNNING variant seam with REAL CSS
// tokens (the #3613 drill pattern): every probe targets a refusal path, so
// nothing can land in the store; every refusal must be TYPED and leave an
// owl.write spine event (the #3612 one-exit contract).
//
//   node govern-3612-drill.mjs
//
// Env:
//   OWL_URL   variant seam under test   default http://localhost:3362
//   AGENT     whose cred to mint with   default wren  (~/.chorus/identity/$AGENT/cred.json)
//   CHORUS_LOG_FILE  spine log to check default $CHORUS_HOME/platform/logs/chorus.log
//
// Never prints token or secret values — statuses and claims only.
import { readFileSync, existsSync } from 'fs';
import { homedir } from 'os';

const OWL = process.env.OWL_URL || 'http://localhost:3362';
const AGENT = process.env.AGENT || 'wren';

function cred() {
  const f = `${homedir()}/.chorus/identity/${AGENT}/cred.json`;
  if (!existsSync(f)) throw new Error(`no cred at ${f} — seed-css.sh first`);
  const c = JSON.parse(readFileSync(f, 'utf8'));
  return { id: c.id ?? c.client_id, secret: c.secret ?? c.client_secret, tokenEndpoint: c.tokenEndpoint, issuer: c.issuer };
}

async function mint() {
  const c = cred();
  const auth = Buffer.from(`${c.id}:${c.secret}`).toString('base64');
  const r = await fetch(c.tokenEndpoint || `${c.issuer}/.oidc/token`, {
    method: 'POST',
    headers: { Authorization: `Basic ${auth}`, 'Content-Type': 'application/x-www-form-urlencoded' },
    body: 'grant_type=client_credentials&scope=webid',
  });
  if (!r.ok) throw new Error(`token mint failed: ${r.status}`);
  return (await r.json()).access_token;
}

async function hit(method, path, token, body, headers = {}) {
  const h = { ...headers };
  if (token) h.Authorization = `Bearer ${token}`;
  const r = await fetch(`${OWL}${path}`, { method, headers: h, body });
  let text = '';
  try { text = await r.text(); } catch {}
  return { status: r.status, body: text.slice(0, 300) };
}

let failed = 0;
const check = (name, cond, detail) => {
  console.log(`  ${cond ? 'PASS' : 'FAIL'}  ${name}${detail ? '  — ' + detail : ''}`);
  if (!cond) { failed++; process.exitCode = 1; }
};

const startedAt = new Date().toISOString();

// ---- the refusal matrix: one probe per gate class, every one a refusal ----

// 1. authn — no token on an entity write → typed 401 (+ spine)
let r = await hit('POST', '/domains', null, '{"name":"govern-drill-never-lands"}');
check('entity write, no token → 401 authn-missing', r.status === 401 && r.body.includes('authn-missing'), `${r.status} ${r.body}`);

// 2. authn — no token on batch → typed 401 (+ spine)
r = await hit('POST', '/batch', null, 'INS\t<urn:x>\t<urn:y>\t"z"', { 'x-target-graph': 'urn:chorus:domains:tests' });
check('batch, no token → 401 authn-missing', r.status === 401 && r.body.includes('authn-missing'), `${r.status} ${r.body}`);

const token = await mint();
console.log(`  minted ES256 token for ${AGENT} (value not shown)`);

// 3. out-of-scope — batch with a VALID but unscoped CSS token → typed 403 (+ spine).
//    fail-closed-by-omission: batch REQUIRES scope; CSS client_credentials tokens
//    carry none, so this is a real refusal with a real token.
r = await hit('POST', '/batch', token, 'INS\t<urn:x>\t<urn:y>\t"z"', { 'x-target-graph': 'urn:chorus:domains:tests' });
check('batch, valid unscoped token → 403 out-of-scope', r.status === 403 && r.body.includes('out-of-scope'), `${r.status} ${r.body}`);

// 4. closed-shape — create with an off-model property → typed 422 (+ spine off-model).
//    The off-model key guarantees refusal BEFORE the DAL: nothing can land.
r = await hit('POST', '/domains', token, '{"name":"govern-drill-never-lands","offModelKey3612":"x"}');
check('create, off-model property → 422 validation', r.status === 422 && r.body.includes('validation'), `${r.status} ${r.body}`);

// 5. validation — batch malformed line with valid-shaped graph header refused
//    at handle_batch (typed 422 + spine) IF the token were scoped; with the
//    unscoped token the gate refuses first (403) — either way: typed, never 200.
r = await hit('POST', '/domains/govern-drill-absent/partof', token, '{"target":"also-absent"}');
check('edge write on absent/unowned entity → typed refusal (403 authz fail-closed)', r.status === 403 && (r.body.includes('authz') || r.body.includes('owning role')), `${r.status} ${r.body}`);

// 6. injection-shaped entity name → typed 422, refused before any graph work
r = await hit('PUT', '/domains/bad%20name%3E', token, '{"label":"x"}');
check('replace, injection-shaped name → 422 validation', r.status === 422 && r.body.includes('validation'), `${r.status} ${r.body}`);

// 7. /bulk (#3612 incr-2) — same governed primitive, same gate: no token → 401,
//    valid-but-unscoped token → 403. Refusal-only, nothing can land.
r = await hit('POST', '/bulk', null, 'INS\t<urn:x>\t<urn:y>\t"z"', { 'x-target-graph': 'urn:chorus:domains:tests' });
check('bulk, no token → 401 authn-missing', r.status === 401 && r.body.includes('authn-missing'), `${r.status} ${r.body}`);
r = await hit('POST', '/bulk', token, 'INS\t<urn:x>\t<urn:y>\t"z"', { 'x-target-graph': 'urn:chorus:domains:tests' });
check('bulk, valid unscoped token → 403 out-of-scope', r.status === 403 && r.body.includes('out-of-scope'), `${r.status} ${r.body}`);

// ---- spine verification: the refusals above must be OBSERVABLE ----
// owl.write events flow chorus-log → shim → chorus-api spine, so ask Loki (the
// spine's query surface) for the last 5 minutes of owl.write lines and assert
// one event per refusal class. (chorus.log on disk is a different stream —
// grepping it was this drill's own first false-negative.)
const LOKI = process.env.LOKI_URL || 'http://localhost:3102';
await new Promise((res) => setTimeout(res, 1500)); // emission is best-effort async
const q = new URLSearchParams({
  query: '{appName="chorus-events"} |= `owl.write`',
  start: `${(Date.now() - 5 * 60 * 1000) * 1e6}`,
  limit: '60',
});
const lr = await fetch(`${LOKI}/loki/api/v1/query_range?${q}`);
if (lr.ok) {
  const ld = await lr.json();
  const lines = (ld.data?.result ?? []).flatMap((s) => s.values.map(([, l]) => l))
    .filter((l) => l.includes('"event":"owl.write"'));
  const has = (frag, opFrag) => lines.some((l) => l.includes(frag) && (!opFrag || l.includes(opFrag)));
  check('spine: authn refusal recorded (anon)', has('"result":"authn-missing"'), 'owl.write result=authn-missing');
  check('spine: out-of-scope refusal recorded', has('"result":"out-of-scope"', '"op":"batch"'), 'owl.write op=batch result=out-of-scope');
  check('spine: off-model refusal recorded', has('"result":"off-model"', '"op":"create"'), 'owl.write op=create result=off-model');
  check('spine: authz refusal recorded (per-primitive label)', has('"result":"authz"', '"op":"add-edge"'), 'owl.write op=add-edge result=authz');
  check('spine: invalid-name refusal recorded', has('"result":"validation"', '"op":"replace"'), 'owl.write op=replace result=validation');
  check('spine: bulk refusal recorded with its own op label', has('"result":"out-of-scope"', '"op":"bulk"'), 'owl.write op=bulk result=out-of-scope');
} else {
  check('spine queryable via Loki', false, `${LOKI} → ${lr.status}`);
}

console.log(failed === 0
  ? `\nGOVERN drill: all refusals typed + spined (started ${startedAt}) — nothing written, by construction.`
  : `\nGOVERN drill: ${failed} FAILED`);
