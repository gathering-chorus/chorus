#!/usr/bin/env python3
"""
Render the hermeticity audit JSON to HTML. #2523.

Reads /tmp/hermeticity-audit.json (or --in PATH) produced by
hermeticity-audit.py and writes an HTML report to
designing/docs/ci-harness-hermeticity-audit.html.

Card #2523. Plan: /docs/designing/ci-harness-disconnect-plan.html
"""

import argparse
import html
import json
import os
import sys
from collections import Counter
from pathlib import Path


RULES_DOC = """
<h2>Hermeticity rules</h2>
<p>A test in the kept set is <strong>hermetic</strong> when every rule below holds.
Each rule is a property the test must satisfy in isolation — if it can't, it
either gets fixed, moved to <code>*.integration.test.ts</code> /
<code>*.contract.test.ts</code> / <code>*.smoke.test.ts</code>, or quarantined.</p>
<table class="rules">
<tr><th>#</th><th>Rule</th><th>What it forbids</th><th>How verified</th></tr>
<tr><td>1</td><td>No network</td><td>Outbound HTTP / WebSocket / TCP / DNS from test code, including <code>localhost:*</code></td><td>Source-grep + integration-test convention</td></tr>
<tr><td>2</td><td>No fs outside tmp</td><td>File writes outside <code>/tmp</code> or per-test tmpdir</td><td>Source-grep + manual review for "maybe" cases</td></tr>
<tr><td>3</td><td>No clock or random</td><td>Unmocked <code>Date.now</code>, <code>new Date()</code>, <code>Math.random</code>, <code>performance.now</code>, <code>SystemTime::now</code></td><td>Source-grep — mocked timers downgrade to "review"</td></tr>
<tr><td>4</td><td>No env coupling</td><td>Reads of <code>process.env.*</code> / <code>std::env::var</code> without explicit per-test setup</td><td>Source-grep — known harness env (<code>CHORUS_ROOT</code>) downgrades to "review"</td></tr>
<tr><td>5</td><td>Order-independent</td><td>Test outcome depends on previous test's state</td><td>CI runs <code>jest --randomize</code> on every PR (#2532) — not source-detectable</td></tr>
</table>
"""


def render(payload: dict) -> str:
    summary = payload["summary"]
    results = payload["results"]
    by_kind = summary.get("by_kind", {})
    by_verdict = summary.get("by_verdict", {})

    # Per-rule fail counts
    rule_fails = Counter()
    for r in results:
        for rule, info in r.get("rules", {}).items():
            if info.get("status") == "fail":
                rule_fails[rule] += 1

    # Group results by package for the table
    by_dir = {}
    for r in results:
        top = "/".join(r["path"].split("/")[:3])
        by_dir.setdefault(top, []).append(r)

    rows = []
    for top in sorted(by_dir.keys()):
        for r in sorted(by_dir[top], key=lambda x: x["path"]):
            verdict = r.get("verdict", "?")
            rules = r.get("rules", {})
            failed_rules = [k for k, v in rules.items() if v.get("status") == "fail"]
            maybe_rules = [k for k, v in rules.items() if v.get("status") == "maybe"]
            tags = ", ".join(failed_rules + [f"{m}?" for m in maybe_rules])
            badge_class = {
                "hermetic": "ok",
                "non-hermetic": "bad",
                "review": "warn",
            }.get(verdict, "")
            evidence = []
            for k in failed_rules + maybe_rules:
                hits = rules.get(k, {}).get("hits", [])
                if hits:
                    evidence.append(f"{html.escape(k)}: <code>{html.escape(', '.join(hits[:3]))}</code>")
            evidence_html = "<br>".join(evidence)
            rows.append(
                f'<tr class="{badge_class}"><td>{html.escape(r["kind"])}</td>'
                f'<td><code>{html.escape(r["path"])}</code></td>'
                f'<td><span class="badge {badge_class}">{html.escape(verdict)}</span></td>'
                f'<td>{html.escape(tags)}</td>'
                f'<td>{evidence_html}</td></tr>'
            )

    html_doc = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>CI Harness Hermeticity Audit — #2523</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: Georgia, serif; margin: 40px 1in; color: #1a1a1a; line-height: 1.55; }}
  h1 {{ font-size: 1.6em; border-bottom: 2px solid #333; padding-bottom: 8px; margin-bottom: 4px; }}
  h1 span {{ color: #2563eb; font-weight: normal; font-size: 0.65em; }}
  .date {{ color: #999; font-size: 0.82em; margin-bottom: 16px; }}
  h2 {{ font-size: 1.2em; margin-top: 2em; color: #444; border-bottom: 1px solid #ddd; padding-bottom: 4px; }}
  h3 {{ font-size: 1em; margin-top: 1.2em; color: #2d4a7a; }}
  p {{ margin-bottom: 10px; font-size: 0.88em; }}
  table {{ border-collapse: collapse; width: 100%; margin: 10px 0; font-size: 0.82em; }}
  th {{ text-align: left; padding: 6px 10px; background: #e0e7ff; color: #2d4a7a; font-size: 0.76em; text-transform: uppercase; }}
  td {{ padding: 6px 10px; border-bottom: 1px solid #e5e7eb; vertical-align: top; }}
  code {{ background: #f4f4f4; padding: 1px 5px; border-radius: 3px; font-size: 0.88em; word-break: break-all; }}
  .summary-grid {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin: 12px 0; }}
  .stat {{ background: #f9fafb; border: 1px solid #e5e7eb; border-radius: 4px; padding: 10px 14px; }}
  .stat .num {{ font-size: 1.6em; font-weight: bold; }}
  .stat .lbl {{ font-size: 0.78em; color: #6b7280; text-transform: uppercase; letter-spacing: 0.5px; }}
  .badge {{ display: inline-block; padding: 1px 8px; border-radius: 10px; font-size: 0.74em; font-weight: bold; }}
  .badge.ok {{ background: #d1fae5; color: #064e3b; }}
  .badge.bad {{ background: #fee2e2; color: #7f1d1d; }}
  .badge.warn {{ background: #fef3c7; color: #92400e; }}
  tr.bad td {{ background: #fffafa; }}
  tr.warn td {{ background: #fffdf6; }}
  .ac {{ background: #ecfccb; padding: 12px 16px; margin: 14px 0; border-radius: 4px; border-left: 4px solid #65a30d; font-size: 0.88em; }}
  .ac strong {{ color: #3f6212; }}
  .footer {{ color: #999; font-size: 0.72em; margin-top: 3em; border-top: 1px solid #ddd; padding-top: 8px; }}
</style>
</head>
<body>
<h1>CI Harness Hermeticity Audit <span>#2523</span></h1>
<p class="date">Generated {payload.get("generated_at","").split("T")[0]} · Phase 0 exit evidence for the <a href="/docs/designing/ci-harness-disconnect-plan.html">CI harness disconnect plan</a></p>

<div style="background:#d1fae5;border-left:4px solid #059669;padding:12px 16px;margin:14px 0;border-radius:4px;font-size:0.88em;">
<strong>Scanner v2 — gaps closed.</strong> Per Kade review (chat <code>silas-kade-1777390393</code>, 2026-04-28), the scanner now discovers the kept-set (no hardcoded paths), detects shellouts via <code>child_process</code>, dynamic imports, DB clients (<code>pg/ioredis/kafkajs/mongodb/mongoose/mysql/redis/prisma</code>), Rust HTTP+socket (<code>reqwest/ureq/hyper/std::net/tokio::net/async_std::net</code>), bats shell-env coupling (<code>$VAR</code>), and Python network/clock/env. Mock-hints tightened to require explicit <code>beforeEach/afterEach</code> or <code>monkeypatch</code> patterns. <code>localhost:*</code> hits reclassified to "review" (in-process spawn vs daemon-hit ambiguity is not lexically resolvable). Pytest added to discovery. Pending wave 2 close: Kade's reconciliation diff between this scanner's <code>kept_set()</code> output and his <code>discover-tests</code> file list.
</div>

<p>This report classifies every test in the <strong>kept set</strong> — the tests that will be required-checks on <code>main</code> per the <a href="#">#2525</a> DEC — against the five hermeticity rules. The audit is the gate that lets Phase 1 (rules disconnect) ship without re-creating the bypass-and-flake failure mode the disconnect is meant to retire.</p>

<h2>Summary</h2>
<div class="summary-grid">
  <div class="stat"><div class="num">{summary["total"]}</div><div class="lbl">Total tests in kept set</div></div>
  <div class="stat"><div class="num">{by_verdict.get("hermetic",0)}</div><div class="lbl">Hermetic</div></div>
  <div class="stat"><div class="num">{by_verdict.get("non-hermetic",0)}</div><div class="lbl">Non-hermetic</div></div>
  <div class="stat"><div class="num">{by_verdict.get("review",0)}</div><div class="lbl">Needs review (maybe)</div></div>
  <div class="stat"><div class="num">{by_kind.get("jest",0)}</div><div class="lbl">jest</div></div>
  <div class="stat"><div class="num">{by_kind.get("bats",0)}</div><div class="lbl">bats</div></div>
  <div class="stat"><div class="num">{by_kind.get("cargo-external",0)+by_kind.get("cargo-inline",0)}</div><div class="lbl">cargo</div></div>
</div>

<h3>Failures by rule</h3>
<table>
<tr><th>Rule</th><th>Failing tests</th></tr>
{''.join(f'<tr><td>{html.escape(k)}</td><td>{v}</td></tr>' for k,v in rule_fails.most_common())}
</table>

{RULES_DOC}

<h2>Remediation buckets (for non-hermetic)</h2>
<p>Each non-hermetic test gets one of three remediations. Per-test routing is in the table below; this section names the buckets and the rationale.</p>
<ul>
  <li><strong>Fix in place</strong> — test logically belongs in the kept set; mock the dependency. Examples: replace <code>Date.now()</code> with <code>jest.useFakeTimers()</code>; replace <code>fetch()</code> with a stub; wrap <code>process.env</code> reads in a <code>beforeEach</code> setup.</li>
  <li><strong>Rename to integration</strong> — test exercises real cross-component behavior and needs a live dependency. Rename <code>foo.test.ts</code> → <code>foo.integration.test.ts</code>, removing it from the hermetic kept set per the #2524 convention.</li>
  <li><strong>Quarantine</strong> — test is non-hermetic and not yet feasible to fix or split. Move under a quarantine directory or tag, file a follow-on card. Quarantine is bounded by TTL per #2530.</li>
</ul>

<h2>AC status</h2>
<div class="ac">
<strong>AC1</strong> — Hermeticity rules documented: <span class="badge ok">done</span> (above).<br>
<strong>AC2</strong> — Audit report lists every test with status: <span class="badge ok">done</span> ({summary["total"]} tests, table below).<br>
<strong>AC3</strong> — Each non-hermetic test has remediation plan: <span class="badge warn">in progress</span> (bucket framework above; per-test routing pending manual review of {by_verdict.get("non-hermetic",0)} non-hermetic + {by_verdict.get("review",0)} review).<br>
<strong>AC4</strong> — Kept set passes 100/100 shuffled CI runs over 48h: <span class="badge warn">deferred</span> (window required; mechanism shipped in #2532).<br>
<strong>AC5</strong> — Audit linked from plan doc: <span class="badge warn">pending</span> (will be added once non-hermetic remediation completes).
</div>

<h2>Per-test results</h2>
<p>Sorted by package, then path. Click a row to inspect the source. The "Evidence" column shows the first three pattern hits per failing rule.</p>
<table>
<tr><th>Kind</th><th>Path</th><th>Verdict</th><th>Failing rules</th><th>Evidence</th></tr>
{''.join(rows)}
</table>

<div class="footer">
Card #2523. Source: <code>platform/scripts/hermeticity-audit.py</code>. Renderer: <code>platform/scripts/hermeticity-audit-render.py</code>.
JSON: <code>/tmp/hermeticity-audit.json</code>.
</div>
</body>
</html>
"""
    return html_doc


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--in", dest="inp", default="/tmp/hermeticity-audit.json")
    parser.add_argument("--out", default=None)
    parser.add_argument("--root", default=os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus"))
    args = parser.parse_args()

    payload = json.loads(Path(args.inp).read_text())
    out_path = args.out or f"{args.root}/designing/docs/ci-harness-hermeticity-audit.html"
    Path(out_path).write_text(render(payload))
    print(f"wrote audit report to {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
