// @domain: awareness
//! #4744 — the live tree (#4638), as an Operation of the awareness domain: prompt → role
//! (moving/quiet) → turn → card → run. Declared in the model (chorus:operation-tree),
//! compiled into every product that owns or composes awareness by athena-gen; this file is
//! the handler, ported from platform/pulse/src/pulse-tree.ts with the same answers
//! (fixtures/tree-parity-4638.json is that file's own output for one input).
//!
//! A burst of lines from one actor within BURST_GAP_S of each other is ONE prompt (Jeff
//! 2026-10-23 14:56): roles answer the joined prompt, never a fragment. A sender that is
//! not a real actor never makes a prompt; it is counted, never stored as unknown (#4637).

use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};

pub const BURST_GAP_S: i64 = 5;
pub const QUIET_AFTER_S: i64 = 120;
pub const LAST_N_PROMPTS: usize = 5;

/// Milliseconds since the epoch for an ISO time: `Z`, `±HH:MM`, `±HHMM`, or no zone (UTC,
/// as pulse's store writes it, with a space or a `T`). None when it is not a time.
pub fn epoch_ms(t: &str) -> Option<i64> {
    let t = t.trim();
    let b = t.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| t.get(r).and_then(|s| s.parse::<i64>().ok());
    let (y, mo, d, h, mi, s) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    let mut i = 19;
    let mut ms = 0;
    if b.get(i) == Some(&b'.') {
        let start = i + 1;
        i = start;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let frac = &t[start..i];
        ms = format!("{:0<3}", &frac[..frac.len().min(3)]).parse::<i64>().ok()?;
    }
    let offset_min = match &t[i..] {
        "" | "Z" | "z" => 0,
        z => {
            let sign = match z.as_bytes()[0] { b'+' => 1, b'-' => -1, _ => return None };
            let digits: String = z[1..].chars().filter(|c| *c != ':').collect();
            if digits.len() != 4 {
                return None;
            }
            sign * (digits[0..2].parse::<i64>().ok()? * 60 + digits[2..4].parse::<i64>().ok()?)
        }
    };
    // days from the civil date (Howard Hinnant's algorithm)
    let (yy, mm) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + h * 3600 + mi * 60 + s) - offset_min * 60) * 1000 + ms)
}

/// The ISO form pulse-tree.ts writes (`Date.toISOString`): UTC, milliseconds, `Z`.
pub fn iso(ms: i64) -> String {
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000)
}

fn at(v: &Value) -> i64 {
    v.as_str().and_then(epoch_ms).unwrap_or(0)
}

/// `Math.round((a - b) / 1000)`
fn secs(a: i64, b: i64) -> i64 {
    ((a - b) as f64 / 1000.0).round() as i64
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

struct Prompt {
    id: String,
    by: String,
    to: Vec<String>,
    at: String,
    text: String,
    lines: Vec<String>,
}

impl Prompt {
    fn json(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("id".into(), json!(self.id));
        m.insert("by".into(), json!(self.by));
        m.insert("to".into(), json!(self.to));
        m.insert("at".into(), json!(self.at));
        m.insert("text".into(), json!(self.text));
        m.insert("lines".into(), json!(self.lines));
        m
    }
}

/// Join each actor's lines into prompts; lines from non-actors are counted, not kept.
fn join_prompts(lines: &[Value], actors: &BTreeSet<String>) -> (Vec<Prompt>, HashMap<String, usize>, Vec<(String, i64)>) {
    let mut sorted: Vec<&Value> = lines.iter().collect();
    sorted.sort_by_key(|l| at(&l["sentAt"])); // stable, as Array.prototype.sort is
    let mut refused: Vec<(String, i64)> = Vec::new();
    let mut prompts: Vec<Prompt> = Vec::new();
    let mut line_to_prompt = HashMap::new();
    let mut open: HashMap<String, usize> = HashMap::new();
    let mut last_at: HashMap<String, i64> = HashMap::new();
    for l in sorted {
        let from = s(l, "from").to_string();
        if !actors.contains(&from) {
            let who = if from.is_empty() { "(none)".to_string() } else { from };
            match refused.iter_mut().find(|(w, _)| *w == who) {
                Some((_, n)) => *n += 1,
                None => refused.push((who, 1)),
            }
            continue;
        }
        let sent = at(&l["sentAt"]);
        let (id, text, to) = (s(l, "id").to_string(), s(l, "text").to_string(), strs(&l["to"]));
        let joined = match (open.get(&from), last_at.get(&from)) {
            (Some(&p), Some(&prev)) if secs(sent, prev) <= BURST_GAP_S => Some(p),
            _ => None,
        };
        let idx = match joined {
            Some(p) => {
                let cur = &mut prompts[p];
                // pulse stores a line sent to two roles once per role: same words, a role not yet on the prompt
                let copy = cur.text.split('\n').last() == Some(text.as_str()) && to.iter().any(|t| !cur.to.contains(t));
                if !copy {
                    cur.text.push('\n');
                    cur.text.push_str(&text);
                }
                cur.lines.push(id.clone());
                for t in &to {
                    if !cur.to.contains(t) {
                        cur.to.push(t.clone());
                    }
                }
                p
            }
            None => {
                prompts.push(Prompt { id: format!("prompt-{id}"), by: from.clone(), to, at: s(l, "sentAt").to_string(), text, lines: vec![id.clone()] });
                open.insert(from.clone(), prompts.len() - 1);
                prompts.len() - 1
            }
        };
        last_at.insert(from, sent);
        line_to_prompt.insert(id, idx);
    }
    (prompts, line_to_prompt, refused)
}

/// The whole tree for `input` (the TreeInput of pulse-tree.ts, as JSON).
pub fn build_tree(input: &Value) -> Value {
    let actors: BTreeSet<String> = strs(&input["actors"]).into_iter().collect();
    let lines = input["lines"].as_array().cloned().unwrap_or_default();
    let turns = input["turns"].as_array().cloned().unwrap_or_default();
    let runs = input["runs"].as_array().cloned().unwrap_or_default();
    let notes = input["notes"].as_array().cloned().unwrap_or_default();
    let now = s(input, "now").to_string();
    let now_ms = epoch_ms(&now).unwrap_or(0);
    let (prompts, line_to_prompt, refused) = join_prompts(&lines, &actors);
    let prompt_of = |line: Option<&str>| line.and_then(|l| line_to_prompt.get(l)).map(|&i| prompts[i].id.clone());

    let mut notes_by_turn: HashMap<String, Vec<Value>> = HashMap::new();
    for n in &notes {
        notes_by_turn.entry(s(n, "turn").to_string()).or_default().push(n.clone());
    }
    // role names: roles with turns (first-seen order), then roles with only a last call
    let mut role_names: Vec<String> = Vec::new();
    let mut by_role: HashMap<String, Vec<&Value>> = HashMap::new();
    for t in &turns {
        let r = s(t, "role").to_string();
        if !role_names.contains(&r) {
            role_names.push(r.clone());
        }
        by_role.entry(r).or_default().push(t);
    }
    let last_calls: Vec<(String, String)> = input["lastCalls"].as_object()
        .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
        .unwrap_or_default();
    for (r, _) in &last_calls {
        if !role_names.contains(r) {
            role_names.push(r.clone());
        }
    }
    let mut roles: Vec<(String, &'static str, i64)> = role_names.iter().map(|role| {
        let mut calls: Vec<i64> = by_role.get(role).map(|ts| ts.iter().map(|t| at(&t["lastCallAt"])).collect()).unwrap_or_default();
        if let Some((_, c)) = last_calls.iter().find(|(r, _)| r == role) {
            calls.push(epoch_ms(c).unwrap_or(0));
        }
        let last = calls.into_iter().fold(0_i64, |m, c| if c > m { c } else { m });
        let sec = secs(now_ms, last);
        (role.clone(), if sec < QUIET_AFTER_S { "moving" } else { "quiet" }, sec)
    }).collect();
    roles.sort_by(|a, b| a.0.cmp(&b.0));
    let state_of = |r: &str| roles.iter().find(|x| x.0 == r).map(|x| (x.1, x.2));

    let last_n = input["lastN"].as_u64().map(|n| n as usize).unwrap_or(LAST_N_PROMPTS);
    let start = prompts.len().saturating_sub(last_n);
    let mut nodes = Vec::new();
    for p in prompts[start..].iter().rev() {
        let p_turns: Vec<&Value> = turns.iter().filter(|t| prompt_of(t.get("wokenByLine").and_then(|x| x.as_str())).as_deref() == Some(p.id.as_str())).collect();
        let p_runs: Vec<Value> = runs.iter().filter(|r| prompt_of(r.get("startedByLine").and_then(|x| x.as_str())).as_deref() == Some(p.id.as_str())).cloned().collect();
        let mut named: Vec<String> = p.to.iter().filter(|r| state_of(r).is_some()).cloned().collect();
        for t in &p_turns {
            named.push(s(t, "role").to_string());
        }
        let named: Vec<String> = named.into_iter().collect::<BTreeSet<_>>().into_iter().collect();
        let mut role_nodes = Vec::new();
        for role in &named {
            let mut mine: Vec<&Value> = p_turns.iter().filter(|t| s(t, "role") == role).cloned().collect();
            mine.sort_by(|a, b| at(&b["startedAt"]).cmp(&at(&a["startedAt"])));
            let mut card_ids: Vec<i64> = Vec::new();
            for t in &mine {
                if let Some(c) = t.get("card").and_then(|c| c.as_i64()) {
                    if !card_ids.contains(&c) {
                        card_ids.push(c);
                    }
                }
            }
            let cards: Vec<Value> = card_ids.iter().map(|card| {
                let runs: Vec<&Value> = p_runs.iter().filter(|r| r["card"].as_i64() == Some(*card)).collect();
                let notes: Vec<&Value> = mine.iter()
                    .flat_map(|t| notes_by_turn.get(s(t, "id")).map(|n| n.iter().collect::<Vec<_>>()).unwrap_or_default())
                    .filter(|n| n["card"].as_i64() == Some(*card))
                    .collect();
                json!({ "card": card, "runs": runs, "notes": notes })
            }).collect();
            let (state, sec) = state_of(role).unwrap_or(("quiet", -1));
            let turn_nodes: Vec<Value> = mine.iter().map(|t| {
                let mut m = Map::new();
                m.insert("id".into(), t["id"].clone());
                m.insert("startedAt".into(), t["startedAt"].clone());
                if let Some(e) = t.get("endedAt") { m.insert("endedAt".into(), e.clone()); }
                if let Some(c) = t.get("card") { m.insert("card".into(), c.clone()); }
                let n: Vec<&Value> = notes_by_turn.get(s(t, "id")).map(|n| n.iter().filter(|n| n.get("card").is_none()).collect()).unwrap_or_default();
                m.insert("notes".into(), json!(n));
                Value::Object(m)
            }).collect();
            let mut m = Map::new();
            m.insert("role".into(), json!(role));
            m.insert("state".into(), json!(state));
            m.insert("secondsSinceLastCall".into(), json!(sec));
            m.insert("cards".into(), json!(cards));
            m.insert("turns".into(), json!(turn_nodes));
            role_nodes.push(Value::Object(m));
        }
        let mut m = p.json();
        m.insert("roles".into(), json!(role_nodes));
        m.insert("runs".into(), json!(p_runs));
        nodes.push(Value::Object(m));
    }
    let mut refused_obj = Map::new();
    for (k, v) in refused {
        refused_obj.insert(k, json!(v));
    }
    json!({
        "at": now,
        "prompts": nodes,
        "roles": roles.iter().map(|(r, st, sec)| json!({ "role": r, "state": st, "secondsSinceLastCall": sec })).collect::<Vec<_>>(),
        "refusedSenders": refused_obj,
    })
}

/// One role's slice: its state, every role's moving/quiet, and only the prompts it is in.
pub fn slice_for(tree: &Value, role: &str) -> Value {
    let roles = tree["roles"].as_array().cloned().unwrap_or_default();
    let me = roles.iter().find(|r| s(r, "role") == role).cloned();
    let prompts: Vec<Value> = tree["prompts"].as_array().map(|ps| ps.iter().filter_map(|p| {
        let mine = p["roles"].as_array()?.iter().find(|r| s(r, "role") == role)?.clone();
        Some(json!({ "id": p["id"], "by": p["by"], "to": p["to"], "at": p["at"], "text": p["text"], "lines": p["lines"], "mine": mine }))
    }).collect()).unwrap_or_default();
    let mut m = Map::new();
    m.insert("at".into(), tree["at"].clone());
    if let Some(me) = me { m.insert("me".into(), me); }
    m.insert("team".into(), json!(roles));
    m.insert("prompts".into(), json!(prompts));
    Value::Object(m)
}

/// The handler athena-gen routes `/<product>/awareness/tree[/<role>]` to. Live inputs
/// (Jeff's lines from the messages rows, actors, turns and runs from the spine) come in
/// the next slice; until then the operation answers 503 and names what it is missing,
/// never an empty tree that would read as "nobody did anything".
pub fn run(_ctx: &crate::Ctx, _rest: &[&str], _query: &str) -> (u16, Value) {
    (503, json!({ "error": "unmeasured", "message": "tree: live inputs (messages, actors, spine) are not wired yet (#4744 slice 3c)" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARITY: &str = include_str!("fixtures/tree-parity-4638.json");

    #[test]
    fn the_tree_answers_what_pulse_tree_ts_answers_for_the_same_input() {
        let f: Value = serde_json::from_str(PARITY).unwrap();
        assert_eq!(build_tree(&f["input"]), f["tree"]);
    }

    #[test]
    fn each_slice_answers_what_pulse_tree_ts_answers() {
        let f: Value = serde_json::from_str(PARITY).unwrap();
        let tree = build_tree(&f["input"]);
        for role in ["wren", "silas", "nobody"] {
            assert_eq!(slice_for(&tree, role), f["slices"][role], "slice for {role}");
        }
    }

    /// NEGATIVE PROOF: the parity check can fail — one line moved past the burst gap is a
    /// different tree, and the comparison sees it.
    #[test]
    fn negative_proof_a_different_burst_is_a_different_tree() {
        let f: Value = serde_json::from_str(PARITY).unwrap();
        let mut input = f["input"].clone();
        input["lines"][1]["sentAt"] = json!("2026-10-26T09:50:30.000Z");
        assert_ne!(build_tree(&input), f["tree"]);
    }

    #[test]
    fn times_parse_in_every_form_the_inputs_carry() {
        let z = epoch_ms("2026-10-26T13:46:09.141Z").unwrap();
        assert_eq!(epoch_ms("2026-10-26T09:46:09.141-0400"), Some(z));
        assert_eq!(epoch_ms("2026-10-26T09:46:09.141-04:00"), Some(z));
        assert_eq!(epoch_ms("2026-10-26 13:46:09.141"), Some(z));
        assert_eq!(iso(z), "2026-10-26T13:46:09.141Z");
        assert_eq!(epoch_ms("not a time"), None);
    }
}
