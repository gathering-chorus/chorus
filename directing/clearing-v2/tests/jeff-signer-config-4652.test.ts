// @test-type: unit — temp files only; no relay, no real key.
// @domain: messages
// @card: #4652
// @owner: wren
// #4652 reopen 2 — the switch that turns on "stored as Jeff". Off keeps #4608's room copy.
import fs from 'fs';
import os from 'os';
import path from 'path';
import { buildJeffPublisher, clientKey } from '../src/jeff-signer-config';
import { JeffPublisher } from '../src/jeff-publisher';

const A = 'a'.repeat(64), B = 'b'.repeat(64);
const tmpFile = () => path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'signer-cfg-4652-')), 'client.key');

describe('#4652 the signing-service switch', () => {
  beforeEach(() => { jest.spyOn(console, 'log').mockImplementation(() => undefined); jest.spyOn(console, 'error').mockImplementation(() => undefined); });
  afterEach(() => jest.restoreAllMocks());

  test('with no config, Jeff\'s posts keep the room copy (no publisher)', () => {
    expect(buildJeffPublisher(undefined, 'ws://relay')).toBeUndefined();
    expect(buildJeffPublisher({}, 'ws://relay')).toBeUndefined();
  });

  test('with both keys set, a publisher is built and the room\'s client key is created owner-only', () => {
    const file = tmpFile();
    expect(buildJeffPublisher({ bunkerPubkey: A, userPubkey: B, clientKeyFile: file }, 'ws://relay')).toBeInstanceOf(JeffPublisher);
    expect(fs.statSync(file).mode & 0o777).toBe(0o600);
  });

  test('the client key is made once and reused, so the signer\'s allow-list stays valid', () => {
    const file = tmpFile();
    expect(clientKey(file)).toBe(clientKey(file));
  });

  test('NEGATIVE PROOF: a half-filled config is refused, not half-used', () => {
    expect(buildJeffPublisher({ bunkerPubkey: A }, 'ws://relay')).toBeUndefined();
    expect(buildJeffPublisher({ bunkerPubkey: 'not-hex', userPubkey: B }, 'ws://relay')).toBeUndefined();
  });
});
