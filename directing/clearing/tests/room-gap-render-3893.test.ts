// @test-type: unit — router classification only; brings its own world.
/**
 * #3893 — the gap marker has to survive the room's own filters.
 *
 * NEGATIVE PROOF (#3734): `the same text without the gap type is swept away`
 * runs the identical message through the real classifier with only the type
 * removed and shows it coming out hidden. That is not hypothetical — the marker
 * is authored by `system`, and the plumbing sweep hides unrecognised non-role
 * senders by design. Without the rule the notice about hidden messages would
 * itself be hidden, and every other test here would still pass, because they
 * only ever assert on what the room DOES show.
 */
import { MessageRouter } from '../src/router';

const MARKER = '2 messages from wren never arrived (#5, 6)';
const TS = '2026-08-15T15:30:00.000Z';

describe('#3893 gap marker — Jeff sees what never reached him', () => {
  it('renders, though it is authored by system', () => {
    const router = new MessageRouter();
    router.ingest({ from: 'system', text: MARKER, type: 'gap', ts: TS });
    const msg = router.getRecent(10, true).find((m) => m.text === MARKER);
    expect(msg).toBeDefined();
    expect(msg!.visible).toBe(true);
    expect(msg!.type).toBe('gap');
  });

  it('NEGATIVE: the same text without the gap type is swept away as plumbing', () => {
    // Identical sender, identical words — only the type is missing. This is the
    // state the rule exists to prevent, and it is reachable: any caller that
    // forgets the type loses the notice entirely.
    const router = new MessageRouter();
    router.ingest({ from: 'system', text: MARKER, ts: TS });
    const msg = router.getRecent(10, true).find((m) => m.text === MARKER);
    expect(msg?.visible ?? false).toBe(false);
  });

  it('survives the machinery sweep even when the marker quotes a filesystem path', () => {
    const noisy = '3 messages from silas never arrived (#11, 12, 13) — /Users/jeff/.chorus';
    const router = new MessageRouter();
    router.ingest({ from: 'system', text: noisy, type: 'gap', ts: TS });
    expect(router.getRecent(10, true).find((m) => m.text === noisy)!.visible).toBe(true);
  });

  it('does not make every system message visible — the exemption is narrow', () => {
    const batch = '[progress] batch 4 of 9';
    const router = new MessageRouter();
    router.ingest({ from: 'system', text: batch, ts: TS });
    const msg = router.getRecent(10, true).find((m) => m.text === batch);
    expect(msg?.visible ?? false).toBe(false);
  });
});
