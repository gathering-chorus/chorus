/**
 * #4350 — one definition of an open card. Jeff, 2026-09-26: "i want to see any
 * open card regardless of bucket/column". Open used to be eight hand-typed
 * lists of column names; his Keep column was in none of them, so its cards
 * vanished from `cards list`, the chunk projection and /sup. Now a card is open
 * unless its column is one of the two that close it, and a column added
 * tomorrow is open by default.
 */
export const CLOSED: ReadonlySet<string> = new Set(['Done', "Won't Do"]);

export function isOpen(status: string): boolean {
  return !CLOSED.has(status);
}

/** The columns people know, in the order they read them. Anything else follows, by name. */
const PREFERRED = ['Now', 'WIP', 'SWAT', 'Harvesting', 'Blocked', 'Next', 'Keep', 'Later', 'Jeff Tickets', 'Tech Debt'];

/** The columns present, in reading order; closed ones last, and only when asked for. */
export function statusOrder(present: Iterable<string>, includeClosed = false): string[] {
  const seen = new Set(present);
  const open = [...seen].filter(isOpen);
  const known = PREFERRED.filter(s => seen.has(s));
  const rest = open.filter(s => !PREFERRED.includes(s)).sort();
  const closed = includeClosed ? ["Won't Do", 'Done'].filter(s => seen.has(s)) : [];
  return [...known, ...rest, ...closed];
}
