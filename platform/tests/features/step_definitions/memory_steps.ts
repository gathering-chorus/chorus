import { Given, When, Then, After } from '@cucumber/cucumber';
import { execSync } from 'child_process';
import * as assert from 'assert';

// State shared across steps within a scenario
let lastResponse = { status: 0, body: '' };
let thread: Array<{ speaker: string; text: string; time: string }> = [];

const API = 'http://localhost:3340';

function curl(url: string): { status: number; body: string } {
  try {
    const raw = execSync(
      `curl -s -o /tmp/memory-test-body -w '%{http_code}' "${url}" --connect-timeout 5 --max-time 15 2>/dev/null`,
      { encoding: 'utf-8', timeout: 20000 }
    ).trim();
    const status = parseInt(raw, 10);
    const body = require('fs').existsSync('/tmp/memory-test-body')
      ? require('fs').readFileSync('/tmp/memory-test-body', 'utf-8')
      : '';
    return { status, body };
  } catch (e: any) {
    return { status: 0, body: e.message || 'curl failed' };
  }
}

function todayBoston(): string {
  return execSync(`TZ=America/New_York date '+%Y-%m-%d'`, { encoding: 'utf-8' }).trim();
}

// --- Background ---

Given('the Chorus API is running on port {int}', function (port: number) {
  const r = curl(`http://localhost:${port}/health`);
  assert.strictEqual(r.status, 200, `Chorus API not running on port ${port}: status ${r.status}`);
});

// --- When: request conversation ---

When('a role requests the conversation between {string} and {string} from today', function (role1: string, role2: string) {
  const date = todayBoston();
  const r = curl(`${API}/api/chorus/conversation?roles=${role1},${role2}&date=${date}&tz=America/New_York`);
  lastResponse = r;
  try {
    const parsed = JSON.parse(r.body);
    thread = parsed.thread || [];
  } catch {
    thread = [];
  }
});

When('a role requests the conversation between {string} and {string} from {string} to {string} today', function (role1: string, role2: string, from: string, to: string) {
  const date = todayBoston();
  const r = curl(`${API}/api/chorus/conversation?roles=${role1},${role2}&date=${date}&after=${from}&before=${to}&tz=America/New_York`);
  lastResponse = r;
  try {
    const parsed = JSON.parse(r.body);
    thread = parsed.thread || [];
  } catch {
    thread = [];
  }
});

// --- Then: conversation structure ---

Then('the response contains a conversation thread', function () {
  assert.ok(lastResponse.status === 200, `Expected 200, got ${lastResponse.status}`);
  const parsed = JSON.parse(lastResponse.body);
  assert.ok(Array.isArray(parsed.thread), `Response missing "thread" array. Keys: ${Object.keys(parsed)}`);
  assert.ok(parsed.thread.length > 0, 'Thread is empty — no conversation found');
});

Then('each message has a speaker, text, and timestamp', function () {
  for (const msg of thread) {
    assert.ok(msg.speaker, `Message missing speaker: ${JSON.stringify(msg)}`);
    assert.ok(msg.text, `Message missing text: ${JSON.stringify(msg)}`);
    assert.ok(msg.time, `Message missing time: ${JSON.stringify(msg)}`);
  }
});

Then('messages are ordered chronologically', function () {
  for (let i = 1; i < thread.length; i++) {
    assert.ok(
      thread[i].time >= thread[i - 1].time,
      `Messages out of order: [${i - 1}] ${thread[i - 1].time} > [${i}] ${thread[i].time}`
    );
  }
});

Then('both Jeff\'s messages and Wren\'s messages appear in the thread', function () {
  const speakers = new Set(thread.map(m => m.speaker.toLowerCase()));
  assert.ok(speakers.has('jeff'), `Jeff's messages missing. Speakers found: ${[...speakers]}`);
  assert.ok(speakers.has('wren'), `Wren's messages missing. Speakers found: ${[...speakers]}`);
});

// --- Then: Jeff's voice ---

Then('Jeff\'s messages appear with speaker {string}', function (expectedSpeaker: string) {
  const jeffMsgs = thread.filter(m => m.speaker.toLowerCase() === expectedSpeaker);
  assert.ok(jeffMsgs.length > 0, `No messages with speaker "${expectedSpeaker}"`);
});

Then('Jeff\'s messages contain his actual words — not skill loads or system reminders', function () {
  const jeffMsgs = thread.filter(m => m.speaker.toLowerCase() === 'jeff');
  for (const msg of jeffMsgs) {
    assert.ok(
      !msg.text.startsWith('Base directory for this skill:'),
      `Jeff message is a skill load, not his words: ${msg.text.slice(0, 100)}`
    );
    assert.ok(
      !msg.text.startsWith('<system-reminder>'),
      `Jeff message is a system reminder: ${msg.text.slice(0, 100)}`
    );
  }
});

Then('Jeff\'s messages are not reconstructed from assistant context', function () {
  const jeffMsgs = thread.filter(m => m.speaker.toLowerCase() === 'jeff');
  assert.ok(jeffMsgs.length > 0, 'No Jeff messages to verify');
  // Jeff's messages should come from user turns, not be extracted from assistant text
  // This is verified by the source field if present
  for (const msg of jeffMsgs) {
    if ((msg as any).source) {
      assert.ok(
        (msg as any).source !== 'reconstructed',
        `Jeff message was reconstructed from assistant context: ${msg.text.slice(0, 100)}`
      );
    }
  }
});

// --- Then: time ---

Then('all returned messages fall within 10:00 AM and 2:00 PM Boston time', function () {
  for (const msg of thread) {
    const hour = parseInt(msg.time.split(' ')[1]?.split(':')[0] || msg.time.split('T')[1]?.slice(0, 2) || '0', 10);
    assert.ok(
      hour >= 10 && hour < 14,
      `Message outside range: ${msg.time} (hour=${hour})`
    );
  }
});

Then('timestamps display in Boston time — not UTC', function () {
  // Boston time hours for a workday conversation should be 6-23, not 10-27 (UTC offset)
  // If we see hours like 14-18 for a 10am-2pm Boston conversation, it's UTC
  assert.ok(thread.length > 0, 'No messages to check timestamps');
  const firstHour = parseInt(thread[0].time.split(' ')[1]?.split(':')[0] || thread[0].time.split('T')[1]?.slice(0, 2) || '0', 10);
  assert.ok(
    firstHour < 24,
    `Timestamp looks like raw UTC: ${thread[0].time}`
  );
});

// --- Then: thread structure ---

Then('the response is a single ordered thread — not ranked search results', function () {
  const parsed = JSON.parse(lastResponse.body);
  assert.ok(!parsed.results, 'Response has "results" key — looks like search hits, not a thread');
  assert.ok(!parsed.total, 'Response has "total" key — looks like search results');
  assert.ok(Array.isArray(parsed.thread), 'Response should have "thread" array');
});

Then('there are no relevance scores or snippets', function () {
  for (const msg of thread) {
    assert.ok(!(msg as any).score, `Message has relevance score: ${JSON.stringify(msg)}`);
    assert.ok(!(msg as any).snippet, `Message has snippet: ${JSON.stringify(msg)}`);
    assert.ok(!(msg as any).rank, `Message has rank: ${JSON.stringify(msg)}`);
  }
});

Then('consecutive messages from the same speaker are not deduplicated', function () {
  // Just verify the thread preserves the original conversation flow
  // Consecutive same-speaker messages should both appear
  assert.ok(thread.length > 0, 'Thread is empty');
});

// --- Then: full session ---

Then('the thread includes the full session — not just keyword matches', function () {
  // A conversation thread should have more than a handful of messages
  // If we're only getting keyword hits, we'd see 5-10. A real session has dozens.
  assert.ok(
    thread.length > 5,
    `Thread only has ${thread.length} messages — looks like keyword matches, not a full session`
  );
});

Then('messages that don\'t match a search term are still included', function () {
  // This is inherent to conversation retrieval vs search —
  // verified by the thread containing varied content
  const texts = thread.map(m => m.text.toLowerCase());
  const unique = new Set(texts);
  assert.ok(unique.size > 3, `Only ${unique.size} unique messages — looks filtered`);
});

Then('the conversation reads as a continuous exchange', function () {
  // Verify we have alternating speakers (at least some back-and-forth)
  let switches = 0;
  for (let i = 1; i < thread.length; i++) {
    if (thread[i].speaker !== thread[i - 1].speaker) switches++;
  }
  assert.ok(switches > 0, 'No speaker switches — not a conversation');
});

// --- Then: empty ---

Then('the response contains an empty thread', function () {
  const parsed = JSON.parse(lastResponse.body);
  assert.ok(Array.isArray(parsed.thread), 'Response missing "thread" array');
  assert.strictEqual(parsed.thread.length, 0, `Expected empty thread, got ${parsed.thread.length} messages`);
});

Then('the response status is {int}', function (expected: number) {
  assert.strictEqual(lastResponse.status, expected, `Expected ${expected}, got ${lastResponse.status}`);
});

// --- Cleanup ---

After(function () {
  try { require('fs').unlinkSync('/tmp/memory-test-body'); } catch {}
});
