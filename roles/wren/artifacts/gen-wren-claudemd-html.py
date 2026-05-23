#!/usr/bin/env python3
"""Generate the Wren CLAUDE.md shared-vs-local visualization with per-tile folds.

Reads the live manifest (designing/claudemd/manifest.json) + fragment files so the
output stays accurate — no hardcoded fragment bodies. Re-runnable and deterministic:
running it against the current manifest reproduces wren-claudemd-annotated.html.

Paths resolve relative to this script's location, so it works from canonical or any werk.
Usage:  python3 roles/wren/artifacts/gen-wren-claudemd-html.py
"""
import html, pathlib, datetime

SCRIPT_DIR = pathlib.Path(__file__).resolve().parent          # roles/wren/artifacts
REPO_ROOT  = SCRIPT_DIR.parents[2]                            # repo root
ROOT       = REPO_ROOT / "designing" / "claudemd"            # fragment source tree
OUT        = SCRIPT_DIR / "wren-claudemd-annotated.html"

# Concern grouping + heading per fragment. cls is computed from the manifest below,
# not hardcoded here — this table only carries presentation (group + display heading).
GROUP = {
    "roles/wren/title.md":                       ("Identity",                          "Product Manager Role — Wren"),
    "shared/chorus-prompt.md":                   ("Protocol header",                   "Chorus Prompt (MANDATORY)"),
    "roles/wren/principles.md":                  ("Identity",                          "Core Principles"),
    "roles/wren/how-you-operate.md":             ("How Wren works",                    "How You Operate"),
    "roles/wren/portfolio.md":                   ("State & scope",                     "Project Portfolio"),
    "shared/infrastructure-operations-core.md":  ("Infrastructure, gates & reference", "Infrastructure Operations (MANDATORY)"),
    "shared/cross-machine-operations-core.md":   ("Infrastructure, gates & reference", "Cross-Machine Operations (ADR-012)"),
    "roles/wren/working-with-jeff.md":           ("How Wren works",                    "Working with Jeff"),
    "roles/wren/tone.md":                        ("Identity",                          "Tone"),
    "shared/communication-discipline.md":        ("Discipline",                        "Brevity Rules"),
    "shared/idle-awareness.md":                  ("Discipline",                        "Idle Awareness (MANDATORY)"),
    "roles/wren/session-moderation.md":          ("How Wren works",                    "Session Moderation & Interaction Patterns"),
    "shared/team-kanban-board-core.md":          ("Operating model & coordination",    "Team Kanban Board"),
    "shared/team-operating-model.md":            ("Operating model & coordination",    "Team Operating Model"),
    "shared/execution-modes.md":                 ("Operating model & coordination",    "Execution Modes (DEC-058)"),
    "shared/intellectual-honesty.md":            ("Discipline",                        "Intellectual Honesty (DEC-069)"),
    "shared/search-hierarchy.md":                ("Operating model & coordination",    "Search Hierarchy (DEC-074)"),
    "shared/domain-endpoints.md":                ("Infrastructure, gates & reference", "Domain Endpoints (DEC-093)"),
    "shared/session-close-out-core.md":          ("Operating model & coordination",    "Session Close-Out (MANDATORY)"),
    "roles/wren/close-out-docs.md":              ("Close-out",                         "Wren Domain Docs (If-Touched)"),
    "shared/error-handling-discipline.md":       ("Discipline",                        "Error Handling Discipline"),
    "shared/tdd-discipline.md":                  ("Discipline",                        "TDD Discipline (DEC-1674)"),
    "shared/icd-gate.md":                        ("Infrastructure, gates & reference", "ICD Gate (DEC-095)"),
    "shared/worktree-convention.md":             ("Infrastructure, gates & reference", "Per-Role Worktree Convention (RETIRED #2640)"),
    "roles/wren/state-files.md":                 ("State & scope",                     "State Files"),
}


def load_manifest():
    import json
    m = json.loads((ROOT / "manifest.json").read_text())
    sections = m["roles"]["wren"]["sections"]
    core = set(m.get("protocol_core", []))
    rows = []
    for i, path in enumerate(sections, 1):
        if path.startswith("roles/wren/"):
            cls = "local"
        elif path in core:
            cls = "core"
        else:
            cls = "shared"
        group, head = GROUP.get(path, ("Ungrouped", pathlib.Path(path).stem))
        rows.append((i, path, cls, group, head))
    return rows


TAGTXT = {"core": "CORE", "shared": "SHARED", "local": "WREN"}


def fold(order, path, cls, head):
    body = (ROOT / path).read_text()
    nlines = body.count("\n") + 1
    return f'''<details class="frag {cls}">
  <summary>
    <span class="ord">{order}</span>
    <span class="title"><span class="head">{html.escape(head)}</span><span class="path">{html.escape(path)} · {nlines} lines</span></span>
    <span class="tags"><span class="tag {cls}">{TAGTXT[cls]}</span></span>
  </summary>
  <pre class="md">{html.escape(body)}</pre>
</details>'''


def main():
    rows = load_manifest()
    n_core   = sum(1 for r in rows if r[2] == "core")
    n_shared = sum(1 for r in rows if r[2] == "shared")
    n_local  = sum(1 for r in rows if r[2] == "local")

    chips = "\n".join(
        f'<span class="chip {cls}"><span class="ord">{o}</span><span class="dot"></span>{pathlib.Path(p).stem}</span>'
        for (o, p, cls, _g, _h) in rows
    )

    SHARED_GROUPS = ["Protocol header", "Operating model & coordination", "Discipline", "Infrastructure, gates & reference"]
    WREN_GROUPS   = ["Identity", "How Wren works", "State & scope", "Close-out"]

    def column(groups, want_local):
        out = []
        for g in groups:
            items = [r for r in rows if r[3] == g and ((r[2] == "local") == want_local)]
            if not items:
                continue
            out.append(f'<div class="subhead">{html.escape(g)}</div>')
            for (o, p, cls, _g, h) in items:
                out.append(fold(o, p, cls, h))
        return "\n".join(out)

    shared_col = column(SHARED_GROUPS, want_local=False)
    wren_col   = column(WREN_GROUPS,   want_local=True)
    today = datetime.date.today().isoformat()

    doc = f'''<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<title>Wren CLAUDE.md — shared vs local (live, folds)</title>
<style>
  :root {{ --shared:#3b82f6; --shared-bg:#eef5ff; --core:#7c3aed; --core-bg:#f3eaff;
           --local:#16a34a; --local-bg:#ecfdf3; --ink:#1a1a1a; --muted:#6b7280; --line:#e5e7eb; }}
  * {{ box-sizing:border-box; }}
  body {{ font:15px/1.55 -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif;
         color:var(--ink); max-width:1080px; margin:0 auto; padding:32px 24px 80px; background:#fafafa; }}
  h1 {{ font-size:26px; margin:0 0 4px; }}
  .sub {{ color:var(--muted); margin:0 0 24px; font-size:13px; }}
  code {{ font-family:ui-monospace,"SF Mono",Menlo,monospace; font-size:.92em; }}
  .legend {{ display:flex; flex-wrap:wrap; gap:18px; align-items:center; background:#fff;
            border:1px solid var(--line); border-radius:8px; padding:14px 18px; margin-bottom:18px; font-size:13px; }}
  .controls {{ margin-bottom:24px; font-size:13px; }}
  .controls button {{ font:inherit; font-size:12px; padding:5px 12px; margin-right:8px; border:1px solid var(--line);
                     background:#fff; border-radius:6px; cursor:pointer; }}
  .controls button:hover {{ background:#f0f0f0; }}
  .tag {{ display:inline-block; padding:2px 9px; border-radius:5px; font-size:11px; font-weight:700;
         letter-spacing:.3px; color:#fff; vertical-align:middle; }}
  .tag.shared {{ background:var(--shared); }} .tag.core {{ background:var(--core); }} .tag.local {{ background:var(--local); }}
  h2.section {{ font-size:18px; margin:36px 0 14px; padding-bottom:6px; border-bottom:2px solid var(--line); }}
  .flow {{ display:flex; flex-wrap:wrap; gap:6px; margin-bottom:12px; }}
  .chip {{ display:flex; align-items:center; gap:6px; border-radius:6px; padding:6px 10px; font-size:12.5px;
          border:1px solid; background:#fff; white-space:nowrap; }}
  .chip .ord {{ font-weight:700; color:var(--muted); font-size:11px; }}
  .chip.shared {{ border-color:var(--shared); background:var(--shared-bg); }}
  .chip.core {{ border-color:var(--core); background:var(--core-bg); }}
  .chip.local {{ border-color:var(--local); background:var(--local-bg); }}
  .chip .dot {{ width:7px; height:7px; border-radius:50%; }}
  .chip.shared .dot {{ background:var(--shared); }} .chip.core .dot {{ background:var(--core); }} .chip.local .dot {{ background:var(--local); }}
  .group h3 {{ font-size:14px; margin:0 0 10px; text-transform:uppercase; letter-spacing:.6px; }}
  .subhead {{ font-size:12px; color:var(--muted); margin:14px 0 6px; font-weight:700; }}
  details.frag {{ background:#fff; border:1px solid var(--line); border-left-width:4px; border-radius:7px;
                 margin-bottom:8px; overflow:hidden; }}
  details.frag.shared {{ border-left-color:var(--shared); }}
  details.frag.core {{ border-left-color:var(--core); }}
  details.frag.local {{ border-left-color:var(--local); }}
  summary {{ list-style:none; cursor:pointer; padding:11px 14px; display:grid;
            grid-template-columns:30px 1fr auto; gap:12px; align-items:baseline; position:relative; }}
  summary::-webkit-details-marker {{ display:none; }}
  summary::after {{ content:"\\25B8"; color:var(--muted); position:absolute; right:14px; top:12px; font-size:11px; }}
  details[open] summary::after {{ content:"\\25BE"; }}
  summary .ord {{ color:var(--muted); font-weight:700; font-size:12px; }}
  summary .head {{ font-weight:600; }}
  summary .path {{ display:block; color:var(--muted); font-size:11.5px;
                  font-family:ui-monospace,"SF Mono",Menlo,monospace; margin-top:2px; }}
  pre.md {{ margin:0; padding:14px 16px 16px; border-top:1px solid var(--line); background:#fbfbfd;
           font:12.5px/1.5 ui-monospace,"SF Mono",Menlo,monospace; white-space:pre-wrap; word-wrap:break-word; color:#333; }}
  .col2 {{ display:grid; grid-template-columns:1fr 1fr; gap:26px; align-items:start; }}
  @media (max-width:760px) {{ .col2 {{ grid-template-columns:1fr; }} }}
  .note {{ color:var(--muted); font-size:12.5px; background:#fff; border:1px solid var(--line);
          border-radius:8px; padding:12px 16px; margin-top:8px; }}
</style></head><body>

<h1>Wren CLAUDE.md — shared vs local</h1>
<p class="sub">Live against <code>designing/claudemd/manifest.json</code> · regenerated {today} ·
{len(rows)} fragments assembled in order by <code>claudemd-gen.py</code> → <code>roles/wren/CLAUDE.md</code>.
Click any tile to fold open its source content.</p>

<div class="legend">
  <span><span class="tag core">CORE</span> protocol-core — {n_core} · identical across all 3 roles, drives the version-bump hash</span>
  <span><span class="tag shared">SHARED</span> shared, non-core — {n_shared} · same text for roles, outside the core hash</span>
  <span><span class="tag local">WREN</span> Wren-only — {n_local}</span>
</div>
<div class="controls">
  <button onclick="document.querySelectorAll('details.frag').forEach(d=>d.open=true)">Expand all</button>
  <button onclick="document.querySelectorAll('details.frag').forEach(d=>d.open=false)">Collapse all</button>
</div>

<h2 class="section">Assembly flow — concatenation order (1 → {len(rows)})</h2>
<p class="sub" style="margin-top:-6px">The literal sequence the generator writes. Grouping below is by concern, not order.</p>
<div class="flow">
{chips}
</div>

<h2 class="section">Grouped by concern — click to fold open</h2>
<div class="col2">
  <div><div class="group"><h3 style="color:var(--shared)">Shared — {n_core + n_shared} ({n_core} core)</h3>
{shared_col}
  </div></div>
  <div><div class="group"><h3 style="color:var(--local)">Wren-only — {n_local}</h3>
{wren_col}
  </div>
  <div class="note">
    <strong>Why a generator, not a snapshot.</strong> The prior hand-built version drifted
    (read 14/10; missing <code>worktree-convention.md</code>; <code>infrastructure-operations-core.md</code>
    mislabeled Wren-only). This page is generated from the live manifest + fragments, so the counts and
    content can't go stale — re-run <code>roles/wren/artifacts/gen-wren-claudemd-html.py</code>.<br><br>
    <strong>CORE vs SHARED.</strong> All {n_core + n_shared} shared fragments are identical text across
    roles, but only the {n_core} <span class="tag core" style="font-size:10px">CORE</span> ones feed the
    <code>protocol-core</code> hash — editing one auto-bumps every role's <code>chorus-prompt</code> version.
    The {n_shared} plain <span class="tag shared" style="font-size:10px">SHARED</span> can drift without a bump.
  </div>
  </div>
</div>
</body></html>'''

    OUT.write_text(doc)
    print(f"wrote {OUT} ({len(doc)} bytes) — {len(rows)} folds: {n_core} core / {n_shared} shared / {n_local} wren")


if __name__ == "__main__":
    main()
