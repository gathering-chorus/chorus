// @test-type: unit — fixture helper, not a suite
/**
 * #4417 — a stand-in for the security graph the Clearing reads through
 * CHORUS_FUSEKI_QUERY: who is allowed (webids), who is who (principal ↔ webid)
 * and who is a person (principalKind "person"). A test Clearing with no graph
 * at all refuses machine posts (it cannot tell who is a person), so every test
 * world that posts through /api/message brings one of these.
 */
const http = require('http');

const P = 'https://jeffbridwell.com/chorus#';

function startSecurityGraphStub({ persons = ['jeff'], principals = {} } = {}) {
  const server = http.createServer((req, res) => {
    const q = decodeURIComponent((req.url || '').split('query=')[1] || '');
    res.setHeader('Content-Type', 'application/sparql-results+json');
    let bindings;
    if (q.includes('principalKind')) bindings = persons.map((n) => ({ p: { value: `${P}principal-${n}` } }));
    else if (q.includes('?p ?webid')) bindings = Object.entries(principals).map(([webid, id]) => ({ p: { value: `${P}${id}` }, webid: { value: webid } }));
    else if (q.includes('?webid')) bindings = Object.keys(principals).map((webid) => ({ webid: { value: webid } }));
    else bindings = [];
    res.end(JSON.stringify({ results: { bindings } }));
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => {
    resolve({ url: `http://127.0.0.1:${server.address().port}/query`, close: () => new Promise((r) => server.close(r)) });
  }));
}

module.exports = { startSecurityGraphStub };

/**
 * Same stub in its own process, for callers whose event loop blocks while the
 * Clearing calls back (cucumber's steps shell out to curl with execSync, so an
 * in-process stub could never answer). Resolves with { url, close }.
 */
function spawnSecurityGraphStub(config = {}) {
  const { spawn } = require('child_process');
  const child = spawn(process.execPath, [__filename, JSON.stringify(config)], { stdio: ['ignore', 'pipe', 'inherit'] });
  return new Promise((resolve, reject) => {
    child.stdout.once('data', (d) => resolve({ url: String(d).trim(), close: async () => { child.kill('SIGTERM'); } }));
    child.once('exit', (code) => reject(new Error(`security-graph stub exited ${code}`)));
  });
}

module.exports.spawnSecurityGraphStub = spawnSecurityGraphStub;

if (require.main === module) {
  startSecurityGraphStub(JSON.parse(process.argv[2] || '{}')).then((g) => process.stdout.write(`${g.url}\n`));
}
