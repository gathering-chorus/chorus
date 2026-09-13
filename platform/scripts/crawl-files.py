#!/usr/bin/env python3
"""crawl-files.py — the one walker's file pass (#4154 B2).

Walks CHORUS_ROOT once and persists one CodeFile row per file THROUGH THE
GENERATED API (POST /codefiles/batch), never raw SPARQL:

  name        sha1(absolute path)     ADR-040 Rule 0: the caller hands (type, name),
                                      the DAL mints the IRI. One IRI per file.
  filePath    repo-relative path
  hasKind     code | test | log | config | doc   (named CodeKind rows, #4157)
  hasLanguage rust | typescript | bash | …       (named Language rows, #4157)
  fileSha     sha256, for change detection
  fileLastModified  mtime

Rows land in urn:chorus:domains:code because the CodeFileShape declares that
instances graph (ADR-051; urn:chorus:instances is frozen). The crawler identity
(#4154, principal-crawler) is scoped to the code and tests graphs only.

Kinds and languages are named individuals: a file whose extension has no
Language row is written WITHOUT hasLanguage rather than with a free string —
the door would refuse it (unknown-target), and inventing a value is the defect
this card exists to remove.

Env: ATHENA_MAKE_URL (default :3360) · CHORUS_ROOT · CHORUS_ROLE (crawler)
     CRAWL_BATCH (default 200) · CRAWL_LIMIT (0 = no limit) · CRAWL_DRY_RUN=1
"""
import hashlib, json, os, subprocess, sys, time, urllib.error, urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import testfiles

URL   = os.environ.get("ATHENA_MAKE_URL", "http://localhost:3360").rstrip("/")
ROOT  = os.environ.get("CHORUS_ROOT", os.path.expanduser("~/CascadeProjects/chorus"))
ROLE  = os.environ.get("CHORUS_ROLE", "crawler")
BATCH = int(os.environ.get("CRAWL_BATCH", "200"))
LIMIT = int(os.environ.get("CRAWL_LIMIT", "0"))
DRY   = os.environ.get("CRAWL_DRY_RUN") == "1"

# extension → Language row name (designing/data/code-vocab.ttl). A file whose
# extension is absent here gets no hasLanguage: the value set is the model's.
LANG = {
    ".rs": "rust", ".ts": "typescript", ".tsx": "typescript", ".mts": "typescript",
    ".js": "javascript", ".cjs": "javascript", ".mjs": "javascript", ".jsx": "javascript",
    ".sh": "bash", ".bash": "bash", ".bats": "bash", ".py": "python",
    ".md": "markdown", ".ttl": "turtle", ".html": "html", ".htm": "html",
    ".css": "css", ".json": "json", ".yml": "yaml", ".yaml": "yaml",
    ".toml": "toml", ".sql": "sql",
}
# extension → CodeKind row name when the file is not a test (testfiles decides that).
KIND_BY_EXT = {
    ".md": "doc", ".html": "doc", ".htm": "doc",
    ".json": "config", ".yml": "config", ".yaml": "config", ".toml": "config",
    ".ttl": "config", ".conf": "config", ".plist": "config",
    ".log": "log",
}
EXCLUDE_DIRS = {".git", "node_modules", "target", "dist", "coverage", ".venv", "__pycache__"}

def token():
    if os.environ.get("CHORUS_IDENTITY_TOKEN"):
        return os.environ["CHORUS_IDENTITY_TOKEN"]
    script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "chorus-identity-token")
    out = subprocess.run([script, ROLE], capture_output=True, text=True, timeout=30)
    t = out.stdout.strip()
    if not t:
        sys.exit(f"crawl-files: no identity token for {ROLE}: {out.stderr.strip()[:200]}")
    return t

def walk(root):
    for d, dirs, files in os.walk(root):
        dirs[:] = [x for x in dirs if x not in EXCLUDE_DIRS and not x.startswith("dist.")]
        for f in files:
            p = os.path.join(d, f)
            if os.path.islink(p):
                continue
            yield p

def row(abs_path, rel):
    """A CodeFile row, or None for a file this model has no words for.

    A .jpg is not a code file. The model's CodeKind set is code|test|log|config|doc
    and its Language set is the 13 in code-vocab.ttl (#4157); a file outside both
    is SKIPPED and counted, never written as "code" by default — inventing a kind
    is the defect this card removes. Widening the set is a model edit, not a
    walker edit.
    """
    ext = os.path.splitext(abs_path)[1].lower()
    try:
        content = open(abs_path, errors="ignore").read() if ext == ".rs" else None
        st = os.stat(abs_path)
        sha = hashlib.sha256(open(abs_path, "rb").read()).hexdigest()
    except OSError:
        return None
    if testfiles.is_test_file(rel, content):
        kind = "test"
    elif ext in KIND_BY_EXT:
        kind = KIND_BY_EXT[ext]
    elif ext in LANG:
        kind = "code"
    else:
        return "skip"
    r = {
        "name": hashlib.sha1(abs_path.encode()).hexdigest(),
        "filePath": rel,
        "hasKind": kind,
        "fileSha": sha,
        "fileLastModified": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(st.st_mtime)),
    }
    lang = LANG.get(ext)
    if lang:
        r["hasLanguage"] = lang
    return r

def existing(tok):
    """name → fileSha for every CodeFile the door already serves.

    The walker is idempotent by DIFF, not by luck: create is create (a second
    POST of the same row is a 409 conflict, measured 2026-09-12 19:45), so a
    re-walk must send only what is new or changed. Unchanged files are not
    rewritten — a crawler that rewrites 5,000 unchanged rows every pass is the
    storm ADR-033 forbids.
    """
    out = {}
    url = f"{URL}/codefiles?limit=50000"
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {tok}"} if tok else {})
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            body = json.load(r)
    except Exception as e:
        print(f"crawl-files: could not read existing rows ({e}) — refusing to walk blind", file=sys.stderr)
        raise SystemExit(2)
    for row in body.get("data", []):
        out[row.get("name", "")] = {"fileSha": row.get("fileSha", ""), "filePath": row.get("filePath", "")}
    return out

def put_row(r, tok):
    req = urllib.request.Request(
        f"{URL}/codefiles/{r['name']}",
        data=json.dumps(r).encode(),
        headers={"Authorization": f"Bearer {tok}", "Content-Type": "application/json"},
        method="PUT")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return resp.status, ""
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:200]
    except Exception as e:
        return 0, str(e)[:200]

def delete_row(name, tok):
    req = urllib.request.Request(
        f"{URL}/codefiles/{name}",
        headers={"Authorization": f"Bearer {tok}"}, method="DELETE")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return resp.status, ""
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:200]
    except Exception as e:
        return 0, str(e)[:200]

def post_batch(rows, tok):
    req = urllib.request.Request(
        f"{URL}/codefiles/batch",
        data=json.dumps(rows).encode(),
        headers={"Authorization": f"Bearer {tok}", "Content-Type": "application/json"},
        method="POST")
    try:
        with urllib.request.urlopen(req, timeout=180) as r:
            return r.status, r.read().decode()[:300]
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:300]
    except Exception as e:
        return 0, str(e)[:300]

def main():
    tok = "" if DRY else token()
    have = {} if DRY else existing(tok)
    sent = failed = skipped = unreadable = seen = 0
    unchanged = replaced = 0
    seen_names = set()
    batch = []
    counts = {}
    for p in walk(ROOT):
        rel = os.path.relpath(p, ROOT)
        r = row(p, rel)
        if r == "skip":
            skipped += 1
            continue
        if not r:
            unreadable += 1
            continue
        counts[r["hasKind"]] = counts.get(r["hasKind"], 0) + 1
        seen += 1
        seen_names.add(r["name"])
        was = have.get(r["name"])
        if was is not None:
            if was.get("fileSha") == r["fileSha"]:
                unchanged += 1
                continue
            code, body = put_row(r, tok)          # changed on disk: replace the row
            if code in (200, 201):
                replaced += 1
            else:
                failed += 1
                print(f"crawl-files: replace {r['filePath']} http={code} {body}", file=sys.stderr)
            continue
        batch.append(r)
        if LIMIT and seen >= LIMIT:
            break
        if len(batch) >= BATCH:
            sent, failed = flush(batch, tok, sent, failed)
            batch = []
    if batch:
        sent, failed = flush(batch, tok, sent, failed)
    # Orphans: a row whose file LEFT DISK. Bounded by a real check, never by
    # "this walk did not see it": a walk scoped to a subtree would otherwise
    # delete every row outside it (measured 2026-09-12 19:50 — a fixture walk
    # queued 5,500 deletes). A row is an orphan only when its own filePath does
    # not exist under the root this walk owns.
    deleted = 0
    if not DRY:
        for name, meta in have.items():
            rel = meta.get("filePath", "")
            if not rel:
                continue
            if name in seen_names:
                continue
            abs_path = os.path.join(ROOT, rel)
            if not abs_path.startswith(os.path.realpath(ROOT) + os.sep) and not abs_path.startswith(ROOT + os.sep):
                continue                      # outside this walk's root: not ours to judge
            if os.path.exists(abs_path):
                continue                      # still on disk (skipped kind, or unreadable): never delete
            code, body = delete_row(name, tok)
            if code in (200, 204):
                deleted += 1
            else:
                failed += 1
                print(f"crawl-files: orphan delete {rel} http={code} {body}", file=sys.stderr)
    print(f"crawl-files: {sent} new, {replaced} replaced, {unchanged} unchanged, {deleted} orphan(s) deleted, {failed} failed; "
          f"kinds={counts}, skipped={skipped} (no kind/language in the model), unreadable={unreadable}")
    return 1 if failed else 0

def flush(batch, tok, sent, failed):
    if DRY:
        print(f"crawl-files: DRY {len(batch)} row(s), first={json.dumps(batch[0])[:160]}")
        return sent + len(batch), failed
    code, body = post_batch(batch, tok)
    if code in (200, 201):
        return sent + len(batch), failed
    print(f"crawl-files: batch FAILED http={code} {body}", file=sys.stderr)
    return sent, failed + len(batch)

if __name__ == "__main__":
    sys.exit(main())
