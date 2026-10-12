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
