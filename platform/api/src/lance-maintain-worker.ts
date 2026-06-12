// lance-maintain-worker.ts — nightly lance store maintenance, OFF-PROCESS (#3379)
//
// Runs compact → prune → re-ensure-vector-index against ~/.chorus/lance via the
// proven maintainTable (#3157: 31G→14G June 4; 35,032 fragments → 3,109 on
// 2026-06-12 when fragments regrew because no schedule existed — this file IS
// that schedule's body). Launched by chorus-lance-maintain.sh from the
// com.chorus.lance-maintain LaunchAgent; never imported by chorus-api — heavy
// fs work on the serving loop is the wedge class this card kills (#3085 pattern).
import * as os from 'os';
import * as path from 'path';
import * as lancedb from '@lancedb/lancedb';
import { maintainTable } from './lance-store';

const LANCE_DIR = process.env.CHORUS_LANCE_DIR ?? path.join(os.homedir(), '.chorus', 'lance');
const TABLE = process.env.CHORUS_LANCE_TABLE ?? 'messages';

async function main(): Promise<void> {
  const db = await lancedb.connect(LANCE_DIR);
  const table = await db.openTable(TABLE);

  // Zero-row safety: refuse to "maintain" a table that has lost its data —
  // compaction on an empty store would only cement the loss. Fail loud instead.
  const rowsBefore = await table.countRows();
  if (rowsBefore === 0) {
    throw new Error(`lance-maintain: ${TABLE} has 0 rows — refusing to optimize; investigate before maintaining`);
  }

  const result = await maintainTable(table, {});
  const rowsAfter = await table.countRows();
  if (rowsAfter !== rowsBefore) {
    throw new Error(`lance-maintain: row count changed ${rowsBefore} → ${rowsAfter} during maintenance — investigate`);
  }

  // One parseable line — the wrapper script tees this into the watched log and
  // the spine emit. Stable keys; the numbers are the receipt.
  console.log(JSON.stringify({
    table: TABLE,
    rows: rowsAfter,
    reindexed: result.reindexed,
    optimize: result.optimize,
  }));
}

main().catch((err: Error) => {
  console.error(`lance-maintain: FAIL — ${err.message}`);
  process.exit(1);
});
