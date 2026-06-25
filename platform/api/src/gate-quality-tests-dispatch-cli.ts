#!/usr/bin/env node
// #3580 — gate:quality subdomain-dispatch CLI. The thin caller the gate-quality
// skill invokes: resolves the tests covering a card's subdomain from the LIVE
// tests API (owl-api :3360/tests, #2819), prints the receipt, and emits a spine
// event so the consumption is OBSERVABLE (AC4) — not a silent read. Exits 0
// always: this consumer is fail-open and must never block the gate (AC5).
import { spawnSync } from 'child_process';
import { dispatchForCard } from './gate-quality-tests-dispatch';

async function main(): Promise<void> {
  const subdomain = process.argv[2];
  const card = process.argv[3] ?? '';
  if (!subdomain) {
    process.stderr.write('usage: gate-quality-tests-dispatch <subdomain> [card-id]\n');
    process.exit(2);
  }

  const r = await dispatchForCard(subdomain);

  // Human receipt — what the gate scoped its checks to.
  process.stdout.write(
    r.scoped
      ? `gate:quality → subdomain "${r.subdomain}": ${r.count} covering test(s) to check\n`
      : `gate:quality → subdomain "${r.subdomain}": no covering tests in the API (degrade to suite-level)\n`,
  );
  for (const t of r.coveringTests) process.stdout.write(`  · ${t}\n`);

  // Observable receipt: a spine event proving the generated tests API was
  // actually consumed for this card. Best-effort — never affects the exit.
  emitConsulted(r.subdomain, r.count, r.scoped, card);

  process.exit(0);
}

function emitConsulted(subdomain: string, count: number, scoped: boolean, card: string): void {
  const home = process.env.CHORUS_HOME ?? '/Users/jeffbridwell/CascadeProjects/chorus';
  const log = `${home}/platform/scripts/chorus-log`;
  try {
    spawnSync(
      'bash',
      [
        log,
        'gate.quality.tests.consulted',
        'kade',
        `card=${card}`,
        `subdomain=${subdomain}`,
        `covering=${count}`,
        `scoped=${scoped}`,
      ],
      { stdio: 'ignore' },
    );
  } catch {
    /* best-effort: a missing log script never blocks the gate */
  }
}

void main();
