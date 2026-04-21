/**
 * AC tick detection (#2193 wave 1).
 *
 * Pure function — compares old vs new card description and reports how many
 * AC items flipped from `- [ ]` to `- [x]` in the update. Drives emission
 * of the `ac.ticked` spine event in updateCard / setCard.
 *
 * Kept deliberately loose on formatting: tolerates indentation and tracks
 * content as the key, so reordering doesn't cause false positives.
 */

const AC_LINE_RE = /^\s*- \[( |x|X)\] (.+)$/;

interface AcItem {
  content: string;
  checked: boolean;
}

export interface AcDiff {
  tickedCount: number;
  totalChecked: number;
  totalAc: number;
}

export function countAcDiff(before: string, after: string): AcDiff {
  const beforeItems = parseAcItems(before);
  const afterItems = parseAcItems(after);

  // Map of content → checked state before update, for lookup.
  const beforeByContent = new Map<string, boolean>();
  for (const item of beforeItems) beforeByContent.set(item.content, item.checked);

  let tickedCount = 0;
  for (const item of afterItems) {
    if (!item.checked) continue;
    const wasChecked = beforeByContent.get(item.content);
    // Flipped from unchecked (false) or brand-new-and-already-checked (undefined)
    if (wasChecked !== true) tickedCount++;
  }

  const totalChecked = afterItems.filter((i) => i.checked).length;
  const totalAc = afterItems.length;
  return { tickedCount, totalChecked, totalAc };
}

function parseAcItems(desc: string): AcItem[] {
  const out: AcItem[] = [];
  for (const line of desc.split('\n')) {
    const m = line.match(AC_LINE_RE);
    if (!m) continue;
    const marker = m[1];
    const content = m[2].trim();
    out.push({ content, checked: marker === 'x' || marker === 'X' });
  }
  return out;
}
