// @domain: identity — which WebIDs a person's identity-provider account offers at sign-in (#4503)
//! #4503 — Jeff, 2026-10-12: "i dont want to have to pick amongst webids". Every
//! agent and service WebID was made a pod of his account (seed-css.sh), so his
//! sign-in page offers 12. Pure decisions only: which link and which client
//! credentials belong to a name, and whether that name may leave his account.
//! main does the I/O against the account API.

/// Each `"<webid>":"<link url>"` pair from the account's webIdLinks answer.
pub fn webid_links(json: &str) -> Vec<(String, String)> {
    let Some(start) = json.find("\"webIdLinks\"") else { return vec![] };
    let rest = &json[start..];
    let Some(open) = rest.find('{') else { return vec![] };
    let Some(close) = rest[open..].find('}') else { return vec![] };
    let body = &rest[open + 1..open + close];
    let strs: Vec<&str> = body.split('"').enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, s)| s).collect();
    strs.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_string(), c[1].to_string())).collect()
}

/// The pod name a WebID lives under: `https://host/<name>/profile/card#me` → name.
pub fn webid_name(webid: &str) -> Option<&str> {
    webid.split("://").nth(1)?.split('/').nth(1).filter(|s| !s.is_empty())
}

/// The account's client credentials minted for `name`: the URL of each whose id
/// is `<label>-<name>_<uuid>` (chorus-agent-, chorus-3669-, …). Never a prefix
/// match on the name alone: `chorus-agent-silas_…` must not match `si`.
pub fn credentials_for(json: &str, name: &str) -> Vec<String> {
    let Some(start) = json.find("\"clientCredentials\"") else { return vec![] };
    let rest = &json[start..];
    let Some(open) = rest.find('{') else { return vec![] };
    let Some(close) = rest[open..].find('}') else { return vec![] };
    let body = &rest[open + 1..open + close];
    let strs: Vec<&str> = body.split('"').enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, s)| s).collect();
    let needle = format!("-{name}_");
    strs.chunks(2)
        .filter(|c| c.len() == 2 && c[0].contains(&needle))
        .map(|c| c[1].to_string())
        .collect()
}

/// Why `name` may not simply be unlinked from Jeff's account, or None when it may.
/// A name with a live login (a cred.json) or a Principal row must MOVE to its own
/// account first: unlinking it would end that principal's login.
pub fn unlink_refusal(name: &str, has_cred_file: bool, has_principal_row: bool) -> Option<String> {
    if name == "jeff" {
        return Some("jeff is the account's own person; his WebID stays".into());
    }
    if has_cred_file {
        return Some(format!("{name} logs in today (~/.chorus/identity/{name}/cred.json): move it to its own account first, then unlink"));
    }
    if has_principal_row {
        return Some(format!("{name} is a Principal: move it to its own account first, then unlink"));
    }
    None
}

/// The verification token the provider asks for when a WebID is linked to an
/// account that does not own its pod (measured 2026-10-12: a 400 naming
/// `solid:oidcIssuerRegistrationToken` and a quoted uuid).
pub fn registration_token(reply: &str) -> Option<String> {
    let at = reply.find("oidcIssuerRegistrationToken")?;
    let rest = &reply[at..];
    let q = rest.find('"')?;
    let after = rest[q + 1..].trim_start_matches('\\');
    let v: String = after.chars().take_while(|c| c.is_ascii_hexdigit() || *c == '-').collect();
    (v.len() >= 8).then_some(v)
}

/// The one line that proves to the provider the pod owner agrees to the link.
pub fn registration_line(webid: &str, token: &str) -> String {
    format!("\n<{webid}> <http://www.w3.org/ns/solid/terms#oidcIssuerRegistrationToken> \"{token}\".\n")
}

/// The profile with that line taken out again, byte for byte otherwise.
pub fn without_registration_line(card: &str, webid: &str, token: &str) -> String {
    card.replace(&registration_line(webid, token), "")
}

/// A principal's cred.json with the new account's credential id and secret in
/// place of the old one's; every other field (webId, issuer, tokenEndpoint,
/// agent) kept as it was. None when the file is not the expected shape.
pub fn swapped_credential(old: &str, id: &str, secret: &str) -> Option<String> {
    let has = |k: &str| old.contains(&format!("\"{k}\""));
    if !has("id") || !has("secret") || !has("webId") {
        return None;
    }
    let set = |text: &str, k: &str, v: &str| -> Option<String> {
        let key = format!("\"{k}\"");
        let at = text.find(&key)? + key.len();
        let colon = text[at..].find(':')? + at;
        let q1 = text[colon..].find('"')? + colon;
        let q2 = text[q1 + 1..].find('"')? + q1 + 1;
        Some(format!("{}\"{}\"{}", &text[..q1], v.replace('\\', "\\\\").replace('"', "\\\""), &text[q2 + 1..]))
    };
    set(&set(old, "id", id)?, "secret", secret)
}

/// A string field from a small JSON file, pretty-printed or not (cred.json is
/// written with spaces after the colons).
pub fn json_field(text: &str, key: &str) -> Option<String> {
    let k = format!("\"{key}\"");
    let at = text.find(&k)? + k.len();
    let rest = text[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    rest.find('"').map(|end| rest[..end].to_string())
}

/// A URL inside one block of the account API's controls, e.g. ("password",
/// "create"). The blocks' key order varies between answers (measured: a new
/// account lists password.forgot before password.create).
pub fn control(json: &str, block: &str, key: &str) -> Option<String> {
    let b = format!("\"{block}\":{{");
    let start = json.find(&b)? + b.len();
    let end = json[start..].find('}')? + start;
    json_field(&json[start..end], key)
}
