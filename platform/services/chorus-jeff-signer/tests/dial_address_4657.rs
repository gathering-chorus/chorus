// @test-type: unit — pure: where the signer connects, from its two settings; no relay.
// @domain: security
// @card: #4657
// @owner: silas
//! #4657 — src/lib dial_address: the relay's own address (CHORUS_SIGNER_RELAY)
//! becomes the public wss:// one, which this build cannot dial, so the daemon
//! connects on the LAN through CHORUS_SIGNER_DIAL while every AUTH still names
//! the relay's address.
// src/main connects to dial_address(CHORUS_SIGNER_RELAY, CHORUS_SIGNER_DIAL).
use chorus_jeff_signer::dial_address;

#[test]
fn a_stated_dial_address_is_where_it_connects() {
    assert_eq!(dial_address("wss://buzz.lightlifeurbangardens.com", Some("ws://192.168.86.242:3000")), "ws://192.168.86.242:3000");
}

// NEGATIVE PROOF: unset or blank keeps today's behaviour — it connects to the relay address
#[test]
fn unset_or_blank_dials_the_relay_address() {
    assert_eq!(dial_address("ws://192.168.86.242:3000", None), "ws://192.168.86.242:3000");
    assert_eq!(dial_address("ws://192.168.86.242:3000", Some(" ")), "ws://192.168.86.242:3000");
}
