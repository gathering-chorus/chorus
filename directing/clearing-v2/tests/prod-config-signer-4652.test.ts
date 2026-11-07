// @test-type: unit — reads the committed prod config; temp key file, no relay.
// @domain: messages
// @card: #4652
// @owner: wren
// #4652 reopen 4 — prod clearing-v2/clearing-v2.config.json turns the signing service on, so Jeff's
// Clearing posts are stored as Jeff. A half-filled or mistyped switch would silently keep
// the room copy (buildJeffPublisher refuses it), so the committed file is checked here.
import fs from 'fs';
import os from 'os';
import path from 'path';
import { buildJeffPublisher, JeffSignerConfig } from '../src/jeff-signer-config';
import { JeffPublisher } from '../src/jeff-publisher';

const JEFF = 'a42ccebeb2be92f57434f9d25243fb98be918f07752993283eeb95c98857c62a';
const cfg = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'clearing-v2.config.json'), 'utf8')) as { jeffSigner?: JeffSignerConfig; buzzRelayUrl?: string };

describe('#4652 prod config: the signing service is on', () => {
  beforeEach(() => { jest.spyOn(console, 'error').mockImplementation(() => undefined); });
  afterEach(() => jest.restoreAllMocks());

  test('the switch names Jeff\'s app key as the user and the bunker (the signer has no key of its own)', () => {
    expect(cfg.jeffSigner).toEqual(expect.objectContaining({ bunkerPubkey: JEFF, userPubkey: JEFF }));
  });

  test('the committed switch builds a publisher, so Jeff\'s posts do not fall back to the room copy', () => {
    const clientKeyFile = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'prodcfg-4652-')), 'client.key');
    expect(buildJeffPublisher({ ...cfg.jeffSigner, clientKeyFile }, cfg.buzzRelayUrl ?? '')).toBeInstanceOf(JeffPublisher);
  });
});
