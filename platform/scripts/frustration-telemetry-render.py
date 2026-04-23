#!/usr/bin/env python3
"""frustration-telemetry-render.py — #2454

Renders a standalone HTML page with three charts (frustration, relief,
precursor) from the JSON output of frustration-telemetry.sh.

Usage:
  frustration-telemetry.sh --json --days 30 | frustration-telemetry-render.py > /tmp/frustration.html
  open /tmp/frustration.html
"""
import json, sys
from datetime import date, timedelta

env = json.load(sys.stdin)
window = env['window_days']
since = env['since']
generated = env['generated_at']
data = env['data']

# Build dense per-day series (fill zeros for missing days)
today = date.today()
days = [(today - timedelta(days=i)).isoformat() for i in range(window, -1, -1)]

def series_for(cat_data):
    by_day = cat_data['by_day']
    return [sum(by_day.get(d, {}).values()) for d in days]

frust = series_for(data['frustration'])
relief = series_for(data['relief'])
precursor = series_for(data['precursor'])

frust_total = data['frustration']['total']
relief_total = data['relief']['total']
precursor_total = data['precursor']['total']

# Team-learning overlay: memory writes per day + top-bad-days narrative
mem_writes = env.get('memory_writes', {})
mem_samples = env.get('memory_samples', {})
memory_series = [mem_writes.get(d, 0) for d in days]
memory_total = sum(memory_series)

narrative = []
for i, d in enumerate(days):
    fc = frust[i]
    if fc == 0: continue
    follow_mem = sum(mem_writes.get(days[j], 0) for j in range(i, min(i+3, len(days))))
    sample_names = []
    for j in range(i, min(i+3, len(days))):
        sample_names.extend(mem_samples.get(days[j], []))
    narrative.append({'day': d, 'frustration': fc, 'memory_writes_48h': follow_mem, 'sample_memories': sample_names[:6]})
narrative.sort(key=lambda x: -x['frustration'])
narrative_top = narrative[:5]

payload = {
    'days': days,
    'frustration': frust,
    'relief': relief,
    'precursor': precursor,
    'memory_writes': memory_series,
}

html = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Frustration Telemetry — #2454</title>
<style>
  body {{ font-family: -apple-system, sans-serif; max-width: 1100px; margin: 2em auto; padding: 0 1em; color: #222; }}
  h1 {{ margin-bottom: .1em; }}
  .sub {{ color: #666; margin: 0 0 2em 0; font-size: .9em; }}
  .panel {{ margin: 1.5em 0 2.5em; }}
  .panel h2 {{ margin: 0 0 .2em; font-size: 1.1em; }}
  .totals {{ color: #666; font-size: .88em; margin-bottom: .4em; }}
  .zero {{ color: #b00; font-weight: bold; }}
  canvas {{ display: block; width: 100%; height: 240px; background: #fafafa; border: 1px solid #eee; }}
  footer {{ color: #999; font-size: .78em; margin-top: 3em; border-top: 1px solid #eee; padding-top: 1em; }}
</style>
</head>
<body>
<h1>Frustration Telemetry</h1>
<p class="sub">Card #2454 — bidirectional regex signal over Chorus index. Window: {window} days (since {since}). Generated {generated}.</p>

<div class="panel">
  <h2>Jeff → role frustration</h2>
  <div class="totals">Total: {frust_total} {'' if frust_total else '<span class="zero">(0 detected — honest-fold)</span>'}</div>
  <canvas id="c1"></canvas>
</div>
<div class="panel">
  <h2>Jeff → role relief</h2>
  <div class="totals">Total: {relief_total} {'' if relief_total else '<span class="zero">(0 detected — honest-fold)</span>'}</div>
  <canvas id="c2"></canvas>
</div>
<div class="panel">
  <h2>Role → Jeff precursor (experimental)</h2>
  <div class="totals">Total: {precursor_total} {'' if precursor_total else '<span class="zero">(0 detected — honest-fold)</span>'} — vocabulary-only; LLM labeling needed for production</div>
  <canvas id="c3"></canvas>
</div>
<div class="panel">
  <h2>Team learning — memory writes per day</h2>
  <div class="totals">Total: {memory_total} {'' if memory_total else '<span class="zero">(0 detected — honest-fold)</span>'} — feedback + project memory files across wren/silas/kade stores (write-rate, not apply-rate)</div>
  <canvas id="c4"></canvas>
</div>
<div class="panel">
  <h2>Top bad days — did we learn?</h2>
  <div class="totals">For each red spike, what memory files landed within 48h. No memories = hit absorbed, not processed.</div>
  <table style="border-collapse:collapse; width:100%; font-size:.88em;">
    <thead><tr style="border-bottom:1px solid #ccc; text-align:left;"><th style="padding:.3em .5em;">Day</th><th style="padding:.3em;">Frustration</th><th style="padding:.3em;">Memory writes 48h</th><th style="padding:.3em;">Sample memory names</th></tr></thead>
    <tbody>
    {''.join(f'<tr style="border-bottom:1px solid #eee;"><td style="padding:.3em .5em; color:#c0392b; font-weight:bold;">{r["day"]}</td><td style="padding:.3em;">{r["frustration"]}</td><td style="padding:.3em;">{r["memory_writes_48h"]}</td><td style="padding:.3em; color:#555;">{", ".join(r["sample_memories"]) if r["sample_memories"] else "<em>(silent — hit not processed)</em>"}</td></tr>' for r in narrative_top)}
    </tbody>
  </table>
</div>

<footer>
  Zero behavior change: log only. No apology change, no model fine-tune. DEC-022 / #2454.
</footer>

<script>
const PAYLOAD = {json.dumps(payload)};

function draw(canvas_id, series, color) {{
  const c = document.getElementById(canvas_id);
  const rect = c.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = rect.width || 900, h = rect.height || 240;
  c.width = w * dpr; c.height = h * dpr;
  const ctx = c.getContext('2d');
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, w, h);

  const pad = {{left: 40, right: 10, top: 10, bottom: 22}};
  const max = Math.max(1, ...series);
  const n = series.length;
  const plotW = w - pad.left - pad.right;
  const plotH = h - pad.top - pad.bottom;

  // axis
  ctx.strokeStyle = '#ddd';
  ctx.beginPath(); ctx.moveTo(pad.left, pad.top); ctx.lineTo(pad.left, h - pad.bottom); ctx.lineTo(w - pad.right, h - pad.bottom); ctx.stroke();
  // y ticks
  ctx.fillStyle = '#888'; ctx.font = '10px -apple-system, sans-serif';
  for (let i = 0; i <= 4; i++) {{
    const y = pad.top + plotH * (1 - i/4);
    const v = Math.round(max * i / 4);
    ctx.fillText(String(v), 4, y + 3);
    ctx.strokeStyle = '#f2f2f2';
    ctx.beginPath(); ctx.moveTo(pad.left, y); ctx.lineTo(w - pad.right, y); ctx.stroke();
  }}
  // x labels — every 5th day
  for (let i = 0; i < n; i += 5) {{
    const x = pad.left + (plotW * i / (n - 1));
    const d = PAYLOAD.days[i].slice(5);
    ctx.fillText(d, x - 10, h - 6);
  }}
  // bars
  ctx.fillStyle = color;
  const bw = Math.max(1, plotW / n - 1);
  for (let i = 0; i < n; i++) {{
    const x = pad.left + (plotW * i / (n - 1)) - bw/2;
    const bh = (series[i] / max) * plotH;
    ctx.fillRect(x, h - pad.bottom - bh, bw, bh);
  }}
}}

function drawAll() {{
  try {{
    draw('c1', PAYLOAD.frustration,   '#c0392b');
    draw('c2', PAYLOAD.relief,        '#27ae60');
    draw('c3', PAYLOAD.precursor,     '#8e44ad');
    draw('c4', PAYLOAD.memory_writes, '#2980b9');
  }} catch (e) {{
    const err = document.createElement('pre');
    err.style.color = 'red';
    err.textContent = 'Chart error: ' + (e.message || e);
    document.body.appendChild(err);
  }}
}}
// Fire in three ways to cover whichever resolves first
if (document.readyState === 'complete') drawAll();
else window.addEventListener('load', drawAll);
window.addEventListener('resize', drawAll);
// Belt-and-suspenders: re-fire after a tick so layout has settled
setTimeout(drawAll, 100);
</script>
</body>
</html>
"""
sys.stdout.write(html)
