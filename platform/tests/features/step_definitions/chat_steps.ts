import { Given, When, Then, After } from '@cucumber/cucumber';
import { execSync } from 'child_process';
import * as assert from 'assert';
import * as fs from 'fs';

const CHAT_SH = '/Users/jeffbridwell/CascadeProjects/platform/scripts/chat.sh';

let chatId = '';
let lastOutput = '';
let lastLineCount = 0;

function chat(args: string): string {
  try {
    return execSync(`bash ${CHAT_SH} ${args} 2>/dev/null`, {
      encoding: 'utf-8',
      timeout: 10000,
    }).trim();
  } catch (e: any) {
    return e.stdout?.trim() || e.message || 'chat.sh failed';
  }
}

// --- Background ---

Given('the chat script is available', function () {
  assert.ok(fs.existsSync(CHAT_SH), `chat.sh not found at ${CHAT_SH}`);
});

// --- Start ---

When('{word} starts a chat with {word} about {string}', function (roleA: string, roleB: string, topic: string) {
  const output = chat(`start ${roleA} ${roleB} "${topic}"`);
  const lines = output.split('\n');
  chatId = lines[lines.length - 1].trim();
  lastOutput = output;
});

Given('{word} started a chat with {word} about {string}', function (roleA: string, roleB: string, topic: string) {
  const output = chat(`start ${roleA} ${roleB} "${topic}"`);
  const lines = output.split('\n');
  chatId = lines[lines.length - 1].trim();
});

Then('a CHAT_ID is returned', function () {
  assert.ok(chatId.length > 0, `No CHAT_ID returned. Output: ${lastOutput}`);
});

Then('the transcript file exists', function () {
  const transcriptPath = `/tmp/chorus-chat/${chatId}.md`;
  assert.ok(fs.existsSync(transcriptPath), `Transcript not found at ${transcriptPath}`);
});

// --- Messages ---

When('{word} says {string}', function (role: string, message: string) {
  const output = chat(`say ${chatId} ${role} "${message}"`);
  const num = parseInt(output.match(/\d+/)?.[0] || '0', 10);
  if (num > 0) lastLineCount = num;
  lastOutput = output;
});

Then('the message appears in the transcript with speaker {string}', function (speaker: string) {
  const transcript = chat(`read ${chatId}`);
  assert.ok(
    transcript.includes(`${speaker}:`),
    `Speaker "${speaker}" not found in transcript:\n${transcript}`
  );
});

Then('the transcript has {int} messages', function (count: number) {
  const transcript = chat(`read ${chatId}`);
  const messageLines = transcript.split('\n').filter(l => l.match(/^\*\*\[\d/));
  assert.strictEqual(
    messageLines.length,
    count,
    `Expected ${count} messages, found ${messageLines.length}:\n${transcript}`
  );
});

// --- Read since ---

When('{word} reads since line {int}', function (_role: string, line: number) {
  lastOutput = chat(`read ${chatId} --since ${line}`);
});

Then('only the new message is returned', function () {
  const lines = lastOutput.split('\n').filter(l => l.match(/^\*\*\[/));
  assert.ok(lines.length >= 1, `Expected at least 1 new message, got:\n${lastOutput}`);
});

// --- Jeff reads ---

When('jeff reads the chat', function () {
  lastOutput = chat(`read ${chatId}`);
});

Then('the transcript shows both messages with timestamps and speakers', function () {
  const lines = lastOutput.split('\n').filter(l => l.match(/^\*\*\[/));
  assert.ok(lines.length >= 2, `Expected 2+ messages with timestamps:\n${lastOutput}`);
  assert.ok(lastOutput.includes('**['), `Missing timestamp format:\n${lastOutput}`);
});

// --- End ---

When('{word} ends the chat', function (_role: string) {
  lastOutput = chat(`end ${chatId}`);
});

Then('the chat is marked ended', function () {
  assert.ok(
    lastOutput.includes('ended') || lastOutput.includes('Chat'),
    `Chat not ended. Output: ${lastOutput}`
  );
});

Then('the transcript is saved to \\/tmp\\/chorus-chat\\/', function () {
  const files = fs.readdirSync('/tmp/chorus-chat/').filter(f => f.includes(chatId));
  assert.ok(files.length > 0, `No transcript file for ${chatId} in /tmp/chorus-chat/`);
});

// --- Cleanup ---

After(function () {
  if (chatId) {
    try { chat(`end ${chatId}`); } catch {}
    chatId = '';
  }
});
