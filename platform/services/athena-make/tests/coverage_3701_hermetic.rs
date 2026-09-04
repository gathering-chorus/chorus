//! #3701 — hermetic coverage: a full in-test Fuseki stub (std TcpListener, port 0)
//! answers every SPARQL query athena-make issues with fixture rows, a stub DAL binary
//! (CHORUS_MODEL_BIN) absorbs writes, and a REAL serve() loop runs on a scratch
//! port. No live Fuseki, no $HOME/~/.chorus state, no live athena-make — the test
//! brings its own world (CHORUS_FUSEKI / CHORUS_HOME / CHORUS_MODEL_BIN /
//! CSS_ISSUER etc. are all set ONCE in the OnceLock
//! world() init, before any test logic runs — env mutation is serialized by
//! construction; no test changes env afterwards).

use athena_make::{generate, serve, RouteTable};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::OnceLock;

const NS: &str = "https://jeffbridwell.com/chorus#";
const NOBODY_WEBID: &str = "http://localhost:3000/pods/chorus/_agents/nobody/profile/card.ttl#me";
const ROLELESS_SCOPED_WEBID: &str =
    "http://localhost:3000/pods/chorus/_agents/roleless-scoped/profile/card.ttl#me";
const UNSCOPED_ROLE_WEBID: &str =
    "http://localhost:3000/pods/chorus/_agents/unscoped-role/profile/card.ttl#me";
const WREN_WEBID: &str = "http://localhost:3000/pods/chorus/_agents/wren/profile/card.ttl#me";
// ───────────────────────── the SPARQL fixture stub ─────────────────────────

fn sparql_rows(vals: &[String]) -> String {
    let bindings = vals
        .iter()
        .map(|v| {
            format!(
                "{{\"v\":{{\"type\":\"literal\",\"value\":\"{}\"}}}}",
                v.replace('\\', "\\\\").replace('"', "\\\"")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"head\":{{\"vars\":[\"v\"]}},\"results\":{{\"bindings\":[{}]}}}}",
        bindings
    )
}

// urldecode + rows_for have a TRIMMED mirror in src/main.rs cli_tests (bin unit
// tests can't import from tests/ — separate compilation units). If a query shape
// changes here, check the mirror; the fixtures are kept deliberately small so a
// drift shows as a test failure, not silent divergence.
fn urldecode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}


/// Apply the query's own LIMIT/OFFSET to fixture rows — the store's job, modelled.
fn apply_limit_offset(q: &str, rows: Vec<String>) -> Vec<String> {
    let num_after = |kw: &str| -> Option<usize> {
        let i = q.find(kw)?;
        q[i + kw.len()..].split_whitespace().next()?.parse::<usize>().ok()
    };
    let offset = num_after("OFFSET ").unwrap_or(0);
    let limit = num_after("LIMIT ");
    let sliced: Vec<String> = rows.into_iter().skip(offset).collect();
    match limit {
        Some(n) => sliced.into_iter().take(n).collect(),
        None => sliced,
    }
}

/// The model fixture: dispatch a decoded SPARQL query to its canned rows.
fn rows_for(q: &str) -> Vec<String> {
    let s = |v: &str| v.to_string();
    // #3688 / ADR-054 §3.3 — the holdsRole map the door resolves the caller's
    // role from. One row per edge, "<webid> <role>". Answered FIRST so no later
    // shape branch can claim it.
    // #3689 — the ES256 door needs the allow-set (HS256 used the KeyRegistry
    // and never asked). The fixture registers wren as a Principal.
    if q.contains("chorus:Principal") && q.contains("chorus:webId") && !q.contains("chorus:holdsRole") && !q.contains("chorus:hasScope") {
        return vec![
            WREN_WEBID.to_string(),
            NOBODY_WEBID.to_string(),
            ROLELESS_SCOPED_WEBID.to_string(),
            UNSCOPED_ROLE_WEBID.to_string(),
        ];
    }
    if q.contains("chorus:holdsRole") {
        return vec![
            format!("{} {}role-wren", WREN_WEBID, NS),
            format!("{} {}role-wren", UNSCOPED_ROLE_WEBID, NS),
        ];
    }
    // #3689 — hasScope: the door resolves write grants from the model. The
    // fixture grants wren the graphs the write tests exercise.
    if q.contains("chorus:hasScope") {
        // wren: the entity tables' instances graph + the batch test graph.
        // NOBODY_WEBID is deliberately absent — allowed to authenticate, zero
        // grants: the model-scope replacement for "unscoped token".
        return vec![
            format!("{} urn:chorus:instances", WREN_WEBID),
            format!("{} urn:test:instances", WREN_WEBID),
            format!("{} urn:chorus:domains:tests", WREN_WEBID),
            format!("{} urn:chorus:ontology", WREN_WEBID),
            format!("{} urn:test:instances", ROLELESS_SCOPED_WEBID),
            format!("{} urn:chorus:domains:tests", ROLELESS_SCOPED_WEBID),
        ];
    }
    // #4010 — the collection COUNT. The serve path now asks the store for the
    // total separately and pushes LIMIT/OFFSET down, so the fixture must answer
    // the count query too. Answered BEFORE the shape branches because it also
    // mentions the class IRI.
    //
    // The reason the split exists: the old path fetched every row to count them,
    // which on the tests domain meant 190,941 rows / 74.6MB / 22.3s and a 502.
    if q.contains("COUNT(DISTINCT ?s)") && q.contains("?s a <") {
        return vec![s("2")];
    }
    // ---- generate()-time shape queries ----
    if q.contains("OPTIONAL { ?p sh:datatype") {
        return if q.contains("#Domain>") {
            vec![
                s("comment|datatype:string"),
                s("label|plain"),
                s("ownedBy|edge:Role"),
                s("secretive|datatype:string"),
                s("status|plain"),
            ]
        } else if q.contains("#Product>") {
            vec![s("hasDomain|edge:Domain"), s("label|plain"), s("status|plain")]
        } else if q.contains("#TestResult>") {
            vec![
                s("filePath|plain"),
                s("ofTest|edge:Test"),
                s("result|plain"),
                s("testName|plain"),
            ]
        } else if q.contains("#Orphan>") {
            vec![s("label|plain")]
        } else if q.contains("#Weird>") {
            vec![s("BadField|plain")]
        } else {
            vec![]
        };
    }
    if q.contains("chorus:requiresAuth") {
        return if q.contains("#Domain>") { vec![s("secured")] } else { vec![] };
    }
    if q.contains("sh:minCount") {
        return if q.contains("#Domain>") {
            vec![s("comment|field")]
        } else if q.contains("#TestResult>") {
            vec![s("filePath|field"), s("ofTest|edge"), s("result|field"), s("testName|field")]
        } else {
            vec![]
        };
    }
    if q.contains("chorus:exposure") {
        return if q.contains("#Domain>") {
            vec![
                s("comment|internal"),
                s("label|public"),
                s("ownedBy|public"),
                s("secretive|secret"),
                s("status|public"),
            ]
        } else {
            vec![]
        };
    }
    if q.contains("chorus:repoTarget ?mount") {
        // read_domain_surfaces: dom|mount|class
        return vec![s("athena|borg/props|Domain")];
    }
    if q.contains("chorus:repoTarget") || q.contains("#repoTarget>") {
        return vec![]; // no declared override (shape or verb)
    }
    if q.contains("chorus:atStep") {
        return vec![s("value-stream-step-designing")];
    }
    if q.contains("chorus:partOf ?t") {
        return vec![s("loom")];
    }
    if q.contains("chorus:instancesGraph") {
        return if q.contains("#Domain>") {
            vec![s("urn:test:instances")]
        } else if q.contains("#Revision>") {
            // #4102 — a replace keeps the version it displaces, and the Revision
            // it writes needs the home graph of chorus:Revision. Without this row
            // the fixture's model has no home for the class, so the door refuses
            // every replace fail-closed (it will not silently drop a version).
            vec![s("urn:chorus:instances")]
        } else if q.contains("#TestResult>") {
            vec![s("urn:chorus:domains:tests")]
        } else {
            vec![]
        };
    }
    if q.contains("SELECT DISTINCT ?v") && q.contains("definesVocabulary ?c") {
        return vec![s("Domain"), s("Product")];
    }
    if q.contains("definesVocabulary <") {
        return if q.contains("#Product>") || q.contains("#Domain>") {
            vec![s("athena")]
        } else if q.contains("#TestResult>") {
            vec![s("tests")]
        } else {
            vec![]
        };
    }
    if q.contains("definesVocabulary ?c") {
        return vec![s("Domain"), s("Product")];
    }
    if q.contains("chorus:treeEdge") {
        return if q.contains("#Domain>") { vec![s("hasChild")] } else { vec![] };
    }
    if q.contains("chorus:treeOrder") {
        return if q.contains("#Domain>") { vec![s("stageOrder")] } else { vec![] };
    }
    // ---- VerbShape (generate_verb) ----
    if q.contains("#verbFamily>") {
        return if q.contains("verb-athena-deploy>") { vec![s("athena")] } else { vec![] };
    }
    if q.contains("#invocability>") {
        return vec![s("woven")];
    }
    if q.contains("#verbInput>") {
        return vec![s("card|datatype:integer"), s("role|datatype:string")];
    }
    if q.contains("#verbOutput>") {
        return vec![s("landed|datatype:boolean")];
    }
    if q.contains("#verbEdge>") {
        return vec![s("atStep|building")];
    }
    // ---- product index ----
    if q.contains("chorus:hasDomain ?d") {
        return vec![s("borg"), s("cards")];
    }
    // ---- shape_meta version fingerprint ----
    if q.contains("sh:path ?path BIND(STR(?path)") {
        return vec![format!("{NS}comment"), format!("{NS}label")];
    }
    // ---- serve-time instance reads ----
    if q.contains("REPLACE(STR(?s), '.*[#/]', ''), '|', REPLACE(STR(?o)") {
        // tree edge set (root→alpha, alpha→leaf, root→beta) + a detached cycle
        return vec![s("root|alpha"), s("alpha|leaf"), s("root|beta"), s("cyc1|cyc2"), s("cyc2|cyc1")];
    }
    if q.contains("stageOrder> ?r") {
        return vec![s("beta|1"), s("alpha|2")];
    }
    if q.contains("inStream") {
        // /contains fold (contains UNION hasDomain UNION inStream-inverse)
        return vec![format!("{NS}alpha"), format!("{NS}beta")];
    }
    if q.contains("hasDomain> <") {
        // /partof fold
        return vec![format!("{NS}loom")];
    }
    if q.contains("hasChild> ?o") {
        // /has-child fold
        return vec![format!("{NS}childx")];
    }
    if q.contains("FILTER(isLiteral(?o))") {
        // /completeness present-literals
        return if q.contains("#pulse>") {
            vec![s("comment|has one"), s("label|Pulse")]
        } else {
            vec![s("label|Empty")]
        };
    }
    if q.contains("hasProperty") {
        // #3863 — rows now carry the OWNING scope and its class:
        // owner|ownerClass|propIri|key|valueType|value. Two scopes here, not
        // one, so this fixture exercises a real cascade: the Service's value
        // must beat the Product's. A one-element fixture could not tell the
        // two apart, which is how the hardcoded-Service kind survived.
        return vec![
            format!("{NS}pulse|{NS}Service|{NS}prop-1|alert.threshold|int|42"),
            format!("{NS}chorus|{NS}Product|{NS}prop-2|alert.threshold|int|999"),
        ];
    }
    if q.contains("chorus:ownedBy") {
        for owned in ["pulse", "hasparent", "phantom", "dalboom", "shapefail", "borg"] {
            if q.contains(&format!("#{owned}>")) {
                return vec![s("wren")];
            }
        }
        return vec![];
    }
    if q.contains("chorus:partOf ?p . BIND('y'") {
        return if q.contains("#hasparent>") { vec![s("y")] } else { vec![] };
    }
    if q.contains("?p ?o . BIND('y'") {
        // entity_exists
        for e in ["pulse", "hasparent", "borg", "dalboom", "testresult-existing"] {
            if q.contains(&format!("#{e}>")) {
                return vec![s("y")];
            }
        }
        return vec![];
    }
    if q.contains("CONCAT(STR(?p), \"|\", STR(?o))") {
        // entity read (all direct props)
        if q.contains("#pulse>") {
            return vec![
                format!("http://www.w3.org/1999/02/22-rdf-syntax-ns#type|{NS}Domain"),
                format!("{NS}comment|internal notes"),
                format!("{NS}contains|{NS}alpha"),
                format!("{NS}contains|{NS}beta"),
                format!("{NS}label|Pulse"),
                format!("{NS}ownedBy|{NS}wren"),
                format!("{NS}secretive|hidden-value"),
                format!("{NS}status|http://example.org/x#active"),
            ];
        }
        return vec![];
    }
    if q.contains("?s a <") {
        // collection list: subj␟label␟status␟extra… — U+001F columns since #4045 (a value may contain '|')
        let canned: Vec<String> = if q.contains("#Domain>") {
            vec![format!("{NS}borg\u{1f}Borg\u{1f}active\u{1f}wren"), format!("{NS}pulse\u{1f}Pulse\u{1f}active\u{1f}wren")]
        } else if q.contains("#Product>") {
            vec![
                format!("{NS}loom\u{1f}Loom\u{1f}active\u{1f}athena"),
                format!("{NS}loom\u{1f}Loom\u{1f}active\u{1f}borg"),
                format!("{NS}solo\u{1f}Solo\u{1f}\u{1f}"),
            ]
        } else {
            vec![]
        };
        // #4022 — the page is two round-trips, and the fixture answers each the
        // way the store does: the SUBJECT page (`SELECT (STR(?s) AS ?v)`, sliced by
        // LIMIT/OFFSET in stub_handle) is the subject column only; the PROJECTION
        // (`VALUES ?s { <…> }` inside the GRAPH block) is every canned row whose
        // subject is in the VALUES list — multi-valued edges keep all their rows.
        if q.contains("SELECT (STR(?s) AS ?v)") {
            let mut subs: Vec<String> = canned.iter()
                .map(|r| r.split('\u{1f}').next().unwrap_or("").to_string()).collect();
            subs.dedup();
            return subs;
        }
        if let Some(i) = q.find("VALUES ?s {") {
            let list = &q[i + "VALUES ?s {".len()..];
            let list = &list[..list.find('}').unwrap_or(list.len())];
            let wanted: Vec<&str> = list.split_whitespace()
                .map(|t| t.trim_start_matches('<').trim_end_matches('>')).collect();
            return canned.into_iter()
                .filter(|r| wanted.contains(&r.split('\u{1f}').next().unwrap_or("")))
                .collect();
        }
        return canned;
    }
    vec![] // principals / JWKS-adjacent / anything else → empty bindings
}

fn stub_handle(stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let req = athena_make::read_http_request(stream, 1 << 20);
    let body = req.splitn(2, "\r\n\r\n").nth(1).unwrap_or("");
    // #4022 — the client POSTs a raw application/sparql-query body (the query
    // outgrew argv and url-encoding); the form shape is kept for older callers.
    let raw_sparql = req.to_ascii_lowercase().contains("content-type: application/sparql-query");
    let form_query = if raw_sparql { None } else { body.find("query=") };
    let resp_body = if raw_sparql || form_query.is_some() {
        let q = if raw_sparql { body.to_string() } else { urldecode(&body[form_query.unwrap() + 6..]) };
        // #4010 — the fixture now SLICES, because the real store does.
        // athena-make pushes LIMIT/OFFSET into SPARQL rather than fetching every
        // row and paginating in memory; a stub that ignored them would answer a
        // full page for `?limit=1` and let a broken push-down pass.
        sparql_rows(&apply_limit_offset(&q, rows_for(&q)))
    } else if req.contains("/.oidc/jwks") {
        // #3689 — the fixture CSS publishes the test key: ES256 replaced the
        // HS256 fixtures when the legacy arm was deleted.
        test_jwks_json()
    } else {
        "{}".to_string()
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/sparql-results+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        resp_body.len(),
        resp_body
    );
    let _ = stream.write_all(resp.as_bytes());
}

fn start_stub() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("stub bind");
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming() {
            if let Ok(mut c) = c {
                std::thread::spawn(move || stub_handle(&mut c));
            }
        }
    });
    port
}

// ──────────────── token minting — #3689: ES256, the only family the door
// accepts since the HS256 arm was deleted. Deterministic P-256 test key; the
// stub CSS publishes its public half at /.oidc/jwks exactly as CSS would.
// The `scope` parameter is retained in the signature but UNUSED: scope is
// model data (chorus:hasScope fixture rows), not a claim — passing a claim
// scope here would test a mechanism that no longer exists.
use p256::ecdsa::signature::Signer as _;

fn b64url(data: &[u8]) -> String {
    // RFC 4648 §5, unpadded — same table the real minters use.
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 { out.push(T[(n >> 6) as usize & 63] as char); }
        if chunk.len() > 2 { out.push(T[n as usize & 63] as char); }
    }
    out
}

const TEST_KID: &str = "hermetic-css-key-1";

fn test_signing_key() -> p256::ecdsa::SigningKey {
    p256::ecdsa::SigningKey::from_slice(&[7u8; 32]).expect("valid scalar")
}

fn test_jwks_json() -> String {
    let point = test_signing_key().verifying_key().to_encoded_point(false);
    format!(
        r#"{{"keys":[{{"kty":"EC","crv":"P-256","alg":"ES256","kid":"{}","x":"{}","y":"{}"}}]}}"#,
        TEST_KID, b64url(point.x().unwrap()), b64url(point.y().unwrap()),
    )
}

fn mint_token(web_id: &str, _scope: Option<&[&str]>) -> String {
    let header = b64url(format!(r#"{{"alg":"ES256","typ":"JWT","kid":"{}"}}"#, TEST_KID).as_bytes());
    // iss must equal the CSS_ISSUER the World pinned (the stub base) — verify
    // checks issuer before anything else. Force World init FIRST: a test whose
    // first statement is mint_token would otherwise read whatever CSS_ISSUER
    // the invoking shell carries (e.g. the live lightlife issuer) and mint an
    // IssuerMismatch token — green in the full suite (an earlier test inits the
    // World), 401 when run solo. Hermetic contract (#3528): a test brings its
    // own world, never the shell's. Found on #3774; pre-existing, every test.
    let _ = world();
    let iss = std::env::var("CSS_ISSUER").expect("World sets CSS_ISSUER before minting");
    let payload = b64url(format!(
        "{{\"iss\":\"{}\",\"webid\":\"{}\",\"aud\":\"solid\",\"exp\":4102444800}}",
        iss, web_id
    ).as_bytes());
    let signing_input = format!("{}.{}", header, payload);
    let sig: p256::ecdsa::Signature = test_signing_key().sign(signing_input.as_bytes());
    format!("{}.{}.{}", header, payload, b64url(&sig.to_bytes()))
}

// ───────────────────────── the world (env set ONCE) ────────────────────────

struct World {
    port: u16,
    domain: RouteTable,
    product: RouteTable,
    test_result: RouteTable,
    home: std::path::PathBuf,
    dal_batch_log: std::path::PathBuf,
}

static WORLD: OnceLock<World> = OnceLock::new();

fn world() -> &'static World {
    WORLD.get_or_init(|| {
        let stub_port = start_stub();
        let stub_base = format!("http://127.0.0.1:{}", stub_port);
        let home = std::env::temp_dir().join(format!("owl3701-home-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        // stub DAL: succeeds unless the argv names a poison entity; add-batch
        // records its exact argv, forwarded token, and raw stdin for assertions.
        let dal = home.join("dal-stub.sh");
        let dal_batch_log = home.join("dal-add-batch.log");
        let dal_script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"add-batch\" ]; then\n  batch_input=$(cat)\n  {{\n    printf 'ARGV\\t%s\\n' \"$*\"\n    printf 'TOKEN\\t%s\\n' \"$CHORUS_IDENTITY_TOKEN\"\n    printf 'STDIN\\n%s\\nEND\\n' \"$batch_input\"\n  }} > '{}'\n  case \"$batch_input\" in\n    *testresult-existing*) echo \"add-batch: entity 'test-result:testresult-existing': already-exists\" >&2; exit 1;;\n    *\\\"name\\\":\\\"pulse\\\"*) echo \"add-batch: entity 'domain:pulse': already-exists: entity already exists\" >&2; exit 1;;\n    *shapefail*) echo 'shape-violation: comment missing' >&2; exit 1;;\n    *retiredstub*) echo 'chorus-model is RETIRED (#3718) - use athena-model instead.' >&2; exit 1;;\n  esac\nfi\ncase \"$*\" in\n  *shapefail*) echo 'shape-violation: comment missing' >&2; exit 1;;\n  *dalboom*) echo kaboom >&2; exit 1;;\n  *retiredstub*) echo 'chorus-model is RETIRED (#3718) - use athena-model instead.' >&2; exit 1;;\nesac\nexit 0\n",
            dal_batch_log.display(),
        );
        std::fs::write(
            &dal,
            dal_script,
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dal, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // ALL env, set once, before any athena-make logic runs (hermetic world).
        std::env::set_var("CHORUS_FUSEKI", &stub_base);
        std::env::set_var("CHORUS_HOME", home.to_str().unwrap());
        std::env::set_var("CHORUS_MODEL_BIN", dal.to_str().unwrap());
        std::env::set_var("CSS_ISSUER", &stub_base);
        // generate() against the stub — the same tables serve() mounts
        let domain = generate("Domain").expect("generate Domain");
        let product = generate("Product").expect("generate Product");
        let test_result = generate("TestResult").expect("generate TestResult");
        // OS-assigned free port for the real serve loop
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let tables = vec![domain.clone(), product.clone(), test_result.clone()];
        std::thread::spawn(move || {
            let _ = serve(port, &tables);
        });
        // wait for ready
        let mut ready = false;
        for _ in 0..100 {
            if let Ok((code, _, _)) = try_http(port, "GET", "/health", &[], "") {
                if code == 200 {
                    ready = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(ready, "hermetic athena-make serve did not come up on :{}", port);
        World { port, domain, product, test_result, home, dal_batch_log }
    })
}

fn try_http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> std::io::Result<(u16, String, String)> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    let mut req = format!("{} {} HTTP/1.1\r\nHost: t\r\nContent-Length: {}\r\n", method, path, body.len());
    for (k, v) in headers {
        req.push_str(&format!("{}: {}\r\n", k, v));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes())?;
    let mut buf = String::new();
    s.read_to_string(&mut buf)?;
    let code: u16 = buf
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let (h, b) = buf.split_once("\r\n\r\n").unwrap_or((buf.as_str(), ""));
    Ok((code, h.to_string(), b.to_string()))
}

fn http(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> (u16, String, String) {
    let w = world();
    try_http(w.port, method, path, headers, body).expect("request to hermetic serve")
}

fn bearer(tok: &str) -> String {
    format!("Bearer {}", tok)
}

// ───────────────────────── generate() family ───────────────────────────────

#[test]
fn generate_projects_the_full_route_table_from_the_stub_model() {
    let w = world();
    let t = &w.domain;
    assert_eq!(t.class, format!("{}Domain", NS));
    assert!(t.fields.contains(&"comment|datatype:string".to_string()));
    assert!(t.fields.contains(&"ownedBy|edge:Role".to_string()));
    assert!(t.routes.contains(&"GET /domains".to_string()));
    assert!(t.routes.contains(&"POST /domains/:name/partof".to_string()));
    assert!(t.routes.contains(&"GET /domains/:name/tree".to_string()), "treeEdge opt-in emits the tree route");
    assert_eq!(t.secured, vec!["/schema/domain".to_string()]);
    assert_eq!(t.mandatory, vec!["comment".to_string()]);
    assert_eq!(t.write_required, vec!["comment".to_string()]);
    assert_eq!(t.instances_graph, "urn:test:instances");
    // repoTarget projected from the containment chain (no declared override)
    assert_eq!(t.repo_target, "designing/products/loom/domains/domain");
    assert_eq!(t.tree_edges, vec!["hasChild".to_string()]);
    assert_eq!(t.tree_order.as_deref(), Some("stageOrder"));
    // Product: unsecured, instances graph derived from its declaring domain
    assert!(w.product.secured.is_empty());
    assert_eq!(w.product.instances_graph, "urn:chorus:domains:athena");
    assert!(w.product.tree_edges.is_empty());
    assert_eq!(w.test_result.instances_graph, "urn:chorus:domains:tests");
    assert!(w.test_result.fields.contains(&"ofTest|edge:Test".to_string()));
    assert!(w.test_result.write_required.contains(&"ofTest".to_string()));
    assert!(w.test_result.routes.contains(&"POST /testresults/batch".to_string()));
}

#[test]
fn generate_refusals_are_typed() {
    let _ = world();
    let e = generate("bad-class").unwrap_err();
    assert!(e.contains("adr040-violation"), "{}", e);
    let e = generate("Weird").unwrap_err();
    assert!(e.contains("not camelCase"), "shape-sourced field law: {}", e);
    let e = generate("NoShape").unwrap_err();
    assert!(e.contains("no shape found"), "{}", e);
    let e = generate("Orphan").unwrap_err();
    assert!(e.contains("no instance home"), "ADR-051 refusal: {}", e);
}

#[test]
fn generate_verb_projects_the_generation_gap_seam() {
    let _ = world();
    let code = athena_make::generate_verb("athena-deploy").expect("verb projects");
    assert!(code.contains("pub trait AthenaDeployLogic"), "{}", code);
    assert!(code.contains("fn run(&self, card: i64, role: String) -> R<bool>"), "{}", code);
    assert!(code.contains("(\"atStep\", \"building\")"), "wiring projected: {}", code);
    assert!(code.contains("invocability=woven"), "{}", code);
    let e = athena_make::generate_verb("nope").unwrap_err();
    assert!(e.contains("no VerbShape instance"), "{}", e);
}

#[test]
fn product_index_and_domain_vocab_project_from_edges() {
    let _ = world();
    let idx = athena_make::generate_product_index("athena").unwrap();
    assert_eq!(
        idx,
        "{ \"product\": \"athena\", \"domains\": [{ \"name\": \"borg\", \"api\": \"/borg\" }, { \"name\": \"cards\", \"api\": \"/cards\" }] }"
    );
    let tables = athena_make::generate_domain_vocab("athena").unwrap();
    assert_eq!(tables.len(), 2, "athena definesVocabulary Domain+Product");
    let vocab = athena_make::all_vocab_classes().unwrap();
    assert_eq!(vocab, vec!["Domain".to_string(), "Product".to_string()]);
    let surfaces = athena_make::read_domain_surfaces().unwrap();
    assert_eq!(surfaces.len(), 1);
    assert_eq!(surfaces[0].mount, "borg/props");
    assert_eq!(surfaces[0].classes, vec!["Domain".to_string()]);
}

// ───────────────────────── serve(): discovery + reads ──────────────────────

#[test]
fn serve_discovery_health_and_liveness() {
    let (c, _, b) = http("GET", "/health", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"ok\": true"));
    let (c, _, b) = http("GET", "/livez", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"ok\": true"));
    let (c, _, b) = http("GET", "/", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"kind\": \"Discovery\""));
    assert!(b.contains("\"collection\": \"/v1/domains\""), "{}", b);
    assert!(b.contains("\"collection\": \"/v1/products\""), "{}", b);
    assert!(b.contains("\"collection\": \"/v1/testresults\""), "{}", b);
    let (c, _, b) = http("GET", "/v1", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"count\": 3"));
}

#[test]
fn serve_collection_is_enveloped_and_paginated() {
    let (c, _, b) = http("GET", "/domains", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"apiVersion\": \"v1\""), "{}", b);
    assert!(b.contains("\"count\": 2"), "{}", b);
    assert!(b.contains("\"name\": \"pulse\""), "{}", b);
    assert!(b.contains("\"requiresAuth\": true"), "Domain schema surface is secured: {}", b);
    // page 1 of limit=1 carries the next-cursor link
    let (c, _, b) = http("GET", "/domains?limit=1", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"next\": \"/v1/domains?cursor=1&limit=1\""), "{}", b);
    let (c, _, b) = http("GET", "/domains?limit=1&cursor=1", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"name\": \"pulse\"") && !b.contains("\"name\": \"borg\""), "{}", b);
    // Product: no exposure annotations → open projection; multi-valued edge → array
    let (c, _, b) = http("GET", "/products", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"hasDomain\": [\"athena\", \"borg\"]"), "multi-valued extra renders as array: {}", b);
    assert!(b.contains("\"name\": \"solo\""), "{}", b);
}

#[test]
fn serve_entity_read_projects_data_links_and_exposure() {
    // unauth: internal + secret + unmarked fields hidden, public shown
    let (c, _, b) = http("GET", "/domains/pulse", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"label\": \"Pulse\""), "{}", b);
    assert!(b.contains("\"status\": \"active\""), "http-fragment literal strips to localname: {}", b);
    assert!(!b.contains("internal notes"), "internal field hidden unauth: {}", b);
    assert!(!b.contains("hidden-value"), "secret never serves: {}", b);
    assert!(b.contains("\"contains\": [\"chorus:alpha\", \"chorus:beta\"]"), "multi-valued edge → links array: {}", b);
    assert!(b.contains("\"ownedBy\": \"chorus:wren\""), "{}", b);
    assert!(b.contains("\"id\": \"chorus:pulse\""), "{}", b);
    // authed: internal appears, secret still never
    let tok = mint_token(WREN_WEBID, None);
    let (c, _, b) = http("GET", "/domains/pulse", &[("Authorization", &bearer(&tok))], "");
    assert_eq!(c, 200);
    assert!(b.contains("internal notes"), "internal shows to authed caller: {}", b);
    assert!(!b.contains("hidden-value"), "{}", b);
    // 404 is the enveloped RFC-9457 problem
    let (c, _, b) = http("GET", "/domains/ghost", &[], "");
    assert_eq!(c, 404);
    assert!(b.contains("\"kind\": \"Error\""), "{}", b);
    assert!(b.contains("/errors/not-found"), "{}", b);
    assert!(b.contains("no such domain: ghost"), "{}", b);
    // injection-shaped name → 400
    let (c, _, b) = http("GET", "/domains/bad%20name", &[], "");
    assert_eq!(c, 400);
    assert!(b.contains("invalid entity name"), "{}", b);
}

#[test]
fn serve_folds_completeness_and_tree() {
    let (c, _, b) = http("GET", "/domains/pulse/contains", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"count\": 2") && b.contains("\"alpha\""), "{}", b);
    let (c, _, b) = http("GET", "/domains/pulse/partof", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"partof\": [\"loom\"]"), "{}", b);
    let (c, _, b) = http("GET", "/domains/pulse/has-child", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"hasChild\": [\"childx\"]"), "{}", b);
    let (c, _, b) = http("GET", "/domains/pulse/completeness", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"met\": true") && b.contains("\"percentage\": 100"), "{}", b);
    let (c, _, b) = http("GET", "/domains/empty/completeness", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"met\": false") && b.contains("\"missing\": [\"comment\"]"), "{}", b);
    // tree: rank-ordered siblings (beta rank 1 before alpha rank 2), depth-bounded
    let (c, _, b) = http("GET", "/domains/root/tree", &[], "");
    assert_eq!(c, 200);
    let beta = b.find("beta").unwrap();
    let alpha = b.find("alpha").unwrap();
    assert!(beta < alpha, "rank order beta<alpha: {}", b);
    assert!(b.contains("\"name\": \"leaf\""), "{}", b);
    let (c, _, b) = http("GET", "/domains/root/tree?depth=1", &[], "");
    assert_eq!(c, 200);
    assert!(!b.contains("leaf"), "depth=1 stops above leaf: {}", b);
    // cycle → 409, named
    let (c, _, b) = http("GET", "/domains/cyc1/tree", &[], "");
    assert_eq!(c, 409);
    assert!(b.contains("\"error\": \"cycle\""), "{}", b);
    // a kind that didn't opt in has no tree surface
    let (c, _, _) = http("GET", "/products/loom/tree", &[], "");
    assert_eq!(c, 404);
}

#[test]
fn serve_openapi_schema_and_composed_surfaces() {
    // per-plural served OpenAPI (machine + human)
    let (c, _, b) = http("GET", "/domains/openapi.json", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"openapi\": \"3.1.0\""), "{}", b);
    assert!(b.contains("\"required\": [\"comment\"]"), "completeness floor projected: {}", b);
    let (c, h, b) = http("GET", "/products/openapi", &[], "");
    assert_eq!(c, 200);
    assert!(h.contains("text/html"), "{}", h);
    assert!(b.contains("generated Product API"), "{}", b);
    // secured schema surface: 401 unauth, 200 with a real service token
    let (c, _, b) = http("GET", "/schema/domain", &[], "");
    assert_eq!(c, 401);
    assert!(b.contains("unauthorized"), "{}", b);
    let tok = mint_token(WREN_WEBID, None);
    let (c, _, b) = http("GET", "/schema/domain", &[("Authorization", &bearer(&tok))], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"generatedFrom\""), "{}", b);
    // composed domain surface: index + class sub-resource rewrite
    let (c, _, b) = http("GET", "/borg/props", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"domain\": \"athena\""), "{}", b);
    assert!(b.contains("\"class\": \"Domain\""), "{}", b);
    let (c, _, b) = http("GET", "/borg/props/domain/pulse", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"id\": \"chorus:pulse\""), "sub-resource rewrites to /domains/pulse: {}", b);
    let (c, _, b) = http("GET", "/borg/props/nosuch/x", &[], "");
    assert_eq!(c, 200);
    assert!(b.contains("\"vocab\""), "unknown sub falls back to the index: {}", b);
    // unknown resource → typed 404 listing the served roots
    let (c, _, b) = http("GET", "/widgets", &[], "");
    assert_eq!(c, 404);
    for root in ["/domains", "/products", "/testresults"] {
        assert!(b.contains(&format!("\"{}\"", root)), "served roots must include {}: {}", root, b);
    }
}

#[test]
fn serve_etag_conditional_get_and_telemetry() {
    let (c, h, _) = http("GET", "/domains/pulse", &[], "");
    assert_eq!(c, 200);
    let etag = h
        .lines()
        .find(|l| l.starts_with("ETag:"))
        .and_then(|l| l.split('"').nth(1))
        .expect("ETag on a 200 GET")
        .to_string();
    let inm = format!("\"{}\"", etag);
    let (c, h2, b2) = http("GET", "/domains/pulse", &[("If-None-Match", &inm)], "");
    assert_eq!(c, 304, "conditional GET revalidates: {}", h2);
    assert!(b2.is_empty(), "304 has no body");
    // telemetry: the dated jsonl landed under the world's CHORUS_HOME
    let w = world();
    let logs = w.home.join("ops").join("logs");
    let mut found = false;
    if let Ok(rd) = std::fs::read_dir(&logs) {
        for e in rd.flatten() {
            let content = std::fs::read_to_string(e.path()).unwrap_or_default();
            if content.contains("\"event\":\"api.request.served\"") && content.contains("\"class\":\"Domain\"") {
                found = true;
            }
        }
    }
    assert!(found, "telemetry jsonl written under {:?}", logs);
}

// ───────────────────────── serve(): writes ─────────────────────────────────

#[test]
fn writes_require_authn_and_respect_scope() {
    // no token → 401, typed
    let (c, _, b) = http("POST", "/domains", &[], "{\"name\":\"x\"}");
    assert_eq!(c, 401);
    assert!(b.contains("authn-missing"), "{}", b);
    // #3689 — out-of-scope target → 403. Scope is the Principal's hasScope
    // grants; wren is granted urn:{chorus,test}:instances + urn:chorus:ontology,
    // and urn:other:graph is not among them. (The mint's scope param is inert.)
    let tok = mint_token(WREN_WEBID, None);
    let (c, _, b) = http(
        "POST",
        "/domains",
        &[("Authorization", &bearer(&tok)), ("x-target-graph", "urn:other:graph")],
        "{\"name\":\"x\"}",
    );
    assert_eq!(c, 403);
    assert!(b.contains("out-of-scope"), "{}", b);
    // Even an allowed graph cannot be used as a decoy header for a class whose
    // generated route writes a different instances graph.
    let (c, _, b) = http(
        "POST",
        "/domains",
        &[("Authorization", &bearer(&tok)), ("x-target-graph", "urn:chorus:instances")],
        "{\"name\":\"x\"}",
    );
    assert_eq!(c, 403);
    assert!(b.contains("does not match this class's write graph"), "{}", b);
}

#[test]
fn write_lifecycle_create_replace_edge_delete() {
    let tok = mint_token(WREN_WEBID, None); // legacy/unscoped → ownedBy authZ path
    let auth = bearer(&tok);
    let hdrs: &[(&str, &str)] = &[("Authorization", &auth)];
    // CREATE ok (delegates to the stub DAL)
    let (c, _, b) = http("POST", "/domains", hdrs, "{\"name\":\"newdomain\",\"comment\":\"fresh\",\"label\":\"New\",\"ownedBy\":\"wren\"}");
    assert_eq!(c, 201, "{}", b);
    assert!(b.contains("created newdomain via DAL (ownedBy wren)"), "{}", b);
    // CREATE conflict: entity already exists
    let (c, _, b) = http("POST", "/domains", hdrs, "{\"name\":\"pulse\",\"comment\":\"x\"}");
    assert_eq!(c, 409);
    assert!(b.contains("entity already exists"), "{}", b);
    // CREATE without a name → 422
    let (c, _, b) = http("POST", "/domains", hdrs, "{\"comment\":\"x\"}");
    assert_eq!(c, 422);
    assert!(b.contains("create requires a 'name'"), "{}", b);
    // CREATE with an off-model property → 422 (closed shape)
    let (c, _, b) = http("POST", "/domains", hdrs, "{\"name\":\"n2\",\"evil\":\"x\"}");
    assert_eq!(c, 422);
    assert!(b.contains("off-model property 'evil'"), "{}", b);
    // CREATE that the DAL refuses on the floor → 422 shape-violation
    let (c, _, b) = http("POST", "/domains", hdrs, "{\"name\":\"shapefail\",\"label\":\"L\"}");
    assert_eq!(c, 422);
    assert!(b.contains("shape-violation"), "{}", b);
    // REPLACE ok (pulse ownedBy wren, exists)
    let (c, _, b) = http("PUT", "/domains/pulse", hdrs, "{\"comment\":\"rewritten\",\"label\":\"P2\"}");
    assert_eq!(c, 200, "{}", b);
    assert!(b.contains("replaced pulse via DAL"), "{}", b);
    // REPLACE with empty body → 422
    let (c, _, b) = http("PUT", "/domains/pulse", hdrs, "{}");
    assert_eq!(c, 422);
    assert!(b.contains("at least one shape property"), "{}", b);
    // REPLACE a node with no ownedBy on record → 403 fail-closed
    let (c, _, b) = http("PUT", "/domains/ghost", hdrs, "{\"comment\":\"x\"}");
    assert_eq!(c, 403);
    assert!(b.contains("only the owning role"), "{}", b);
    // REPLACE owned-but-absent → 404
    let (c, _, b) = http("PUT", "/domains/phantom", hdrs, "{\"comment\":\"x\"}");
    assert_eq!(c, 404);
    assert!(b.contains("entity does not exist"), "{}", b);
    // EDGE add ok
    let (c, _, b) = http("POST", "/domains/pulse/partof", hdrs, "{\"target\":\"loom\"}");
    assert_eq!(c, 200);
    assert!(b.contains("add-edge pulse partof -> loom (via DAL)"), "{}", b);
    // EDGE add on single-valued partOf when a parent exists → 409
    let (c, _, b) = http("POST", "/domains/hasparent/partof", hdrs, "{\"target\":\"loom\"}");
    assert_eq!(c, 409);
    assert!(b.contains("partOf is single-valued"), "{}", b);
    // EDGE remove ok
    let (c, _, b) = http("DELETE", "/domains/pulse/contains", hdrs, "{\"target\":\"alpha\"}");
    assert_eq!(c, 200);
    assert!(b.contains("remove-edge pulse contains -> alpha"), "{}", b);
    // EDGE unknown type → 422; missing target → 422
    let (c, _, b) = http("POST", "/domains/pulse/bogus", hdrs, "{\"target\":\"x\"}");
    assert_eq!(c, 422);
    assert!(b.contains("unknown edge type"), "{}", b);
    let (c, _, b) = http("POST", "/domains/pulse/partof", hdrs, "{}");
    assert_eq!(c, 422);
    assert!(b.contains("missing 'target'"), "{}", b);
    // DELETE ok + DAL hard failure → 502 typed dal error
    let (c, _, b) = http("DELETE", "/domains/pulse", hdrs, "");
    assert_eq!(c, 200);
    assert!(b.contains("deleted pulse (via DAL)"), "{}", b);
    let (c, _, b) = http("DELETE", "/domains/dalboom", hdrs, "");
    assert_eq!(c, 502);
    assert!(b.contains("\"error\": \"dal\"") && b.contains("kaboom"), "{}", b);
    // oversized body → 422 cap message
    let big = format!("{{\"name\":\"n3\",\"comment\":\"{}\"}}", "x".repeat(athena_make::MAX_WRITE_BYTES));
    let (c, _, b) = http("POST", "/domains", hdrs, &big);
    assert_eq!(c, 422);
    assert!(b.contains("exceeds"), "{}", b);
}

#[test]
fn testresult_batch_reuses_auth_prepares_all_and_delegates_one_exact_ndjson_call() {
    let w = world();
    let tok = mint_token(WREN_WEBID, None);
    let auth = bearer(&tok);
    let valid_single = r#"{"name":"tr-auth","filePath":"platform/a.rs","testName":"a","result":"pass","ofTest":"test-a"}"#;
    let valid_batch = format!("[{}]", valid_single);

    // Class dispatch happens before the normal non-GET seam, so single and
    // batch create receive byte-for-byte identical authn/scope refusals.
    let (single_code, _, single_body) = http("POST", "/testresults", &[], valid_single);
    let (batch_code, _, batch_body) = http("POST", "/testresults/batch", &[], &valid_batch);
    assert_eq!((batch_code, &batch_body), (single_code, &single_body));
    assert_eq!(batch_code, 401);
    assert!(batch_body.contains("authn-missing"), "{}", batch_body);

    let out_of_scope = [("Authorization", auth.as_str()), ("x-target-graph", "urn:other:graph")];
    let (single_code, _, single_body) = http("POST", "/testresults", &out_of_scope, valid_single);
    let (batch_code, _, batch_body) = http("POST", "/testresults/batch", &out_of_scope, &valid_batch);
    assert_eq!((batch_code, &batch_body), (single_code, &single_body));
    assert_eq!(batch_code, 403);
    assert!(batch_body.contains("out-of-scope"), "{}", batch_body);

    let decoy_scope = [("Authorization", auth.as_str()), ("x-target-graph", "urn:chorus:instances")];
    let (single_code, _, single_body) = http("POST", "/testresults", &decoy_scope, valid_single);
    let (batch_code, _, batch_body) = http("POST", "/testresults/batch", &decoy_scope, &valid_batch);
    assert_eq!((batch_code, &batch_body), (single_code, &single_body));
    assert_eq!(batch_code, 403);
    assert!(batch_body.contains("does not match this class's write graph"), "{}", batch_body);

    // A role-bearing Principal whose final graph grant is absent/revoked must
    // fail closed. Neither class route may treat an empty resolved scope as a
    // legacy allow-all state.
    let unscoped_auth = bearer(&mint_token(UNSCOPED_ROLE_WEBID, None));
    let unscoped_headers = [("Authorization", unscoped_auth.as_str())];
    let (single_code, _, single_body) =
        http("POST", "/testresults", &unscoped_headers, valid_single);
    let (batch_code, _, batch_body) =
        http("POST", "/testresults/batch", &unscoped_headers, &valid_batch);
    assert_eq!((batch_code, &batch_body), (single_code, &single_body));
    assert_eq!(batch_code, 403);
    assert!(batch_body.contains("not in this token's scope"), "{}", batch_body);

    // A registered and scoped Principal without holdsRole may authenticate,
    // but cannot create ownerless records. Single and batch share the refusal.
    let nobody_auth = bearer(&mint_token(ROLELESS_SCOPED_WEBID, None));
    let nobody_headers = [("Authorization", nobody_auth.as_str())];
    let (single_code, _, single_body) = http("POST", "/testresults", &nobody_headers, valid_single);
    let (batch_code, _, batch_body) = http("POST", "/testresults/batch", &nobody_headers, &valid_batch);
    assert_eq!((batch_code, &batch_body), (single_code, &single_body));
    assert_eq!(batch_code, 403);
    assert!(batch_body.contains("model-resolved role"), "{}", batch_body);

    let hdrs = [("Authorization", auth.as_str())];
    std::fs::write(&w.dal_batch_log, "").unwrap();

    // JSON-array framing is typed before preparation. The splitter must not
    // mistake punctuation inside strings for item boundaries.
    for (body, want) in [
        ("{}", "JSON array"),
        ("[42]", "item 1 must be a JSON object"),
        ("[{\"name\":\"x\"},]", "trailing comma"),
        ("[{\"name\":\"x\" \"filePath\":\"a.rs\"}]", "separated by"),
    ] {
        let (code, _, response) = http("POST", "/testresults/batch", &hdrs, body);
        assert_eq!(code, 422, "{}", response);
        assert!(response.contains(want), "want {:?}: {}", want, response);
    }
    let malformed_single = r#"{"name":"x" "filePath":"a.rs"}"#;
    let (code, _, response) = http("POST", "/testresults", &hdrs, malformed_single);
    assert_eq!(code, 422, "{}", response);
    assert!(response.contains("separated by"), "{}", response);

    // The same prepare_create rejects the same defect on both surfaces. Batch
    // names the first failing item and makes no partial DAL call.
    let invalid_single = r#"{"name":"tr-invalid","filePath":"a.rs","evil":"x"}"#;
    let (single_code, _, single_body) = http("POST", "/testresults", &hdrs, invalid_single);
    let invalid_batch = format!("[{},{}]", valid_single, invalid_single);
    let (batch_code, _, batch_body) = http("POST", "/testresults/batch", &hdrs, &invalid_batch);
    assert_eq!(single_code, 422, "{}", single_body);
    assert_eq!(batch_code, single_code, "{}", batch_body);
    assert!(single_body.contains("off-model property 'evil'"), "{}", single_body);
    assert!(batch_body.contains("batch item 2: off-model property 'evil'"), "{}", batch_body);
    assert_eq!(std::fs::read_to_string(&w.dal_batch_log).unwrap(), "", "validation must delegate zero writes");

    let conflict_batch = format!(
        "[{},{{\"name\":\"testresult-existing\",\"filePath\":\"b.rs\",\"testName\":\"b\",\"result\":\"pass\",\"ofTest\":\"test-b\"}}]",
        valid_single,
    );
    let (code, _, body) = http("POST", "/testresults/batch", &hdrs, &conflict_batch);
    assert_eq!(code, 409, "{}", body);
    assert!(body.contains("test-result:testresult-existing") && body.contains("already-exists"), "{}", body);
    let conflict_log = std::fs::read_to_string(&w.dal_batch_log).unwrap();
    assert_eq!(conflict_log.matches("ARGV\tadd-batch").count(), 1, "the governed DAL owns minted-identity conflict checks");

    std::fs::write(&w.dal_batch_log, "").unwrap();
    let duplicate = format!("[{},{}]", valid_single, valid_single);
    let (code, _, body) = http("POST", "/testresults/batch", &hdrs, &duplicate);
    assert_eq!(code, 409, "{}", body);
    assert!(body.contains("batch item 2: duplicate entity name 'tr-auth'"), "{}", body);
    assert_eq!(std::fs::read_to_string(&w.dal_batch_log).unwrap(), "", "duplicate must delegate zero writes");

    let oversized = format!(
        "[{{\"name\":\"tr-big\",\"filePath\":\"{}\",\"testName\":\"big\",\"result\":\"pass\",\"ofTest\":\"test-big\"}}]",
        "x".repeat(athena_make::MAX_WRITE_BYTES),
    );
    let (code, _, body) = http("POST", "/testresults/batch", &hdrs, &oversized);
    assert_eq!(code, 422, "{}", body);
    assert!(body.contains("exceeds 65536-byte cap"), "{}", body);
    assert_eq!(std::fs::read_to_string(&w.dal_batch_log).unwrap(), "", "oversize must delegate zero writes");

    // A normal single create still succeeds through the same prepared fields.
    let (code, _, body) = http(
        "POST",
        "/testresults",
        &hdrs,
        r#"{"name":"tr-single-parity","filePath":"single.rs","testName":"single","result":"pass","ofTest":"test-single"}"#,
    );
    assert_eq!(code, 201, "{}", body);

    // Two validated entities cross the DAL boundary exactly once. No entity
    // data appears in argv; stdin is strict WriteReq-like NDJSON in input order.
    let success = r#"[
      {"name":"tr-batch-a","filePath":"platform/a.rs","testName":"keeps },{ and \"quoted\"","result":"pass","ofTest":"test-a"},
      {"name":"tr-batch-b","filePath":"platform/b.rs","testName":"second","result":"fail","ofTest":"test-b"}
    ]"#;
    let (code, _, body) = http("POST", "/testresults/batch", &hdrs, success);
    assert_eq!(code, 201, "{}", body);
    assert!(body.contains("created 2 testresults via one DAL batch (ownedBy wren)"), "{}", body);

    let line_a = r#"{"kind":"test-result","name":"tr-batch-a","fields":{"ownedBy":"wren","filePath":"platform/a.rs","result":"pass","testName":"keeps },{ and \"quoted\""},"more_values":[],"edges":[["ofTest","test","test-a"]],"graph":"urn:chorus:domains:tests"}"#;
    let line_b = r#"{"kind":"test-result","name":"tr-batch-b","fields":{"ownedBy":"wren","filePath":"platform/b.rs","result":"fail","testName":"second"},"more_values":[],"edges":[["ofTest","test","test-b"]],"graph":"urn:chorus:domains:tests"}"#;
    let expected = format!("ARGV\tadd-batch\nTOKEN\t{}\nSTDIN\n{}\n{}\nEND\n", tok, line_a, line_b);
    let log = std::fs::read_to_string(&w.dal_batch_log).unwrap();
    assert_eq!(log, expected, "exact argv/stdin contract; one ARGV marker means one DAL process");
    assert_eq!(log.matches("ARGV\t").count(), 1, "batch must delegate once");
}

#[test]
fn negative_proof_3774_write_path_wired_to_a_retired_dal_fails_loudly() {
    // #3734 negative proof for #3774: the guarded condition VIOLATED — the
    // write path shelling to a DAL that answers like the retired chorus-model
    // stub (#3718: retirement message, exit 1). The door must surface that as
    // a LOUD typed refusal carrying the stub's message, never a 2xx. This is
    // the exact state production sat in from #3718 until #3774: every doored
    // write hit the stub, werk-test's wire-back read the refusals as
    // 1208/1208 failed case-posts, and the tests domain froze at Aug 3.
    let tok = mint_token(WREN_WEBID, None);
    let auth = bearer(&tok);
    let hdrs: &[(&str, &str)] = &[("Authorization", &auth)];
    let (c, _, b) = http("POST", "/domains", hdrs, "{\"name\":\"retiredstub\",\"comment\":\"x\"}");
    assert_eq!(c, 502, "retired-stub DAL answer must refuse loudly, got {}: {}", c, b);
    assert!(b.contains("RETIRED (#3718)"), "refusal must carry the stub's message: {}", b);
}

#[test]
fn batch_route_is_gated_and_delegates_typed_slots() {
    // 401 without a token
    let (c, _, b) = http("POST", "/batch", &[], "INS\turn:s\turn:p\to");
    assert_eq!(c, 401);
    assert!(b.contains("authn-missing"), "{}", b);
    // #3689 — a principal with NO hasScope grants → 403 (batch REQUIRES scope;
    // scope is model data, so "unscoped" means "granted nothing").
    let tok = mint_token(NOBODY_WEBID, None);
    let (c, _, b) = http(
        "POST",
        "/batch",
        &[("Authorization", &bearer(&tok)), ("x-target-graph", "urn:test:instances")],
        "INS\turn:s\turn:p\to",
    );
    assert_eq!(c, 403);
    assert!(b.contains("scoped token"), "{}", b);
    // Scope alone is not write authority: a scoped Principal without a
    // model-resolved holdsRole edge is refused before delegation.
    let tok = mint_token(ROLELESS_SCOPED_WEBID, None);
    let (c, _, b) = http(
        "POST",
        "/batch",
        &[("Authorization", &bearer(&tok)), ("x-target-graph", "urn:test:instances")],
        "INS\turn:s\turn:p\to",
    );
    assert_eq!(c, 403);
    assert!(b.contains("model-resolved role"), "{}", b);
    // scoped + in-scope: applies via the DAL
    let tok = mint_token(WREN_WEBID, Some(&["urn:test:instances"]));
    let auth = bearer(&tok);
    let hdrs: &[(&str, &str)] = &[("Authorization", &auth), ("x-target-graph", "urn:test:instances")];
    let (c, _, b) = http("POST", "/batch", hdrs, "DEL\turn:s\turn:p\t?o\nINS\turn:s\turn:p\tv2\n");
    assert_eq!(c, 200);
    assert!(b.contains("batch applied: 1 del, 1 ins -> <urn:test:instances> (via DAL)"), "{}", b);
    // malformed line / bad op / empty body → typed 422s
    let (c, _, b) = http("POST", "/batch", hdrs, "INS\tonly\ttwo");
    assert_eq!(c, 422);
    assert!(b.contains("OP<tab>S<tab>P<tab>O"), "{}", b);
    let (c, _, b) = http("POST", "/batch", hdrs, "UPS\ta\tb\tc");
    assert_eq!(c, 422);
    assert!(b.contains("DEL or INS"), "{}", b);
    let (c, _, b) = http("POST", "/batch", hdrs, "\n\n");
    assert_eq!(c, 422);
    assert!(b.contains("no DEL/INS lines"), "{}", b);
}

// ───────────────────────── direct handle() coverage ────────────────────────

#[test]
fn effective_config_read_resolves_and_coerces() {
    let w = world();
    let (c, b) = athena_make::handle("/effective/pulse/alert.threshold", &w.domain);
    assert_eq!(c, 200, "{}", b);
    assert!(b.contains("\"value\":42"), "typed int, not a string: {}", b);
    assert!(b.contains("\"winningScope\""), "{}", b);
    // unset key → 404
    let (c, b) = athena_make::handle("/effective/pulse/nope.key", &w.domain);
    assert_eq!(c, 404);
    assert!(b.contains("no property sets key"), "{}", b);
    // hygiene guards
    let (c, _) = athena_make::handle("/effective/bad$node/k", &w.domain);
    assert_eq!(c, 400);
    let (c, b) = athena_make::handle("/effective/pulse/bad$key", &w.domain);
    assert_eq!(c, 400);
    assert!(b.contains("invalid key"), "{}", b);
    // handle_meta /health short-circuit
    let ((c, b), meta) = athena_make::handle_meta("/health", &w.domain, false);
    assert_eq!(c, 200);
    assert!(b.contains("\"ok\": true"));
    assert_eq!(meta.route, "health");
    // unknown route lists the generated routes
    let (c, b) = athena_make::handle("/nonsense", &w.domain);
    assert_eq!(c, 404);
    assert!(b.contains("GET /domains"), "{}", b);
}

#[test]
fn emitters_project_the_same_model() {
    let w = world();
    let dash = athena_make::dashboards_json(&w.domain);
    assert!(dash.contains("\"uid\": \"athena-make-domain\""), "{}", dash);
    assert!(dash.contains("silent-broken-chain watch"), "{}", dash);
    let tm = athena_make::tests_manifest(&w.domain);
    assert!(tm.contains("\"unauth-create-401\""), "{}", tm);
    assert!(tm.contains("\"secured-401 /schema/domain\""), "{}", tm);
    assert!(tm.contains("edge-target-type-reject ownedBy"), "{}", tm);
    let mcp = athena_make::mcp_binding(&w.domain);
    assert!(mcp.contains("chorus_domains_list"), "{}", mcp);
    assert!(mcp.contains("\"delegatesTo\": \"DAL (athena-model)\""), "{}", mcp);
    let page = athena_make::page_html(&w.domain);
    assert!(page.contains("window.OWL_CLASS = \"Domain\""), "{}", page);
    assert!(page.contains("domain-renderer.js"), "Domain keeps the rich renderer: {}", page);
    let page_p = athena_make::page_html(&w.product);
    assert!(page_p.contains("entity-renderer.js"), "other classes use the generic renderer: {}", page_p);
    let dal = athena_make::dal_skeleton_ts(&w.domain, &["urn:test:instances".to_string()]);
    assert!(dal.contains("export const SCOPE: string[]"), "{}", dal);
    assert!(dal.contains("\"urn:test:instances\""), "{}", dal);
    assert!(dal.contains("export async function writeDomain("), "{}", dal);
    assert!(dal.contains("const REQUIRED: string[] = [\n  \"comment\"\n];"), "floor projected: {}", dal);
    let rj = athena_make::routes_json(&w.domain);
    assert!(rj.contains("\"mandatory\": [\"comment\"]"), "{}", rj);
    // error_envelope covers the non-404 titles too
    let e = athena_make::error_envelope(&w.domain, "pulse", 429, "rate", "slow down", &[("comment".into(), "too long".into())]);
    assert!(e.contains("\"title\": \"Too Many Requests\""), "{}", e);
    assert!(e.contains("\"errors\": [{ \"field\": \"comment\", \"detail\": \"too long\" }]"), "{}", e);
    let e = athena_make::error_envelope(&w.domain, "x", 599, "weird", "d", &[]);
    assert!(e.contains("\"title\": \"Error\""), "{}", e);
}
