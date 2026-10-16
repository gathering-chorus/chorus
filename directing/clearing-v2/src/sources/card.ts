// @domain: messages
// Card drawer: one card from the Athena cards door (single source).

import { CardDoorResponse } from './models';
import { withTimeout, readJson } from './http';
import { ownerBase, capitalize } from './format';

export interface DrawerCard {
  id: number;
  title: string;
  owner?: string;
  status?: string;
  domain?: string;
}

function drawerOwner(rawOwner: string | number | undefined): string | undefined {
  if (!rawOwner || typeof rawOwner !== 'string') return undefined;
  const base = ownerBase(rawOwner);
  const lower = base.toLowerCase();
  if (lower === 'abby-normal' || lower === 'abby normal') return 'Abby Normal';
  return capitalize(base);
}

function toDrawerCard(cardId: number, json: CardDoorResponse | null): DrawerCard | null {
  const data = json?.data;
  if (!json || !data || !(json.kind === 'Card' || data.label || data.title)) return null;
  return {
    id: cardId,
    title: data.label || data.title || `Card #${cardId}`,
    owner: drawerOwner(json.links?.assignee || json.links?.ownedBy || data.creator || json.owner),
    status: data.status,
    domain: json.servedFrom || data.domain,
  };
}

/** The card, or null when the door is unreachable, refuses, or has no such card (the route answers 404). */
export async function fetchDrawerCard(cardsUrl: string, cardId: number): Promise<DrawerCard | null> {
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
