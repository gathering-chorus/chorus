// @domain: cards
// #4574 — the Board tab reads the board API's columns as they are served; v2 sorts
// nothing into columns itself. chorus-api serves /board/wip and /board/next; a column
// with no API (Done today) says so, it is never shown as empty.
export interface BoardCard { id: number; title: string; owner?: string; status?: string }
export type BoardColumn =
  | { name: string; ok: true; cards: BoardCard[] }
  | { name: string; ok: false; reason: string };

/** The columns the board API serves, in the order the page shows them. */
export const BOARD_COLUMNS: Array<{ name: string; path: string | null }> = [
  { name: 'WIP', path: 'wip' },
  { name: 'Next', path: 'next' },
  { name: 'Done', path: null },
];

export async function fetchBoardColumn(boardUrl: string, col: { name: string; path: string | null }): Promise<BoardColumn> {
  if (!col.path) return { name: col.name, ok: false, reason: 'no board API serves this column yet' };
  if (!boardUrl) return { name: col.name, ok: false, reason: 'board API not configured' };
  try {
    const res = await fetch(`${boardUrl}/${col.path}`, { signal: AbortSignal.timeout(5000) });
    if (!res.ok) return { name: col.name, ok: false, reason: `board API answered ${res.status}` };
    const json = (await res.json()) as { data?: { cards?: Array<{ id?: number | string; title?: string; owner?: string; status?: string }> } };
    const rows = json.data?.cards;
    if (!Array.isArray(rows)) return { name: col.name, ok: false, reason: 'board API returned no card list' };
    return {
      name: col.name, ok: true,
      cards: rows.map((c) => ({ id: Number(c.id), title: c.title ?? '', owner: c.owner || undefined, status: c.status || undefined })),
    };
  } catch (e) {
    return { name: col.name, ok: false, reason: `board API unreachable: ${(e instanceof Error ? e.message : String(e)).split('\n')[0]}` };
  }
}

export async function fetchBoard(boardUrl: string): Promise<BoardColumn[]> {
  return Promise.all(BOARD_COLUMNS.map((c) => fetchBoardColumn(boardUrl, c)));
}
