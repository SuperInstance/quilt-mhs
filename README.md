# quilt-mhs — the quilt ecosystem × Anthropic's Model Hardware Standard

> **A quilt cell is an addressable resource. An MHS device is an addressable
> resource. This repo is the two adapters that make those the same sentence.**

Anthropic announced the **Model Hardware Standard (MHS)** on 2026-08-27: a
shared spec for AI agents to safely operate physical devices — microscopes,
liquid handlers, robotic arms — "USB-C" style ([CNBC, 2026-08-27][cnbc]).
The real spec/SDK is **not public yet** (research preview; open source
planned, no date). So this repo does the part you can do before a spec
ships: it builds the bridge against the *announced shape*, isolates every
guess behind ports, and ships a conformance suite so the real SDK drops in
later without touching the quilt side. Every public claim we relied on is
sourced in [MHS-SPEC-WATCH.md](MHS-SPEC-WATCH.md); every guess is tagged.

**What runs today, with zero hardware and zero SDK:**

- a quilt→MHS **controller adapter**: the 5+1 quilt opcodes (BIND / LINK /
  EFFECT / VIEW / TICK + FORGET) drive MHS devices through the announced
  surface (discover / read / write / code files / abort), with safety
  interlocks as first-class *forgettable* state;
- a quilt-as-MHS-device **substrate profile**: a quilt runtime exposed as an
  MHS-addressable machine — cells are the channels, cell contracts are the
  enforced limits — so other agents (Claude, other fleets, other quilts)
  can operate quilt substrates through MHS-shaped messaging;
- **inter-quilt federation**: two quilt runtimes operating each other
  through the same MHS-shaped messages an external agent would use;
- a **conformance suite** any MHS transport must pass — including a test
  that a lying transport (clamps instead of rejecting) *fails*;
- generated, drift-checked **JSON schemas** for the manifest / safety
  envelope / command / telemetry surfaces.

## 30 seconds

```rust
use quilt_mhs::{QuiltMhsAdapter, MockMHS, MhsClient};
use quilt_mhs::mhs::types::*;

let mut quilt = QuiltMhsAdapter::new(MockMHS::new());      // any MhsClient
quilt.bind_to_device("bath", &"mock-thermal-01".into(), "bath.setpoint", &[])?;
quilt.bind_to_device("temp", &"mock-thermal-01".into(), "bath.temperature", &[])?;

quilt.effect("bath", MhsValue::Float(65.0))?;   // EFFECT → device write
let t = quilt.view("temp")?;                    // VIEW  → device telemetry
quilt.tick(10.0)?;                              // TICK  → machine control-loop step

// safety is the device's job, not the agent's politeness:
let err = quilt.effect("bath", MhsValue::Float(150.0)).unwrap_err();
assert!(matches!(err, MhsError::SafetyViolation(..))); // envelope: 0..100

// and FORGET is a safety teardown, not just an unlink:
quilt.forget("bath")?;  // tears down bindings, links, grants — parks machines
```

## Layout

```
crates/quilt-mhs/src/
├── mhs/            PORT 1 — the MHS seam
│   ├── types.rs      manifest/safety/command/telemetry types (A-tagged)
│   ├── client.rs     MhsClient trait (what quilt needs from any MHS)
│   ├── mock.rs       MockMHS: arm + thermal bath, full enforcement, runs today
│   └── conformance.rs  C1..C9 — the porting contract
├── controller/     quilt→MHS adapter (5+1 opcode mapping, interlocks)
└── device/         PORT 2 — quilt-as-MHS-device + inter-quilt federation
schemas/            generated JSON schemas (drift-checked by test)
PORTING.md          the guide for porting to other MHS systems
MHS-SPEC-WATCH.md   every sourced fact, every assumption, diff-day procedure
```

## Tests

`cargo test` — 22 tests, zero warnings:

- `tests/laws.rs` (11) — the 5+1+1 laws enforced through the adapter:
  idempotence, transitivity (+ device propagation), associativity
  (grouping invariance), purity, monotonicity (+ real control-loop
  relaxation), super-relevance ranking, forget-completeness (grants
  released, machine parked and latched), envelope enforcement, interlock
  gating, multi-holder interlocks (explicit + visible).
- `tests/conformance.rs` (3) — MockMHS passes C1..C9; a quilt device
  profile passes the same suite; a clamping transport **fails** C5.
- `tests/federation.rs` (5) — A drives B's sheet over MHS-shaped messages;
  envelope enforced across the link; destructive cells interlock-gated
  across runtimes; FORGET parks the federated runtime; symmetric.
- `tests/schemas.rs` (3) — committed schemas match generated ones
  (drift-proof), round-trips, examples parse.

## Why Rust

quilt-rust is the reference runtime of the ecosystem (single binary, sync
core, `Arc<QuiltEngine>` at the boundary), the 5+1 opcodes already have a
proven C/Rust lineage on metal (quilt-esp32 `qm_opcodes`, verified on an
ESP32-S3 2026-08-26), and the compat-contract pattern this repo leans on —
one contract, conformance tests, many substrates — is already
Rust-native in quilt-rust (`compat/conformance_test.rs` + golden vectors).
The adapter core is std-only plus serde/schemars: no async runtime, no
heavy deps, `no_std`-friendly types. A Python port would have been faster
to start and slower to trust.

## Honest status

- No real MHS spec exists yet; **everything MHS-specific is press-derived
  or assumption-tagged** (A-1..A-10 in MHS-SPEC-WATCH.md).
- MockMHS is the only transport; the MCP/CLI/code-file surfaces named in
  the announcement are ports to fill (PORTING.md has the walkthrough).
- The controller adapter's cell model is the minimal reference shape; the
  intended production path is swapping `SheetRuntime` for quilt-rust's
  `QuiltEngine` behind the same five-method surface.
- Not an Anthropic product, not affiliated; this is an ecosystem adapter
  built the day of the announcement.

## Sources (all 2026-08-27)

- [Anthropic — Previewing the Model Hardware Standard][anth]
- [Reuters — Anthropic unveils new framework allowing AI agents to operate physical devices][reut]
- [CNBC — Anthropic pushes into physical world with new standard][cnbc]
- [TNW — Anthropic tests a new standard for Claude to work with factory and lab hardware][tnw]
- [kingy.ai — Anthropic Model Hardware Standard (MHS) Explained][king]

[anth]: https://www.anthropic.com/news/model-hardware-standard-research-preview
[reut]: https://www.reuters.com/technology/anthropic-unveils-new-framework-allowing-ai-agents-operate-physical-devices-2026-08-27/
[cnbc]: https://www.cnbc.com/2026/08/27/anthropic-pushes-into-physical-world-with-new-standard-to-help-ai-agents-operate-machines.html
[tnw]: https://thenextweb.com/news/anthropic-model-hardware-standard-mhs-eu-machinery-regulation-2027
[king]: https://kingy.ai/blog/anthropic-model-hardware-standard-mhs/

Related quilt repos: [quilt-cellular-arch](https://github.com/SuperInstance/quilt-cellular-arch)
(the 5+1 opcodes, the laws, the framework) · [quilt-rust](https://github.com/SuperInstance/quilt-rust)
(the reference engine this adapter targets) · [quilt-esp32](https://github.com/SuperInstance/quilt-esp32)
(the metal precedent: `qm_opcodes` on S3).

Apache-2.0.
