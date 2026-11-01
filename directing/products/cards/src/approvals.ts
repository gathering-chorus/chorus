/**
 * #4505 — an agent's card ask is ONE Approval row on the door (decisions domain),
 * written as the asking role. Jeff decides it on the go page with his passkey;
 * chorus-go replays the argv as data. No pending files, no chat keyword.
 */
import { ATHENA_MAKE, identityToken } from './graph-sync';

/** Mirrors chorus-go's CARDS_ADD_FLAGS: the only flags an approved ask may carry. */
export const CARDS_ADD_FLAGS = ['--owner', '--priority', '--domain', '--type', '--origin', '--desc', '--sequence'] as const;

export const APPROVALS_ROUTE = '/decisions/approvals';
export const APPROVALS_PAGE = process.env.CHORUS_GO_APPROVALS_URL || 'https://lightlifeurbangardens.com/go/approvals';

export interface AskOpts {
  owner?: string; priority?: string; domain?: string; type?: string; origin?: string;
  description?: string; sequence?: string;
  product?: string; chunk?: string; subproduct?: string; parent?: string; status?: string;
}

/** The `cards add` argv Jeff's approve replays. Refuses what the replay could not carry. */
export function buildApprovalArgv(title: string, opts: AskOpts): string[] {
  const carried: Array<[string, string | undefined]> = [
    ['--owner', opts.owner], ['--priority', opts.priority], ['--domain', opts.domain],
    ['--type', opts.type], ['--origin', opts.origin], ['--desc', opts.description], ['--sequence', opts.sequence],
  ];
  const dropped = (['product', 'chunk', 'subproduct', 'parent', 'status'] as const).filter((k) => opts[k]);
  if (dropped.length) {
    throw new Error(`not a cards add argv: an ask cannot carry ${dropped.map((k) => `--${k}`).join(', ')} yet; file it without, or ask Jeff to /card it`);
  }
  const argv = ['add', title];
  for (const [flag, val] of carried) if (val) argv.push(flag, val);
  checkApprovalArgv(argv);
  return argv;
}

/** Same rule chorus-go applies at replay: "add", a title, then allow-listed `--flag value` pairs. */
export function checkApprovalArgv(argv: unknown): string[] {
  if (!Array.isArray(argv) || argv.some((a) => typeof a !== 'string' || a.includes('\0'))) {
    throw new Error('not a cards add argv: an array of strings');
  }
  const a = argv as string[];
  if (a.length < 2 || a[0] !== 'add' || !a[1] || a[1].startsWith('-')) {
    throw new Error('not a cards add argv: it must start with "add" and a title');
  }
  for (let i = 2; i < a.length; i += 2) {
    if (!(CARDS_ADD_FLAGS as readonly string[]).includes(a[i])) throw new Error(`not a cards add argv: flag '${a[i]}' is not allowed`);
    const v = a[i + 1];
    if (v === undefined || v === '-' || v.startsWith('--')) throw new Error(`not a cards add argv: '${a[i]}' takes a value`);
  }
  return a;
}

/** One row per role+title: the door refuses a second row of the same name, so a retry is not a second ask. */
export function approvalName(role: string, title: string): string {
  const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 60) || 'ask';
  return `approval-${role}-${slug}`;
}

export type PostResult = { outcome: 'asked' | 'already-waiting'; name: string; othersWaiting: number };

type FetchFn = (url: string, init?: { method?: string; headers?: Record<string, string>; body?: string }) =>
  Promise<{ ok: boolean; status: number; json?: () => Promise<unknown>; text?: () => Promise<string> }>;

/** Post the ask as the role (the door stamps ownedBy from the verified token, never the body). */
export async function postApprovalRow(args: {
  role: string; title: string; argv: string[];
  fetchImpl?: FetchFn; token?: (role: string) => string; door?: string;
}): Promise<PostResult> {
  const f: FetchFn = args.fetchImpl || (fetch as unknown as FetchFn);
  const door = args.door || ATHENA_MAKE;
  const name = approvalName(args.role, args.title);
  checkApprovalArgv(args.argv);
  let othersWaiting = 0;
  try {
    const r = await f(`${door}${APPROVALS_ROUTE}?limit=10000`);
    if (r.ok && r.json) {
      const body = (await r.json()) as { data?: Array<{ name?: string }> };
      othersWaiting = (body.data || []).filter((d) => d.name !== name).length;
    }
  } catch { /* the count only decides whether to ping; the write below is what matters */ }
  const token = (args.token || identityToken)(args.role);
  const res = await f(`${door}${APPROVALS_ROUTE}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name, label: args.title, approvalKind: 'card', approvalSubject: args.title,
      approvalAction: JSON.stringify(args.argv),
    }),
  });
  if (res.status === 409) return { outcome: 'already-waiting', name, othersWaiting };
  if (!res.ok) {
    const t = res.text ? (await res.text()).slice(0, 200) : '';
    throw new Error(`the door refused the ask (${res.status}): ${t}`);
  }
  return { outcome: 'asked', name, othersWaiting };
}

/** The one line Jeff sees. A second ask while others wait adds to the list without a second link ping. */
export function headsUp(r: PostResult, role: string, title: string): string {
  if (r.outcome === 'already-waiting') return `[card-approval] already waiting on Jeff: "${title}" (${role}). Not asked again.`;
  if (r.othersWaiting > 0) return `[card-approval] ${role} added "${title}" to Jeff's list (${r.othersWaiting + 1} waiting).`;
  return `[card-approval] ${role} asks Jeff: "${title}" — approve or decline at ${APPROVALS_PAGE}`;
}
