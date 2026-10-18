// @domain: messages
// Card drawer: one card from the Athena cards door or Athena card detail API.

import { CardDoorResponse } from './models';
import { withTimeout, readJson } from './http';
import { ownerBase, capitalize } from './format';

export interface DrawerCard {
  id: number;
  title: string;
  owner?: string;
  status?: string;
  priority?: string;
  domain?: string;
  description?: string;
  acItems?: Array<{ text: string; checked: boolean }>;
}

function drawerOwner(rawOwner: string | number | undefined): string | undefined {
  if (!rawOwner || typeof rawOwner !== 'string') return undefined;
  const base = ownerBase(rawOwner);
  const lower = base.toLowerCase();
  if (lower === 'abby-normal' || lower === 'abby normal') return 'Abby Normal';
  return capitalize(base);
}

function parseAcItems(description: string | undefined): Array<{ text: string; checked: boolean }> {
  if (!description) return [];
  const matches = description.match(/- \[([ x])\] .+/g) ?? [];
  return matches.map((line) => ({
    text: line.replace(/^- \[[ x]\] /, ''),
    checked: line.startsWith('- [x]'),
  }));
}

export function toDrawerCard(cardId: number, json: any): DrawerCard | null {
  if (!json) return null;
  const data = json.data || json;
  if (!data || !(json.kind === 'Card' || data.label || data.title)) return null;

  const rawOwner = json.links?.assignee || json.links?.ownedBy || data.creator || data.owner || json.owner;
  const description = typeof data.description === 'string' ? data.description : undefined;

  let acItems: Array<{ text: string; checked: boolean }> | undefined;
  if (Array.isArray(data.ac_items)) {
    acItems = data.ac_items.map((item: any) => ({
      text: String(item.text || item.label || ''),
      checked: Boolean(item.checked),
    }));
  } else if (Array.isArray(data.acItems)) {
    acItems = data.acItems.map((item: any) => ({
      text: String(item.text || item.label || ''),
      checked: Boolean(item.checked),
    }));
  } else if (description) {
    const parsed = parseAcItems(description);
    if (parsed.length > 0) acItems = parsed;
  }

  return {
    id: cardId,
    title: data.label || data.title || `Card #${cardId}`,
    owner: drawerOwner(rawOwner),
    status: data.status,
    priority: data.priority,
    domain: json.servedFrom || data.domain,
    description,
    acItems,
  };
}

/** The card, or null when the door is unreachable, refuses, or has no such card (the route answers 404). */
export async function fetchDrawerCard(
  cardsUrl: string,
  cardId: number,
  options?: { athenaCardUrl?: string }
): Promise<DrawerCard | null> {
  const isDefaultCardsUrl = cardsUrl.includes('localhost:3360');
  const athenaUrl = options?.athenaCardUrl || (isDefaultCardsUrl ? (process.env.ATHENA_CARD_URL || 'http://localhost:3340/api/athena/card') : undefined);

  // 1. If a custom cardsUrl was provided (e.g. test stub), query it first
  if (cardsUrl && !isDefaultCardsUrl) {
    try {
      const card = await withTimeout(1500, async (signal) => {
        const res = await fetch(`${cardsUrl}/${cardId}`, { signal });
        const r = await readJson<any>(res);
        return r.ok ? toDrawerCard(cardId, r.value) : null;
      });
      if (card) return card;
    } catch {
      // Fall through
    }
  }

  // 2. Next try Athena card endpoint (which returns full description + parsed AC)
  if (athenaUrl) {
    try {
      const card = await withTimeout(1500, async (signal) => {
        const res = await fetch(`${athenaUrl}/${cardId}`, { signal });
        const r = await readJson<any>(res);
        return r.ok ? toDrawerCard(cardId, r.value) : null;
      });
      if (card && (card.description || card.acItems)) return card;
      if (card) return card;
    } catch {
      // Fall through to default cardsUrl
    }
  }

  // 3. Fallback to default cardsUrl (door endpoint)
  try {
    return await withTimeout(1500, async (signal) => {
      const res = await fetch(`${cardsUrl}/${cardId}`, { signal });
      const r = await readJson<CardDoorResponse | null>(res);
      return r.ok ? toDrawerCard(cardId, r.value) : null;
    });
  } catch {
    return null; // unreachable door -> no card; the route reports it as not found
  }
}
