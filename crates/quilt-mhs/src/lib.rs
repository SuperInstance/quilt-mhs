//! # quilt-mhs
//!
//! Bridge the quilt ecosystem to Anthropic's Model Hardware Standard
//! (MHS, announced 2026-08-27, research preview; spec/SDK not yet public).
//!
//! Two halves, one seam:
//!
//! - **Controller side** ([`controller::QuiltMhsAdapter`]): quilt cells
//!   and effects drive MHS devices. The 5+1 quilt opcodes (BIND / LINK /
//!   EFFECT / VIEW / TICK + FORGET) map onto the announced MHS surface —
//!   read/write primitives, discovery, reference-file manifests, code
//!   files, and a safety-teardown path.
//! - **Substrate side** ([`device::QuiltDeviceProfile`]): a quilt runtime
//!   exposed AS an MHS-addressable machine, so other agents and other
//!   quilts operate quilt substrates through MHS-shaped messaging
//!   (intra- and inter-quilt; see [`device::federation`]).
//!
//! Everything runs TODAY against [`mhs::mock::MockMHS`] — no hardware, no
//! real SDK. The [`mhs::client::MhsClient`] trait is the only seam that
//! changes when Anthropic ships the real standard; the
//! [`mhs::conformance`] suite is the contract any new transport must
//! pass. Assumptions made from press coverage are catalogued in
//! `MHS-SPEC-WATCH.md` (repo root).

pub mod controller;
pub mod device;
pub mod mhs;

pub use controller::QuiltMhsAdapter;
pub use device::{QuiltDeviceProfile, SheetCellKind, SheetRuntime};
pub use mhs::client::MhsClient;
pub use mhs::mock::MockMHS;
