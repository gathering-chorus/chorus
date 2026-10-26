// @test-type: unit — pure: the AUTH relay tag from its inputs; no relay.
// @domain: messages
// @card: #4657
// @owner: silas
//! #4657 — src/hooks/buzz_reply auth_relay_tag (re-exported by src/nudge_send): the relay accepts one address in
//! a NIP-42 AUTH (its RELAY_URL). Replies and signed nudges dial the LAN/tunnel
//! address but must name BUZZ_AUTH_RELAY_URL once the relay's address is public.
use chorus_hooks::nudge_send::auth_relay_tag;

#[test]
fn a_stated_relay_address_is_the_tag() {
    assert_eq!(auth_relay_tag("ws://relay.home.test:3000", Some("wss://buzz.lightlifeurbangardens.com")), "wss://buzz.lightlifeurbangardens.com");
}

// NEGATIVE PROOF: unset or blank keeps today's tag, so landing with it off changes nothing
#[test]
fn unset_or_blank_keeps_the_dial_address() {
    assert_eq!(auth_relay_tag("ws://relay.home.test:3000", None), "ws://relay.home.test:3000");
    assert_eq!(auth_relay_tag("ws://relay.home.test:3000", Some("  ")), "ws://relay.home.test:3000");
}
