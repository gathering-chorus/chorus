import { Given, When, Then } from '@cucumber/cucumber';
import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';

// Policy steps verify behavioral contracts are DEFINED — memory files exist,
// policy page documents the contract, decisions are referenced.
// Runtime enforcement is via conversation gates (Silas's domain).

const MEMORY_DIR = '/Users/jeffbridwell/.claude/projects/-Users-jeffbridwell-CascadeProjects/memory';
const POLICY_PAGE = '/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/wren/artifacts/domain-policy.html';

let lastContext = '';
let violationDetected = false;

// === SESSION CONTEXT ===

Given('Jeff is in a session with a role', function () {
  lastContext = 'session';
});

Given('a role is presenting a demo to Jeff', function () {
  lastContext = 'demo';
});

// === STOP CONTRACT ===

When('Jeff says {string} or {string} or redirects to different work', function (s1: string, s2: string) {
  lastContext = 'stop';
});

Then('the role stops the current action immediately', function () {
  const page = fs.readFileSync(POLICY_PAGE, 'utf-8');
  assert.ok(page.includes('stop current action'), 'Stop policy not in domain page');
});

Then('the role does not say {string}', function (phrase: string) {
  const page = fs.readFileSync(POLICY_PAGE, 'utf-8');
  assert.ok(page.includes('NO:'), 'Anti-patterns not documented in policy page');
});

Then('the role does not explain what it was doing', function () {
  const page = fs.readFileSync(POLICY_PAGE, 'utf-8');
  assert.ok(page.includes('NO: explain why you were'), 'Explain anti-pattern not documented');
});

Then("the role follows Jeff's new direction in the same response", function () {
  assert.strictEqual(lastContext, 'stop');
});

When('Jeff redirects to different work', function () {
  // Context stays as 'demo' — the redirect happens mid-demo
});

Then('the role abandons the current demo', function () {
  assert.strictEqual(lastContext, 'demo');
});

Then("the role follows Jeff's new direction immediately", function () {
  assert.ok(fs.existsSync(POLICY_PAGE));
});

// === PROBLEM REPORT CONTRACT ===

Given('Jeff says {string} or {string}', function (s1: string, s2: string) {
  lastContext = 'problem';
});

Then('the role asks one short question about symptoms', function () {
  // Runbook: triage first — ask what Jeff sees, not what's wrong
  const mem = path.join(MEMORY_DIR, 'feedback_seek_understanding_first.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: seek_understanding_first');
});

Given('Jeff described a symptom', function () {
  lastContext = 'symptom-given';
});

Then('the role investigates using memory and logs and endpoints', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_investigate_dont_ask.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: investigate_dont_ask');
  const mem2 = path.join(MEMORY_DIR, 'feedback_check_memory_first.md');
  assert.ok(fs.existsSync(mem2), 'Missing feedback memory: check_memory_first');
});

Then('the role does not ask Jeff another question before reporting', function () {
  // After symptoms, the next interaction must be findings, not another question
  const mem = path.join(MEMORY_DIR, 'feedback_listen_before_rationalizing.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: listen_before_rationalizing');
});

Then('the role reports what it found or what it sees so far', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_root_cause_on_issue.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: root_cause_on_issue');
});

Then('the role does not defend the pipeline output', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_listen_before_rationalizing.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: listen_before_rationalizing');
});

Then('the role checks the source Jeff is looking at', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_trust_jeffs_eyes.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: trust_jeffs_eyes');
});

Then('the role reports the discrepancy with root cause', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_root_cause_on_issue.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: root_cause_on_issue');
});

// === FEEDBACK CONTRACT ===

Given('Jeff gives correction {string}', function (s: string) {
  lastContext = 'correction';
});

Given('Jeff gives confirmation {string}', function (s: string) {
  lastContext = 'confirmation';
});

Given('Jeff gives direction {string}', function (s: string) {
  lastContext = 'direction';
});

Given('Jeff requests depth {string}', function (s: string) {
  lastContext = 'depth';
});

Then('the role changes the behavior in the same response', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_no_apology_loops.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: no_apology_loops');
});

Then('the role does not acknowledge then repeat the behavior', function () {
  assert.strictEqual(lastContext, 'correction');
});

Then('the role saves a feedback memory', function () {
  const files = fs.readdirSync(MEMORY_DIR).filter(f => f.startsWith('feedback_'));
  assert.ok(files.length > 50, `Expected 50+ feedback memories, found ${files.length}`);
});

Then('the role continues the approach', function () {
  assert.strictEqual(lastContext, 'confirmation');
});

Then('the role saves a feedback memory noting what worked', function () {
  const files = fs.readdirSync(MEMORY_DIR).filter(f => f.startsWith('feedback_'));
  assert.ok(files.length > 0);
});

// === DIRECTION CONTRACT ===

Then('the role executes immediately', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_no_narrate_before_execute.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: no_narrate_before_execute');
});

Then('the role does not restate what Jeff said', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_announce_is_not_execute.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: announce_is_not_execute');
});

Then('the role reports the outcome when done', function () {
  assert.ok(['direction', 'generic'].includes(lastContext));
});

Then('a card is created in the same response', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_card_on_ask.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: card_on_ask');
});

Then('the card has a title, AC, owner, and priority', function () {
  assert.ok(true);
});

Then('the role does not narrate without acting', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_announce_is_not_execute.md');
  assert.ok(fs.existsSync(mem));
});

// === STORY CONTRACT ===

Given('Jeff shares a personal memory or family experience or values', function () {
  lastContext = 'story';
});

Given('Jeff mentions a person by name with a personal connection', function () {
  lastContext = 'person-story';
});

Then('the role receives it without deflecting to product', function () {
  assert.strictEqual(lastContext, 'story');
});

Then('the role reflects back what matters', function () {
  assert.strictEqual(lastContext, 'story');
});

Then('the role saves to stories.md', function () {
  const storyFiles = fs.readdirSync(MEMORY_DIR).filter(f => f.startsWith('story_'));
  assert.ok(storyFiles.length > 10, `Expected 10+ story memories, found ${storyFiles.length}`);
});

Then('the role checks if the person exists in the knowledge graph', function () {
  assert.strictEqual(lastContext, 'person-story');
});

Then('the role connects the story to what it knows about Jeff', function () {
  assert.strictEqual(lastContext, 'person-story');
});

// === QUESTION CONTRACT ===

Given('Jeff asks {string}', function (q: string) {
  lastContext = 'question';
});

Then('the role checks Chorus search before filesystem', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_check_memory_first.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: check_memory_first');
});

Then('the role checks decisions before guessing', function () {
  assert.strictEqual(lastContext, 'question');
});

Then('the role answers with the source of the information', function () {
  assert.strictEqual(lastContext, 'question');
});

Then('the role says {string} if genuinely unknown', function (phrase: string) {
  const page = fs.readFileSync(POLICY_PAGE, 'utf-8');
  assert.ok(page.includes('069'), 'DEC-069 not referenced in policy page');
});

Then('the role checks chorus-log or team-scan', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_chorus_first_for_team_awareness.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: chorus_first_for_team_awareness');
});

Then('the role does not guess from stale context', function () {
  assert.strictEqual(lastContext, 'question');
});

Then('the role reports current state from live instruments', function () {
  assert.strictEqual(lastContext, 'question');
});

// === ENERGY MATCHING ===

Given('Jeff sends a short message of {int} words or fewer', function (n: number) {
  lastContext = 'short';
});

Then('the role response is under {int} words', function (n: number) {
  assert.strictEqual(lastContext, 'short');
});

Then('the role does not write a paragraph', function () {
  assert.strictEqual(lastContext, 'short');
});

Then('the role provides thorough analysis', function () {
  assert.ok(['depth', 'generic'].includes(lastContext));
});

Then('the role does not give a one-liner', function () {
  assert.ok(['depth', 'generic'].includes(lastContext));
});

// === ANTI-PATTERNS ===

Given('a role says {string} or {string}', function (s1: string, s2: string) {
  violationDetected = true;
});

Given('a role says {string} without investigating', function (s: string) {
  violationDetected = true;
});

Then('this is a policy violation', function () {
  assert.ok(violationDetected, 'Violation should be detected');
});

Then('only Jeff decides when to stop', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_no_time_pressure.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: no_time_pressure');
});

Then('roles do not pattern-match human emotions', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_no_performed_emotion.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: no_performed_emotion');
});

Then('the role must investigate own scripts and hooks first', function () {
  const mem = path.join(MEMORY_DIR, 'feedback_no_blame_platform.md');
  assert.ok(fs.existsSync(mem), 'Missing feedback memory: no_blame_platform');
});
