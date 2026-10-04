// @test-type: unit — signal is this package's own package.json + process env, no live service
// @domain: messages
// #4186 reopen (Silas, 2026-10-03 17:46): tests here call real tools, and a tool
// that fails nudged ops, because the test run never said it was a test.
// chorus_athena's error nudge reached Silas on 09-16, 09-17, 09-25, 10-01 and
// 10-03, each from this package's nightly run. server.ts suppresses the ops
// nudge under CHORUS_SYNTHETIC=1 (shouldNotifyOps); the test script must set it.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'fs';
import * as path from 'path';
import { shouldNotifyOps } from '../src/server';

const pkg = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'package.json'), 'utf8')) as { scripts: Record<string, string> };

test('the package test script marks the run as synthetic', () => {
  assert.match(pkg.scripts.test, /(^|\s)CHORUS_SYNTHETIC=1\s/);
});

test('under npm test, a failing tool does not nudge ops', () => {
  // Negative proof: drop CHORUS_SYNTHETIC=1 from the script and run `npm test` —
  // this case goes red, because the env is unset and a real error nudges.
  assert.equal(process.env.CHORUS_SYNTHETIC, '1');
  assert.equal(shouldNotifyOps('athena-make answered 500', '', process.env.CHORUS_SYNTHETIC === '1'), false);
});

test('outside a test run, the same error still nudges ops', () => {
  assert.equal(shouldNotifyOps('athena-make answered 500', '', false), true);
});
