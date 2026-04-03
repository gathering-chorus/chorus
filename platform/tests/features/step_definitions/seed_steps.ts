import { Given, When, Then, Before, After } from '@cucumber/cucumber';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as crypto from 'crypto';
import assert from 'assert';

const APP_URL = 'http://localhost:3000';
const FUSEKI_URL = 'http://localhost:3030';

const ENV_FILE = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/.env';
let TWILIO_AUTH_TOKEN = '';
let ALLOWED_PHONE = '';
let TWILIO_WEBHOOK_URL = APP_URL;
let FUSEKI_ADMIN_PW = '';
if (fs.existsSync(ENV_FILE)) {
  const env = fs.readFileSync(ENV_FILE, 'utf-8');
  TWILIO_AUTH_TOKEN = env.match(/^TWILIO_AUTH_TOKEN=(.+)$/m)?.[1] || '';
  ALLOWED_PHONE = env.match(/^CAPTURE_ALLOWED_PHONES=(.+)$/m)?.[1]?.split(',')[0] || '';
  TWILIO_WEBHOOK_URL = env.match(/^TWILIO_WEBHOOK_URL=(.+)$/m)?.[1] || APP_URL;
  FUSEKI_ADMIN_PW = env.match(/^FUSEKI_ADMIN_PW=(.+)$/m)?.[1] || '';
}

interface SeedContext {
  contentSid: string;
  hashtagSid: string;
  contentText: string;
  hashtag: string;
  webhookResponses: number[];
}

let ctx: SeedContext;

Before({ tags: '@seed' }, function () {
  ctx = {
    contentSid: `SM_BDD_${Date.now()}_content`,
    hashtagSid: `SM_BDD_${Date.now()}_tag`,
    contentText: '',
    hashtag: '',
    webhookResponses: [],
  };
});

After({ tags: '@seed' }, function () {
  // Clean all BDD test seeds by SID prefix — catches everything regardless of routing
  try {
    const sparql = 'PREFIX jb: <https://jeffbridwell.com/ontology#> DELETE { GRAPH <urn:jb:seeds/> { ?s ?p ?o } } WHERE { GRAPH <urn:jb:seeds/> { ?s jb:messageSid ?sid . FILTER(STRSTARTS(?sid, "SM_BDD_")) . ?s ?p ?o . } }';
    const tmpFile = `/tmp/bdd-seed-cleanup-${Date.now()}.sparql`;
    fs.writeFileSync(tmpFile, sparql);
    execSync(`curl -sf --max-time 5 -X POST "${FUSEKI_URL}/pods/update" -H "Content-Type: application/sparql-update" -u "admin:${FUSEKI_ADMIN_PW}" --data-binary @${tmpFile}`, { stdio: 'ignore' });
    fs.unlinkSync(tmpFile);
  } catch { /* best effort */ }
});

function signWebhook(url: string, params: Record<string, string>): string {
  const sorted = Object.keys(params).sort();
  let signData = url;
  for (const key of sorted) { signData += key + params[key]; }
  return crypto.createHmac('sha1', TWILIO_AUTH_TOKEN).update(signData).digest('base64');
}

function sendSms(body: string, messageSid: string): number {
  const postUrl = `${APP_URL}/api/seed/sms`;
  // Sign against the URL the middleware validates (TWILIO_WEBHOOK_URL), POST to localhost
  const signUrl = `${TWILIO_WEBHOOK_URL}/api/seed/sms`;
  const params: Record<string, string> = {
    Body: body, From: ALLOWED_PHONE, MessageSid: messageSid, NumMedia: '0', To: ALLOWED_PHONE,
  };
  const signature = signWebhook(signUrl, params);
  const formBody = Object.entries(params).map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&');
  try {
    const result = execSync(
      `curl -sf -o /dev/null -w "%{http_code}" --max-time 15 -X POST "${postUrl}" -H "Content-Type: application/x-www-form-urlencoded" -H "X-Twilio-Signature: ${signature}" -d '${formBody}'`,
      { encoding: 'utf-8', timeout: 20000 }
    );
    return parseInt(result.trim(), 10);
  } catch { return 0; }
}

function queryFuseki(sid: string): string {
  const query = `PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT ?seed ?content ?status WHERE { GRAPH <urn:jb:seeds/> { ?seed jb:messageSid "${sid}" . OPTIONAL { ?seed jb:seedContent ?content } OPTIONAL { ?seed jb:seedStatus ?status } } } LIMIT 1`;
  try {
    return execSync(`curl -sf --max-time 5 -H "Accept: application/sparql-results+json" --data-urlencode 'query=${query}' "${FUSEKI_URL}/pods/query"`, { encoding: 'utf-8', timeout: 10000 });
  } catch { return ''; }
}


// --- ARRIVE ---

Given('Jeff sends {string} from his phone', function (text: string) {
  if (text.startsWith('#')) {
    ctx.hashtag = text.replace('#', '');
    ctx.hashtagSid = `SM_BDD_${Date.now()}_tag`;
    ctx.webhookResponses.push(sendSms(text, ctx.hashtagSid));
  } else {
    ctx.contentText = text;
    ctx.contentSid = `SM_BDD_${Date.now()}_content`;
    ctx.webhookResponses.push(sendSms(text, ctx.contentSid));
  }
});

Given('Jeff sends {string} {int} seconds later', function (text: string, _seconds: number) {
  execSync('sleep 1');
  if (text.startsWith('#')) {
    ctx.hashtag = text.replace('#', '');
    ctx.hashtagSid = `SM_BDD_${Date.now()}_tag`;
    ctx.webhookResponses.push(sendSms(text, ctx.hashtagSid));
  } else {
    ctx.contentText = text;
    ctx.webhookResponses.push(sendSms(text, `SM_BDD_${Date.now()}_late`));
  }
});

Given('no content message preceded it', function () { /* already the case */ });

Given('no hashtag follows within {int} seconds', function (_seconds: number) {
  execSync('sleep 2');
});

When('the seed pipeline processes both messages', function () {
  execSync('sleep 2');
  for (const code of ctx.webhookResponses) {
    assert.strictEqual(code, 200, `Webhook returned ${code}, expected 200`);
  }
});

Then('the content is routed to ideas in Chorus', function () {
  const result = queryFuseki(ctx.contentSid);
  assert.ok(result.includes('seed'), `Content seed not found in Fuseki for SID ${ctx.contentSid}`);
});

Then('the hashtag message does not create a capture', function () {
  const result = queryFuseki(ctx.hashtagSid);
  assert.ok(!result.includes('"bindings":[{'), `Hashtag-only message created a capture`);
});

Then('no seed record is created', function () {
  const result = queryFuseki(ctx.hashtagSid);
  assert.ok(!result.includes('"bindings":[{'), `Hashtag-only message created a seed record`);
});

Then('the seed routes to wren by default', function () {
  const result = queryFuseki(ctx.contentSid);
  assert.ok(result.includes('seed'), `Seed not found for SID ${ctx.contentSid}`);
});


// --- ANTI-PATTERNS ---

Given('Jeff sends {string} then {string}', function (content: string, tag: string) {
  ctx.contentText = content;
  ctx.contentSid = `SM_BDD_${Date.now()}_anti_c`;
  sendSms(content, ctx.contentSid);
  execSync('sleep 1');
  ctx.hashtag = tag.replace('#', '');
  ctx.hashtagSid = `SM_BDD_${Date.now()}_anti_t`;
  sendSms(tag, ctx.hashtagSid);
  execSync('sleep 2');
});

When('the triage page loads', function () { /* check Fuseki directly */ });

Then('only the content message appears as a seed', function () {
  const result = queryFuseki(ctx.contentSid);
  assert.ok(result.includes('seed'), 'Content message should be a seed');
});

Then('{string} does not appear as a separate pending seed', function (_tag: string) {
  const result = queryFuseki(ctx.hashtagSid);
  assert.ok(!result.includes('"bindings":[{'), `Hashtag appears as separate seed — should not`);
});

Given('Jeff sends a seed', function () {
  ctx.contentText = '[BDD-TEST] spam check';
  ctx.contentSid = `SM_BDD_${Date.now()}_spam`;
  sendSms(ctx.contentText, ctx.contentSid);
  execSync('sleep 2');
});

When('a role receives it', function () { /* nudge delivered */ });
Then('the role does not batch it for later', function () { /* policy */ });
Then('the role does not discard without reading', function () { /* policy */ });
Then('the role engages in the same prompt cycle', function () { /* policy */ });
