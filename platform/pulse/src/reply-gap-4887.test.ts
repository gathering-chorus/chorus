// @test-type: unit — signal is fixture-data: Abby's two live room rows and their reply.emitted lines
// @domain: messages
// @card: #4887
// @owner: silas
//
// #4887 — the gap watch reads the room's own record (GET :3472/api/messages),
// not the reply.rendered spine echo the v1 tailer used to write (24h count ~0,
// so every reply read as a gap). Abby's 13:02:57 and 13:03:10 replies were in
// the room and the old watch called both undelivered: they are the fixture.
import * as fs from 'fs';
import * as path from 'path';
import { detectGaps, parseSpineTail, RoomMsg } from './reply-gap';
import { contentHash } from '../../chorus-sdk/reply/hash';

const FIX = path.join(__dirname, '../../tests/fixtures/4887');
const events = parseSpineTail(fs.readFileSync(path.join(FIX, 'abby-reply-emitted-4887.jsonl'), 'utf8'));
const room: RoomMsg[] = JSON.parse(fs.readFileSync(path.join(FIX, 'abby-room-rows-4887.json'), 'utf8'));
const NOW = Date.parse('2026-11-04T13:05:00-05:00');
const WINDOW = 60_000;

describe('#4887 gap watch reads the room record', () => {
  it('fixture is whole: two emitted lines, two room rows (a deleted fixture fails loud)', () => {
    expect(events.filter((e) => e.event === 'reply.emitted')).toHaveLength(2);
    expect(room).toHaveLength(2);
  });

  it("the room's text hashes to the emitted hash (one join key)", () => {
    expect(room.map((m) => contentHash(m.text ?? ''))).toEqual(['6c48cfe7645039a1', '8be555a4b26569e7']);
  });

  it("AC2: Abby's two rendered replies count as delivered", () => {
    expect(detectGaps(events, room, NOW, WINDOW, new Set())).toEqual({ gaps: [], unmeasured: 0 });
  });

  // NEGATIVE PROOF (#3734): drop the 13:03:10 row from the room and that reply must gap.
  it('AC3: a reply missing from the room still raises a gap', () => {
    const scan = detectGaps(events, room.slice(0, 1), NOW, WINDOW, new Set());
    expect(scan.gaps).toEqual([{ role: 'abby-normal', hash: '8be555a4b26569e7', emittedAt: '2026-11-04T13:03:10.373-0500' }]);
  });

  it('AC6: a reply.rendered spine echo no longer counts as delivery', () => {
    const echo = { event: 'reply.rendered', role: 'abby-normal', hash: '8be555a4b26569e7', timestamp: '2026-11-04T13:03:11-05:00' };
    expect(detectGaps([...events, echo], room.slice(0, 1), NOW, WINDOW, new Set()).gaps).toHaveLength(1);
  });

  it('an unreadable room reads unmeasured, never a gap', () => {
    expect(detectGaps(events, null, NOW, WINDOW, new Set())).toEqual({ gaps: [], unmeasured: 2 });
  });

  it("a reply older than the room's oldest row is unmeasured (the buffer scrolled), not a gap", () => {
    const later: RoomMsg[] = [{ from: 'wren', text: 'later', ts: '2026-11-04T18:04:00.000Z' }];
    expect(detectGaps(events, later, NOW, WINDOW, new Set())).toEqual({ gaps: [], unmeasured: 2 });
  });
});
