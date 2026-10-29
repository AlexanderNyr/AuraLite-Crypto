//! `aura-node` — the `aurad` node binary.
//!
//! C0 created this crate as an empty skeleton. **D0** gives it its first real
//! content: the [`baseline`] module, which measures what the tree contains and makes
//! the `aurad` binary refuse to pretend it is a node. The node itself (config,
//! tracing, tx pipeline, RPC) still does not exist — that is D1 onwards, and
//! [`baseline::MISSING_CAPABILITIES`] names every phase that owes it.
//!
//! There is no fake `Ok(())` here: `aurad` exits non-zero and says why.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod baseline;
