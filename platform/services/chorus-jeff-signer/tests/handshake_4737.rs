// @test-type: unit — pure: where the signer connects and which host its handshake names; no relay.
// @domain: security
// @card: #4737
// @owner: silas
//! #4737 — src/lib handshake: after the cutover the relay answers only to its
//! public name, so the signer dials the home network but names the relay.
use chorus_jeff_signer::handshake;

#[test]
fn dials_the_lan_but_names_the_public_relay() {
    let (addr, url) = handshake("wss://buzz.lightlifeurbangardens.com", Some("ws://relay.home.test:3000"));
    assert_eq!(addr, "relay.home.test:3000");
    assert_eq!(url, "ws://buzz.lightlifeurbangardens.com");
}

// NEGATIVE PROOF: with no dial address, today's behaviour holds — connect to and name the relay itself
#[test]
fn no_dial_connects_to_and_names_the_relay() {
    assert_eq!(handshake("ws://relay.home.test:3000", None), ("relay.home.test:3000".to_string(), "ws://relay.home.test:3000".to_string()));
    assert_ne!(handshake("wss://buzz.lightlifeurbangardens.com", Some("ws://relay.home.test:3000")).1, "ws://relay.home.test:3000",
        "the handshake must not name the dial address once the relay has a public name");
}
