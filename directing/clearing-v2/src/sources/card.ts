// @domain: messages
// Card drawer: one card from the Athena cards door or Athena card detail API.

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
  comments?: Array<{ author?: string; text: string; time?: string }>;
}

export interface CardPayload {
  kind?: string;
  servedFrom?: string;
  comments?: Array<{ author?: string; text?: string; body?: string; time?: string; created?: string }>;
  data?: {
    label?: string;
    title?: string;
    status?: string;
    priority?: string;
    domain?: string;
    domains?: string[];
    description?: string;
    creator?: string;
    owner?: string;
    ac_items?: Array<{ text?: string; label?: string; checked?: boolean }>;
    acItems?: Array<{ text?: string; label?: string; checked?: boolean }>;
    comments?: Array<{ author?: string; text?: string; body?: string; time?: string; created?: string }>;
  };
  links?: {
    assignee?: string;
    ownedBy?: string;
  };
  owner?: string;
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

function extractAcItems(data: CardPayload['data']): Array<{ text: string; checked: boolean }> | undefined {
  if (!data) return undefined;
  const rawList = data.ac_items ?? data.acItems;
  if (Array.isArray(rawList) && rawList.length > 0) {
    return rawList.map((item) => ({
      text: String(item.text ?? item.label ?? ''),
      checked: Boolean(item.checked),
    }));
  }
  if (data.description) {
    const parsed = parseAcItems(data.description);
    if (parsed.length > 0) return parsed;
  }
  return undefined;
}

function extractComments(json: CardPayload): Array<{ author?: string; text: string; time?: string }> | undefined {
  const rawList = json.data?.comments ?? json.comments;
  if (!Array.isArray(rawList)) return undefined;
  const recent = rawList.slice(-3);
  return recent.map((c) => ({
    author: c.author,
    text: String(c.text ?? c.body ?? ''),
    time: c.time ?? c.created,
  }));
}

function extractOwner(json: CardPayload): string | undefined {
  const data = json.data;
  const raw = json.links?.assignee ?? json.links?.ownedBy ?? data?.creator ?? data?.owner ?? json.owner;
  return drawerOwner(raw);
}

export function toDrawerCard(cardId: number, json: CardPayload | null): DrawerCard | null {
  if (!json) return null;
  const data = json.data;
  if (!data || !(json.kind === 'Card' || data.label || data.title)) return null;

  return {
    id: cardId,
    title: data.label ?? data.title ?? `Card #${cardId}`,
    owner: extractOwner(json),
    status: data.status,
    priority: data.priority,
    domain: json.servedFrom ?? data.domain ?? (Array.isArray(data.domains) ? data.domains.join(', ') : undefined),
    description: data.description,
    acItems: extractAcItems(data),
    comments: extractComments(json),
  };
}

async function fetchFromAthena(athenaUrl: string, cardId: number): Promise<DrawerCard | null> {
  try {
    return await withTimeout(5000, async (signal) => {
      const res = await fetch(`${athenaUrl}/${cardId}`, { signal });
      const r = await readJson<CardPayload | null>(res);
      return r.ok ? toDrawerCard(cardId, r.value) : null;
    });
  } catch {
    return null;
  }
}

async function fetchFromCardsDoor(cardsUrl: string, cardId: number): Promise<DrawerCard | null> {
  try {
    return await withTimeout(1500, async (signal) => {
      const res = await fetch(`${cardsUrl}/${cardId}`, { signal });
      const r = await readJson<CardPayload | null>(res);
      return r.ok ? toDrawerCard(cardId, r.value) : null;
    });
  } catch {
    return null;
  }
}

/** The card, or null when the door is unreachable, refuses, or has no such card (the route answers 404). */
export async function fetchDrawerCard(
  cardsUrl: string,
  cardId: number,
  options?: { athenaCardUrl?: string }
): Promise<DrawerCard | null> {
  const isDefaultCardsUrl = cardsUrl.includes('localhost:3360');
  const athenaUrl = options?.athenaCardUrl || (isDefaultCardsUrl ? (process.env.ATHENA_CARD_URL || 'http://localhost:3340/api/athena/card') : undefined);

  if (cardsUrl && !isDefaultCardsUrl) {
    const card = await fetchFromCardsDoor(cardsUrl, cardId);
    if (card) return card;
  }

  if (athenaUrl) {
    const card = await fetchFromAthena(athenaUrl, cardId);
    if (card) return card;
  }

  return fetchFromCardsDoor(cardsUrl, cardId);
}
