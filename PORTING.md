# PORTING.md — porting quilt to MHS, and MHS to everything else

You are reading this because either (a) the real Anthropic MHS spec/SDK just
landed and you want quilt on it, or (b) you have some *other* MHS-shaped
system and you want the quilt ecosystem to talk to it. Both are the same
job, and it is deliberately small: **this repo is built so that everything
that can change lives behind two ports.**

- MHS announced 2026-08-27 as a research preview; open source planned, no
  date (Anthropic, Reuters, CNBC — full source table in
  [MHS-SPEC-WATCH.md](MHS-SPEC-WATCH.md)).
- As of that date there is **no public spec, SDK, schema, or conformance
  suite** — everything here runs against the *announced shape* plus
  clearly-tagged assumptions (A-1..A-10 in SPEC-WATCH).

## The mental model

```
        quilt cells/effects                    other agents / fleets
                │                                       │
     ┌──────────▼──────────┐              ┌─────────────▼─────────────┐
     │ QuiltMhsAdapter     │              │  any MHS client           │
     │ (controller side)   │              │  (Claude, another quilt)  │
     └──────────┬──────────┘              └─────────────┬─────────────┘
                │  MhsClient trait (PORT 1)             │
     ┌──────────▼───────────────────────────────────────▼─────────────┐
     │  transport: MockMHS today · real SDK / MCP / CLI tomorrow       │
     └──────────┬───────────────────────────────────────┬─────────────┘
                │                                       │
     ┌──────────▼──────────┐              ┌─────────────▼─────────────┐
     │ physical machines   │              │ QuiltDeviceProfile        │
     │ (arms, baths, …)    │              │ (substrate side, PORT 2): │
     └─────────────────────┘              │ a quilt runtime AS an MHS │
                                          │ device — cells are the    │
                                          │ addressable resources     │
                                          └───────────────────────────┘
```

- **PORT 1 — `MhsClient`** (`crates/quilt-mhs/src/mhs/client.rs`): what the
  quilt controller needs from any MHS-shaped system: discover, manifest,
  read, write, run_program, abort (+ grant/poll defaults).
- **PORT 2 — `QuiltDeviceProfile`** (`crates/quilt-mhs/src/device/mod.rs`):
  a quilt runtime *served as* an MHS device. It implements PORT 1 over
  itself, so an adapter can drive it with zero plumbing (intra-quilt), and
  `FederationLink` (`device/federation.rs`) is the transport handle between
  two runtimes (inter-quilt).

## The 5+1 opcode mapping (why each row is what it is)

| quilt opcode | MHS surface | Why this mapping |
|---|---|---|
| `BIND(name, value)` / `bind_to_device` | manifest lookup; cell ↔ channel registration | A bound cell **is** an addressable resource; the manifest (MHS "reference file") is the type system the binding checks against. |
| `LINK(a, b)` | cell-to-device graph edges | Links carry propagation: `sensor → formula → actuator` is three cells and **one** device write. Transitivity gives multi-hop chains to devices. |
| `EFFECT(target, v)` | `write(device, channel, v)` | "write" is an announced MHS primitive (e.g. "set temperature"). The **device** enforces limits — the adapter never second-guesses rejections. |
| `VIEW(target)` | `read(device, channel)` | "read" is the other announced primitive (e.g. "get temperature"). Reads are pure: no state change, no clock advance. |
| `TICK(dt)` | `poll(dt)` — the machine's control-loop step | The announcement's devices run their own loops (code files execute without the agent); TICK drains the transport and advances the quilt clock, monotonically. |
| `FORGET(x)` | `abort(device)` + grant release | **The 6th opcode maps to MHS's safety story.** Interlocks are first-class forgettable state: forgetting the last grant-holding cell on a device parks it. |

## The 5+1+1 laws ↔ MHS safety semantics

(Law numbering follows quilt-cellular-arch `INDEX.md`; FRAMEWORK.md calls
super-relevance the 6th and FORGET the +1 — same seven facts.)

| Law | MHS meaning | Where it's enforced |
|---|---|---|
| BIND_idempotence | registering the same cell↔channel twice is one registration | `bind`/`bind_to_device`; `tests/laws.rs::bind_is_idempotent` |
| LINK_transitivity | chains through the cell graph reach device-bound tails | `link_closure`, propagation; `links_are_transitive`, `effect_propagates_across_links_to_device` |
| EFFECT_associativity | batching writes in any grouping lands identical writes in identical order | `effect_batch`; `effect_grouping_is_invariant` |
| VIEW_purity | telemetry reads never mutate the machine, never advance time | `view`; `view_is_pure` (+ conformance C6) |
| TICK_monotonicity | the control loop only moves forward | `tick`/`poll`; `tick_is_monotonic`, mock returns `NotMonotonic` |
| Super-relevance | channels serving more hands (controllers) rank higher — scheduling fitness across agents | `channel_relevance`/`ranked_channels`; `super_relevance_ranks_multi_hand_channels` |
| FORGET_completeness | teardown removes every trace: links, bindings, grants; last-interlock-out parks the machine | `forget` + `ForgetReceipt`; `forget_is_complete` |

The safety envelope also maps to our forking rules directly: **no
destructive op without an explicit grant**, and the grant is revocable by
FORGET — interlocks are forgettable state, not ambient permission.

## The conformance-test contract

`mhs::conformance::run_conformance(&mut dyn MhsClient)` runs C1..C9 against
*any* implementation:

- C1 discovery returns devices
- C2 manifest declares channels + enforced limits
- C3 write returns a sample
- C4 read reflects the written value
- C5 **out-of-limit write is rejected AND state is unchanged** (the check
  that catches "helpful" clamping transports — see
  `tests/conformance.rs::conformance_catches_a_lying_transport`)
- C6 reads are pure/repeatable
- C7 chained program (code file) completes
- C8 (assumption A-6) abort latches against further writes
- C9 (assumption A-9) abort parks writable channels at the range floor

Checks tagged as assumptions are reported separately (`core_passes` ignores
them) so a real SDK can disagree with an *assumption* without failing the
core contract. This is quilt-rust's compat pattern (one contract, golden
tests, many substrates) applied at the machine boundary.

**Your port is done when:** `run_conformance(&mut YourClient)` passes core
checks, and `tests/laws.rs` passes unchanged (the laws must not depend on
the transport).

## Swapping the transport when the real SDK lands

Design review (2026-08-27, independent pass) sharpened this section:
there are **two seams, not one**. PORT 1 (`MhsClient`) is the behavioral
seam; `mhs/types.rs` is the vocabulary seam. The real SDK ships its own
manifest/command/sample types — reconciliation happens in `types.rs`
(newtype wrappers around SDK types) plus a `gen-schemas` diff. Plan for
both files changing and nothing else.

1. Add the official SDK as an optional dependency (feature `mhs-official`).
2. Write ONE file: `struct OfficialSdkClient { ... }` implementing
   `MhsClient` by delegating to the SDK. Map any vocabulary differences
   here and only here (A-1..A-10 in MHS-SPEC-WATCH.md name every spot).
3. `cargo test --test conformance` — run the suite against your client:
   ```rust
   let results = quilt_mhs::mhs::conformance::run_conformance(&mut OfficialSdkClient::new());
   assert!(quilt_mhs::mhs::conformance::core_passes(&results));
   ```
4. `cargo run --bin gen-schemas` — diff `schemas/` against the official
   schema; reconcile `mhs/types.rs` field names, regenerate.
5. Run `tests/laws.rs` + `tests/federation.rs`. If the laws pass, quilt's
   controller stack works unchanged.
6. Update `MHS-SPEC-WATCH.md` (diff-day procedure is written there).

If the official SDK is MCP-carried (one of the three announced mechanisms:
MCP, CLI, code files), the same steps apply — your `MhsClient` impl speaks
MCP tool-calls instead of SDK calls. The adapter never knows.

## Porting to OTHER MHS-shaped systems (not Anthropic's)

Same story, smaller stakes. Implement `MhsClient` over your system's API
(conformance suite green ⇒ you're compatible with everything above the
port), or implement the device side (`QuiltDeviceProfile`'s five-method
surface: define/set/get/tick + manifest) to *expose* your system as an
MHS-addressable substrate. Intra-quilt (sheets driving sheets through the
enforced seam) and inter-quilt (two runtimes federation-testing each other)
come along for free — `tests/federation.rs` is the executable spec.

## Known limitations (called out, not hidden)

- **Synchronous core, by design.** Everything here is sync (quilt-rust's
  async-at-the-boundary philosophy). Real async transports introduce
  TOCTOU windows the sync design does not model (two controllers racing
  a shared channel; an in-flight write landing after an abort). When a
  real transport lands, ordering/lease semantics for concurrent writers
  get decided there and pinned by a new conformance check — flagged in
  MHS-SPEC-WATCH as follow-up, not silently assumed away.
- **Links are persistent graph edges** (quilt semantics); there is no
  auto-expiring/ephemeral link. A transient coupling is LINK + EFFECT +
  FORGET — three explicit opcodes, no hidden temporal decay.
- **Grants are multi-holder by design**: several cells may share one
  interlock; each FORGET releases its holder independently; the last
  holder out parks the device (`tests/laws.rs::grants_are_multi_holder_and_visible`).

## Operator notes: when a device aborts

- A rejected program step aborts every device the program touched and
  returns a `ProgramReceipt { accepted_steps, abort_reason }` — partial
  application is reported, never hidden.
- Post-abort, writes are refused (`MhsError::Aborted`) until an operator
  clears the latch (`MockMHS::clear_abort` /
  `QuiltDeviceProfile::clear_abort`). Agents do not unlatch their own
  aborts — that is the point.
- Abort parks ranged writable channels at the contract floor (A-9);
  conformance C9 checks this.

## Regulatory note (EU)

TNW (2026-08-27) flags that EU Machinery Regulation 2023/1230, applying
2027-01-20, covers AI-based safety functions, and an MHS file constraining
a machine may count as a regulated safety component. If you deploy a
`QuiltDeviceProfile` safety envelope in the EU after that date, treat
`schemas/mhs-safety-envelope.schema.json` artifacts as potentially
conformity-relevant documents. Source: [MHS-SPEC-WATCH.md F19].
