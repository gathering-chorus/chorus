//! #4340 — messages in the model: the rows the projector writes from messages.db.
use chorus_awake::msgs::*;
use serde_json::json;

fn src(kind: &str, from: &str, to: &str, content: &str, status: &str) -> Src {
    Src { id: 40158, kind: kind.into(), from: from.into(), to: to.into(), content: content.into(),
          created_at: "2026-09-26 17:16:35".into(), status: status.into(), delivered_at: "2026-09-26 17:16:36".into(), error: String::new() }
}
fn principals() -> Vec<String> { ["jeff", "kade", "silas", "wren", "bridge"].iter().map(|s| s.to_string()).collect() }

#[test]
fn a_peer_message_names_its_sender_and_recipient_as_principals() {
    let m = message_row(&src("nudge", "wren", "principal-silas", "hi", "delivered"), &principals(), Some("wren-0tl9e4s2"));
    assert_eq!(m["sentBy"], "principal-wren");
    assert_eq!(m["sentTo"], "principal-silas", "messages.db writes both kade and principal-kade");
    assert_eq!(m["senderName"], "wren");
    assert_eq!(m["sentInSession"], "wren-0tl9e4s2");
    assert_eq!(m["sentAt"], "2026-09-26T17:16:35Z");
    assert_eq!(m["name"], "src-40158", "the bare name; the service adds the kind prefix");
}

#[test]
fn a_machine_sender_is_provenance_not_identity() {
    let m = message_row(&src("nudge", "system", "silas", "chorus-health: fuseki-memory", "delivered"), &principals(), None);
    assert!(m.get("sentBy").is_none(), "system has no principal yet (Wren 13:16)");
    assert_eq!(m["senderName"], "system");
}

#[test]
fn the_channel_follows_the_kind() {
    assert_eq!(channel_for("nudge"), "nudge");
    assert_eq!(channel_for("jeff-input"), "terminal");
    assert_eq!(channel_for("chat"), "clearing");
}

#[test]
fn a_message_that_arrived_shorter_than_sent_is_truncated_not_delivered() {
    // 09-26 12:45: Wren's answer reached Silas's pane as its last line only; pulse said delivered
    let full = "[nudge from wren | 12:45] My answers.\n1 Channel = transport\n5 Buzz is then: add one Channel row";
    let s = src("nudge", "wren", "silas", full, "delivered");
    assert_eq!(outcome(&s, &["5 Buzz is then: add one Channel row".into()]), "truncated");
    // negative proof: the whole text arriving is delivered
    assert_eq!(outcome(&s, &[full.into()]), "delivered");
    // and an unrelated prompt proves nothing
    assert_eq!(outcome(&s, &["work status".into()]), "delivered");
}

#[test]
fn pulse_outcomes_pass_through_and_nothing_else_does() {
    assert_eq!(outcome(&src("nudge", "a", "b", "x", "failed"), &[]), "failed");
    assert_eq!(outcome(&src("nudge", "a", "b", "x", "queued"), &[]), "queued");
    assert_eq!(outcome(&src("nudge", "a", "b", "x", "weird"), &[]), "pending");
}

#[test]
fn the_delivery_lands_on_the_presence_that_was_live_then() {
    let runs = json!({"data":[
        {"name":"silas-run-a","ownedBy":"principal-silas","startedAt":"2026-09-26T13:30:00Z","runEndedAt":"2026-09-26T16:29:00Z"},
        {"name":"silas-run-b","ownedBy":"principal-silas","startedAt":"2026-09-26T16:30:00Z","runEndedAt":""}]});
    let pres = json!({"data":[
        {"name":"silas-presence-a","presenceOf":"session-run-silas-run-a"},
        {"name":"silas-presence-b","presenceOf":"session-run-silas-run-b"}]});
    assert_eq!(presence_at("principal-silas", "2026-09-26T17:16:36Z", &runs, &pres).as_deref(), Some("silas-presence-b"));
    assert_eq!(presence_at("principal-silas", "2026-09-26T15:00:00Z", &runs, &pres).as_deref(), Some("silas-presence-a"));
    // negative proof: no run live then, no presence claimed
    assert_eq!(presence_at("principal-silas", "2026-09-26T10:00:00Z", &runs, &pres), None);
    assert_eq!(presence_at("principal-jeff", "2026-09-26T17:00:00Z", &runs, &pres), None);
}

#[test]
fn a_delivery_row_carries_where_and_how() {
    let s = src("nudge", "wren", "silas", "hi", "delivered");
    let d = delivery_row(&s, "message-src-40158", Some("silas-presence-b"), "delivered");
    assert_eq!(d["deliveryOf"], "message-src-40158");
    assert_eq!(d["deliveredTo"], "silas-presence-b");
    assert_eq!(d["deliveredAt"], "2026-09-26T17:16:36Z");
    let f = delivery_row(&Src { error: "no-window".into(), ..src("nudge", "wren", "kade", "hi", "failed") }, "m", None, "failed");
    assert!(f.get("deliveredTo").is_none());
    assert_eq!(f["deliveryError"], "no-window");
}

#[test]
fn rows_parse_from_sqlite_json() {
    let r = parse_rows(r#"[{"id":40158,"type":"nudge","from":"wren","to":"silas","content":"hi","created_at":"2026-09-26 17:16:35","delivery_status":"delivered","delivered_at":null,"last_delivery_error":null}]"#);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].delivered_at, "");
}

#[test]
fn a_cut_message_is_found_even_when_the_pane_joined_its_lines() {
    // the real 09-26 12:45 case: the content wraps with newline + indent, the pane shows one space
    let s = src("nudge", "wren", "silas", "5 Keep Channel as data.\n  Buzz is then: add one Channel row, add pubkeys to Principals.\n  Step 0: messages live in messages.db", "delivered");
    assert_eq!(outcome(&s, &[" as data. Buzz is then: add one Channel row, add pubkeys to Principals. Step 0: messages live in messages.db".into()]), "truncated");
}
