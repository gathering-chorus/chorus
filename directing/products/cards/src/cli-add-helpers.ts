/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Local CLI helper. fs reads come from --desc-file argument supplied by the
 * role running the CLI; object indexing is on validated argv option names.
 * Local trust model — no HTTP exposure.
 */
/**
 * Helpers for `cards add` parsing (#2223 ergonomics).
 *
 * parseAddArgs: pure parser, throws on error (testable). Handles
 * --desc-file <path> and --desc - (stdin). --desc-file takes precedence
 * over inline --desc when both present.
 */
import * as fs from 'fs';

export interface AddArgs {
  title: string;
  status: string;
  owner: string;
  priority: string;
  domain: string;
  description: string;
  product: string;
  chunk: string;
  sequence: string;
  type: string;
  origin: string;
  // #2652 AC1+AC2 — new tag axes per cards-service-design v1
  subdomain: string;
  subproduct: string;
}

const USAGE =
  'Usage: cards add "title" [--status S] [--owner O] [--priority P] [--domain D] ' +
  '[--product P] [--chunk C] [--sequence S] [--subproduct SP] [--subdomain SD] [--type T] [--origin O] ' +
  '[--desc D | --desc-file PATH | --desc -]';

/** String-valued field names. */
type StringField = keyof AddArgs;

const STRING_FLAGS: Partial<Record<string, StringField>> = {
  '--status': 'status',
  '--owner': 'owner',
  '--priority': 'priority',
  '--domain': 'domain',
  '--description': 'description',
  '--desc': 'description',
  '--product': 'product',
  '-p': 'product',
  '--chunk': 'chunk',
  '--sequence': 'sequence',
  '--seq': 'sequence',
  '--type': 'type',
  '-t': 'type',
  '--origin': 'origin',
  // #2652 AC1+AC2
  '--subdomain': 'subdomain',
  '--subproduct': 'subproduct',
};

function resolveDescription(description: string, descFile: string): string {
  if (descFile) {
    if (!fs.existsSync(descFile)) throw new Error(`--desc-file path does not exist: ${descFile}`);
    return fs.readFileSync(descFile, 'utf-8');
  }
  if (description === '-') return fs.readFileSync(0, 'utf-8');
  return description;
}

export function parseAddArgs(args: string[]): AddArgs {
  const out: AddArgs = {
    title: '', status: 'later', owner: '', priority: '',
    domain: '', description: '', product: '', chunk: '', sequence: '',
    type: '', origin: '', subdomain: '', subproduct: '',
  };
  let descFile = '';

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--desc-file') { descFile = args[++i]; continue; }
    const field = STRING_FLAGS[arg];
    if (field) {
      out[field] = args[++i];
      continue;
    }
    if (!out.title) out.title = arg;
    else throw new Error(`Unexpected argument: ${arg}`);
  }

  out.description = resolveDescription(out.description, descFile);
  if (!out.title) throw new Error(USAGE);
  return out;
}
