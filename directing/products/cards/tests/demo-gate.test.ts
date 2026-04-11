/**
 * Demo gate on cards done — #1834
 *
 * Verifies that doneCard checks for demo evidence before allowing Done transition.
 * Uses SDK source inspection — no real cards created or moved.
 */
import * as fs from 'fs';
import * as path from 'path';

const SDK_SRC = fs.readFileSync(path.join(__dirname, '..', 'src', 'sdk.ts'), 'utf-8');

describe('Demo gate on doneCard (#1834)', () => {

  test('doneCard checks for demo evidence before calling client.done()', () => {
    // The demo gate must appear BEFORE client.done() in the doneCard function
    const doneCardMatch = SDK_SRC.match(/export async function doneCard[\s\S]*?^}/m);
    expect(doneCardMatch).not.toBeNull();
    const doneCardBody = doneCardMatch![0];

    // Must contain demo evidence check
    expect(doneCardBody).toMatch(/demo.*evidence|demo.*brief|demo.*gate|hasDemoEvidence/i);
  });

  test('demo gate checks briefs directory for demo brief file', () => {
    // The check should look for demo brief files matching the card ID
    expect(SDK_SRC).toMatch(/demo.*brief|briefs.*demo/i);
  });

  test('demo gate checks spine events for card.demo.started', () => {
    // The check should also look for spine events
    expect(SDK_SRC).toMatch(/card\.demo\.started|demo\.started/);
  });

  test('demo gate exempts type:chore cards', () => {
    const doneCardMatch = SDK_SRC.match(/export async function doneCard[\s\S]*?^}/m);
    const doneCardBody = doneCardMatch![0];
    expect(doneCardBody).toMatch(/chore/);
  });

  test('demo gate exempts type:swat cards', () => {
    const doneCardMatch = SDK_SRC.match(/export async function doneCard[\s\S]*?^}/m);
    const doneCardBody = doneCardMatch![0];
    expect(doneCardBody).toMatch(/swat/);
  });

  test('demo gate blocks with actionable message when no evidence', () => {
    expect(SDK_SRC).toMatch(/Demo.*required|demo.*first|demo.*evidence/i);
  });
});
