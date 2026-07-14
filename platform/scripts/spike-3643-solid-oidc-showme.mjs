// #3643 SHOW-ME: prove the SOLID-OIDC chain end-to-end against the LIVE, LOCKED store.
//   1. a headless service gets an ES256/WebID token from CSS (no browser)
//   2. verify it with CSS's PUBLIC key (JWKS) — no shared secret
//   3. gate a REAL write to the locked Fuseki store on that verification
//   4. a FORGED token is refused → no write
// Run:  node platform/scripts/spike-3643-solid-oidc-showme.mjs
import { readFileSync } from 'fs';
const APP = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site';
const env = Object.fromEntries(readFileSync(`${APP}/.env`, 'utf8').split('\n')
  .filter(l => l.includes('=') && !l.startsWith('#'))
  .map(l => { const i = l.indexOf('='); return [l.slice(0, i), l.slice(i + 1)]; }));
const { createRemoteJWKSet, jwtVerify } = await import(`file://${APP}/node_modules/jose/dist/node/cjs/index.js`);

const ISS = 'http://localhost:3001';
const FUSEKI = 'http://localhost:3030/pods';
const GRAPH = 'urn:silas:showme-3643';
const JWKS = createRemoteJWKSet(new URL(`${ISS}/.oidc/jwks`));

async function getCssToken() {
  const auth = Buffer.from(`${env.CSS_CC_ID}:${env.CSS_CC_SECRET}`).toString('base64');
  const r = await fetch(`${ISS}/.oidc/token`, { method: 'POST',
    headers: { Authorization: `Basic ${auth}`, 'Content-Type': 'application/x-www-form-urlencoded' },
    body: 'grant_type=client_credentials&scope=webid' });
  return (await r.json()).access_token;
}
// The GATE — this is exactly what owl-api would do at the door (#3613):
async function gatedWrite(token, subjectTag) {
  let webid;
  try { ({ payload: { webid } } = await jwtVerify(token, JWKS)); }   // PUBLIC-KEY verify
  catch (e) { return { wrote: false, reason: `token REJECTED (${e.code || e.message})` }; }
  const fauth = Buffer.from(`admin:${env.FUSEKI_ADMIN_PASSWORD}`).toString('base64');   // store cred behind the door
  const body = `INSERT DATA { GRAPH <${GRAPH}> { <urn:silas:${subjectTag}> <urn:wroteBy> <${webid}> } }`;
  const r = await fetch(`${FUSEKI}/update`, { method: 'POST',
    headers: { Authorization: `Basic ${fauth}`, 'Content-Type': 'application/sparql-update' }, body });
  return { wrote: r.ok, webid, code: r.status };
}
async function ask(sub) {
  const fauth = Buffer.from(`admin:${env.FUSEKI_ADMIN_PASSWORD}`).toString('base64');
  const r = await fetch(`${FUSEKI}/query`, { method: 'POST',
    headers: { Authorization: `Basic ${fauth}`, 'Content-Type': 'application/sparql-query' },
    body: `ASK { GRAPH <${GRAPH}> { <urn:silas:${sub}> ?p ?o } }` });
  return (await r.json()).boolean;
}

const tok = await getCssToken();
console.log('1. CSS issued a token to a headless service (no browser).');
const good = await gatedWrite(tok, 'good');
console.log(`2. VALID token  -> ${good.wrote ? 'WROTE ✓  authorized as ' + good.webid : 'refused'}`);
const forged = tok.slice(0, -8) + 'AAAAAAAA';
const bad = await gatedWrite(forged, 'forged');
console.log(`3. FORGED token -> ${bad.wrote ? 'WROTE — SECURITY HOLE' : 'REFUSED ✓  (' + bad.reason + ')'}`);
console.log(`4. store truth:  good-write present? ${await ask('good')}   forged-write present? ${await ask('forged')}`);
// cleanup
const fauth = Buffer.from(`admin:${env.FUSEKI_ADMIN_PASSWORD}`).toString('base64');
await fetch(`${FUSEKI}/update`, { method: 'POST', headers: { Authorization: `Basic ${fauth}`, 'Content-Type': 'application/sparql-update' }, body: `DROP GRAPH <${GRAPH}>` });
console.log('5. cleaned up.');
