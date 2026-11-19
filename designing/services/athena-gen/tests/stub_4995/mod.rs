//! #4995 — the in-test SPARQL store both doors read (AC3 equality, AC5 parity): an
//! in-memory oxigraph behind a std TcpListener on port 0, speaking the SPARQL 1.1
//! protocol both clients use (POST {endpoint}/query, raw or form body).
#![allow(dead_code)]
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::NamedNode;
use oxigraph::sparql::results::{QueryResultsFormat, QueryResultsSerializer};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The store both doors read: the whole model in urn:chorus:ontology, the principles
/// domain row in urn:chorus:domains:domains, and the real principle rows (with `edit`
/// one row's label changed when `changed` names it) in urn:chorus:domains:principles (Borg Queen 18:27: the three graphs
/// athena-make serve reads at boot).
pub fn store(changed: Option<&str>) -> Store {
    let s = Store::new().unwrap();
    let load = |ttl: &[u8], g: &str| {
        s.load_from_slice(RdfParser::from_format(RdfFormat::Turtle).with_default_graph(NamedNode::new(g).unwrap()), ttl)
            .unwrap_or_else(|e| panic!("load into {g}: {e}"));
    };
    for f in athena_gen::model::model_files(&repo()) {
        load(&std::fs::read(&f).unwrap(), "urn:chorus:ontology");
    }
    load(b"@prefix c: <https://jeffbridwell.com/chorus#> .\nc:principles a c:Domain ; c:definesVocabulary c:Principle .\n",
        "urn:chorus:domains:domains");
    let rows = std::fs::read_to_string(repo().join("roles/wren/ontology/principles-instances-3749.ttl")).unwrap();
    load(rows.as_bytes(), "urn:chorus:domains:principles");
    if let Some(name) = changed {
        s.update(&format!(
            "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> \
             DELETE {{ GRAPH <urn:chorus:domains:principles> {{ <https://jeffbridwell.com/chorus#{name}> rdfs:label ?l }} }} \
             INSERT {{ GRAPH <urn:chorus:domains:principles> {{ <https://jeffbridwell.com/chorus#{name}> rdfs:label \"changed by the #4995 negative proof\" }} }} \
             WHERE {{ GRAPH <urn:chorus:domains:principles> {{ <https://jeffbridwell.com/chorus#{name}> rdfs:label ?l }} }}"))
            .unwrap();
    }
    s
}

/// Serve `store` over the SPARQL protocol on 127.0.0.1:<port 0>; returns the endpoint.
pub fn serve(store: Store) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let st = store.clone();
            std::thread::spawn(move || answer(c, &st));
        }
    });
    format!("http://127.0.0.1:{port}/pods")
}

fn answer(mut c: TcpStream, store: &Store) {
    let mut r = BufReader::new(c.try_clone().unwrap());
    let (mut len, mut form) = (0usize, false);
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
            break;
        }
        let lower = h.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
        if lower.starts_with("content-type:") && lower.contains("x-www-form-urlencoded") {
            form = true;
        }
    }
    let mut body = vec![0u8; len];
    let _ = r.read_exact(&mut body);
    let body = String::from_utf8_lossy(&body).into_owned();
    let query = if form {
        body.split('&').find_map(|kv| kv.strip_prefix("query=")).map(urldecode).unwrap_or_default()
    } else {
        body
    };
    let (status, out) = match run(store, &query) {
        Ok(b) => ("200 OK", b),
        Err(e) => ("400 Bad Request", e.into_bytes()),
    };
    let _ = write!(c, "HTTP/1.1 {status}\r\nContent-Type: application/sparql-results+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", out.len());
    let _ = c.write_all(&out);
}

fn run(store: &Store, q: &str) -> Result<Vec<u8>, String> {
    let ser = QueryResultsSerializer::from_format(QueryResultsFormat::Json);
    match SparqlEvaluator::new().parse_query(q).map_err(|e| e.to_string())?.on_store(store).execute().map_err(|e| e.to_string())? {
        QueryResults::Solutions(sols) => {
            let mut w = ser.serialize_solutions_to_writer(Vec::new(), sols.variables().to_vec()).map_err(|e| e.to_string())?;
            for s in sols {
                w.serialize(&s.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            }
            w.finish().map_err(|e| e.to_string())
        }
        QueryResults::Boolean(b) => ser.serialize_boolean_to_writer(Vec::new(), b).map_err(|e| e.to_string()),
        QueryResults::Graph(_) => Err("the stub answers SELECT and ASK only".into()),
    }
}

fn urldecode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("00"), 16).unwrap_or(b'?'));
                i += 2;
            }
            x => out.push(x),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

