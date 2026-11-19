// browser_signin_steps.ts — #5015 AC3. Jeff's browser path in login.feature: the human
// signs in too ("there's no power-on self-test for the human", Jeff 11-18). Each step
// drives fixtures/browser-signin-5015.py: the guard's real session functions and
// person_session.open_session with a stub door, state in this scenario's world.
// DOOR_5015=401 replays the #4517 regression ("login has no id"): the renew and
// recorded-session scenarios go red, which is this file's negative proof.
import { Before, Given, When, Then } from '@cucumber/cucumber';
import { execFileSync } from 'child_process';
import * as assert from 'assert';
import * as path from 'path';
import { loginWorld } from './login_steps';

const ROOT = path.resolve(__dirname, '../../../..');
const DRIVER = path.join(ROOT, 'platform/tests/fixtures/browser-signin-5015.py');
const STEP = { timeout: 15_000 };
// per-scenario: cleared before each @browser scenario, keyed by device
let last: Record<string, any> = {};
Before({ tags: '@browser' }, () => { last = {}; });

function drive(action: string, arg: string): any {
  const out = execFileSync('python3', [DRIVER, path.join(loginWorld(), 'browser-5015.json'), action, arg], { encoding: 'utf8' });
  return JSON.parse(out.trim().split('\n').pop() || '{}');
}

const signIn = (device: string) => { last[`signin:${device}`] = drive('signin', device); };
When('Jeff signs in on his {word}', STEP, signIn);
Given('Jeff has signed in on his {word}', STEP, signIn);
When('his {word} sits idle for an hour', STEP, (_d: string) => { drive('idle', '3700'); });
When('he opens the Clearing on his {word}', STEP, (device: string) => { last[`visit:${device}`] = drive('visit', device); });
When('Jeff signs out on his {word}', STEP, (device: string) => { drive('signout', device); });

Then("his {word}'s sign-in carries a recorded session", STEP, (device: string) => {
  const r = last[`signin:${device}`];
  assert.ok(r && r.sid, `the door recorded no Session for his ${device} sign-in: the cookie has no sid (${JSON.stringify(r)})`);
});
Then('he is signed in on his {word} and his sign-in is renewed', STEP, (device: string) => {
  const r = last[`visit:${device}`];
  assert.strictEqual(r.state, 'ok', `signed out after an idle hour: ${JSON.stringify(r)}`);
  assert.strictEqual(r.renewed, true, `not renewed after an idle hour, so it runs out: ${JSON.stringify(r)}`);
});
Then('Jeff has two recorded sessions', STEP, () => {
  const n = Math.max(...Object.keys(last).filter((k) => k.startsWith('signin:')).map((k) => last[k].sessions));
  assert.strictEqual(n, 2, JSON.stringify(last));
});
Then('he is signed in on both', STEP, () => {
  for (const d of ['phone', 'desktop']) assert.strictEqual(drive('visit', d).state, 'ok', `${d} not signed in`);
});
Then('his {word} is signed out', STEP, (device: string) => {
  assert.strictEqual(drive('visit', device).state, 'logged-out');
});
Then('he is still signed in on his {word}', STEP, (device: string) => {
  assert.strictEqual(drive('visit', device).state, 'ok');
});
