<<<<<<< HEAD
<<<<<<< HEAD
// @domain: domains
=======
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
=======
// @domain: domains
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
//! athena-gen — #4490, athena-make v2 step 1.
//!
//! Reads the model the repo authors and writes one Rust API crate per product.
//! Shares no source with athena-make (v1): the freeze rule in #4478 keeps v1 to
//! break fixes, and v2 grows here.

pub mod model;
<<<<<<< HEAD
<<<<<<< HEAD
pub mod emit;
=======
>>>>>>> a0f8b1ded (wren: #4490 — athena-gen reads a product's slice of the model from TTL (in-memory Oxigraph, no Fuseki): domains, classes, fields, sh:in in order, v1 path segments; 4 tests incl. 2 negative proofs)
=======
pub mod emit;
>>>>>>> 3795eee59 (wren: #4490 — athena-gen generate writes the pulse-api crate (RowStore trait, remote SPARQL backend, settings env then file then default, versioned envelope); pulse-api compiles and serves the 5 pulse collections read-only on :3392; emitter tests incl. same-model-same-bytes and a model-change negative proof)
