//! The MHS port layer: types, the client trait, the mock transport, and
//! the conformance suite. Everything in here models the ANNOUNCED MHS
//! surface (2026-08-27 press; see MHS-SPEC-WATCH.md). Nothing outside
//! this module talks about MHS specifics — this is the seam.

pub mod client;
pub mod conformance;
pub mod mock;
pub mod types;
