// @test-type: unit — pure predicate over fields objects; no network, no live pulse
// #3963 — the nudge-suppression for the benign claude-code `server/discover`
// probe must be SIGNATURE-SCOPED. Negative proof (#3734): if the predicate
// returned true for anything other than the exact benign signature, a real
// transport error would stop nudging silas — the failure this test forbids.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { isBenignDiscoveryProbe } from '../src/transport';

const DISCOVER_BODY =
  '{"jsonrpc":"2.0","id":"server-discover-probe-1","method":"server/discover","params":{}}';

test('benign claude-code discovery probe is suppressed', () => {
  assert.equal(
    isBenignDiscoveryProbe({ status: '400', user_agent: 'claude-code/2.1.232 (cli)', body: DISCOVER_BODY }),
    true,
  );
});

test('NEGATIVE PROOF: a real tool-error 400 (no discover) still nudges', () => {
  assert.equal(
    isBenignDiscoveryProbe({ status: '400', user_agent: 'claude-code/2.1.232 (cli)', body: '{"method":"tools/call"}' }),
    false,
  );
});

test('NEGATIVE PROOF: server/discover from a NON-claude-code client still nudges', () => {
  assert.equal(
    isBenignDiscoveryProbe({ status: '400', user_agent: 'evil-scanner/9', body: DISCOVER_BODY }),
    false,
  );
});

test('NEGATIVE PROOF: a non-400 status is never treated as the benign probe', () => {
  assert.equal(
    isBenignDiscoveryProbe({ status: '500', user_agent: 'claude-code/2.1.232 (cli)', body: DISCOVER_BODY }),
    false,
  );
});

test('missing fields do not throw and do not suppress', () => {
  assert.equal(isBenignDiscoveryProbe({}), false);
  assert.equal(isBenignDiscoveryProbe({ status: '400' }), false);
});
