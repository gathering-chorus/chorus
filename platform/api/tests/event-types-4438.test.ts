// @test-type: unit — event types by producer and subject, from the generated registry
// @card: #4438
// @owner: wren
import * as path from 'path';
import { typesFor, filterSpineEvents, type SpineEventRow } from '../src/lib/spine-events';
import { loadSpineSchema } from '../src/spine-event-write';

const registry = loadSpineSchema(path.resolve(__dirname, '../../../designing/schemas/spine-events.json')).events ?? {};

describe('#4438 — consume by producer and by subject', () => {
  it('card.pulled is produced by cards, about Card, read by Pulse and the werk service', () => {
    expect(registry['card.pulled']).toMatchObject({ producer: 'cards', about: 'Card', category: 'fact', version: '1' });
    expect((registry['card.pulled'] as { consumers?: string[] }).consumers).toEqual(['pulse', 'service-werk']);
  });

  it('producer=cards names card.pulled and not a hooks event', () => {
    const t = typesFor(registry, { producer: 'cards' })!;
    expect(t).toContain('card.pulled');
    expect(t).not.toContain('hook.decision');
  });

  it('about=Card names card.pulled; producer and about combine', () => {
    expect(typesFor(registry, { about: 'Card' })).toContain('card.pulled');
    expect(typesFor(registry, { producer: 'gates', about: 'Card' })).toEqual([]);
  });

  it('no producer or about asked = no type filter', () => {
    expect(typesFor(registry, {})).toBeUndefined();
  });

  it('an unknown producer matches nothing, never everything (negative proof)', () => {
    const rows: SpineEventRow[] = [
      { event: 'card.pulled', role: 'wren', ts: 1 } as SpineEventRow,
      { event: 'hook.decision', role: 'wren', ts: 2 } as SpineEventRow,
    ];
    const t = typesFor(registry, { producer: 'no-such-domain' })!;
    expect(t).toEqual([]);
    const byCards = filterSpineEvents(rows, { types: typesFor(registry, { producer: 'cards' }) });
    expect(byCards.map((r) => r.event)).toEqual(['card.pulled']);
  });
});
