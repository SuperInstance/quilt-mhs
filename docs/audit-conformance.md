# Conformance & Test-Coverage Audit — `quilt-mhs`

**Date:** 2026-08-27 · **Scope:** conformance contract, law tests, federation, schemas.
**For:** the cowboy. · **Sources:** `mhs/conformance.rs`, `tests/{laws,federation,conformance,schemas}.rs`, `PORTING.md`, `MHS-SPEC-WATCH.md`.

## 1. What exists today — the C1..C9 contract

Defined in `mhs/conformance.rs::run_conformance`, run against any `&mut dyn MhsClient`. Press-sourced checks assert behavior named in F1..F19; assumption-tagged checks (A-n) assert OUR policy and can legitimately be disagreed with by a real SDK.

| ID | Tag | What it asserts |
|---|---|---|
| **C1** | press (F7) | `discover()` returns at least one device |
| **C2** | press (F8) | `manifest(d)` declares ≥1 writable channel and ≥1 enforced `channel_limits` entry |
| **C3** | press (F6) | `write(d, c, v)` echoes the value back as a `Sample` with `value == v` |
| **C4** | press (F6) | `read(d, c)` returns a numeric `Sample` for the just-written channel |
| **C5** | press (F8) | out-of-limit write is rejected with `MhsError::SafetyViolation` AND `read` before == `read` after (the "no lying/clamping transport" check) |
| **C6** | press (F6) | two consecutive `read`s on the same channel return equal `Sample.value` (VIEW_purity at the MHS boundary) |
| **C7** | press (F10) | `run_program` of two in-limit steps returns `completed && accepted_steps == 2` (code file) |
| **C8** | **A-6** | after `abort`, `write` returns `MhsError::Aborted(_)` (operator-clearable latch) |
| **C9** | **A-9** | after `abort`, every ranged writable is parked at its range floor (mock teardown semantics; SDK may specify its own park) |

Outside the C-suite: `laws.rs` has the 5+1+1 law tests (12 funcs); `federation.rs` has 5 inter-quilt tests; `conformance.rs` has 3 (`mock_passes`, `quilt_device_profile_passes`, `conformance_catches_a_lying_transport` — last is the differential witness for C5); `schemas.rs` has 3 (drift detection, round-trip, examples parse). **Total: 23 tests today** (cowboy's 22 undercounts `grants_are_multi_holder_and_visible`).

## 2. What is missing

| Gap | Proposed ID | Status today |
|---|---|---|
| "Lying transport" differential as a **first-class C-check** (not a one-off test in `tests/conformance.rs`) | **C5'** or fold into C5's contract: assertion language should be `core_passes` MUST be `false` when clamping is observed | **exists as test, not as a C-tag.** The clamp-catching logic is in `tests/conformance.rs::LyingClient` — C5 fires correctly, but the test is the contract's only witness; promote it to `run_conformance` itself by adding a `clamp-detector` check (a probe write that lands at exactly the limit and reads back, asserting the unclamped value). |
| `run_program` end-to-end (program runs, device state reflects last step, abort-on-bad-step) | **C10** | **missing.** C7 only checks the receipt of an all-in-limit program. No test drives a program that contains an out-of-limit step and asserts the device aborted and latched. |
| Multi-device interleaving (one controller, two devices; concurrent reads + writes; transport sees ordered ops) | **C11** | **missing.** Conformance picks `devices[0]` and stops; `federation.rs` is symmetric A↔B but does not interleave writes against the same channel while a `read` is in flight, nor verify the journal order. |
| Federation link survives remote abort (B aborts itself; A's next `read` returns `Aborted`; A can re-bind and keep going once B's `clear_abort` runs) | **C12** | **missing.** `federation.rs::forget_parks_the_federated_runtime` covers A-driven abort, but no test exercises B-side autonomous abort and the recovery path. |
| History surfaces monotonic timestamps (every journaled event carries a non-decreasing `t`; `Sample.t` only advances; cross-device ordering is consistent) | **C13** | **partial.** `tick_is_monotonic` covers the adapter clock; nothing asserts `Sample.t` monotonicity or that two devices' clocks in the same transport move in lockstep order. |
| FORGET observable from the journal (a 7th law-level test asserting that every trace the laws can see — `Bind`, `Link`, `Grant`, `Effect`, `View`, `Tick`, `Forget` — is appended exactly once and no orphan remains) | (law-form) | **partial.** `forget_is_complete` checks externally-observable state, but does NOT assert the journal contains `OpEvent::Forget { name }` followed by `OpEvent::ReleaseGrant` and (if last holder) `OpEvent::Abort`. A 1-page audit can ship this; pure journal-asserting test. |

## 3. Code sketches for each missing check (5 lines each)

**C10 — `run_program` aborts on bad step, device latches.** Place in `tests/conformance.rs` (or fold into `run_conformance` as a C10):

```rust
let program = vec![
    Command { device: d.clone(), channel: "joint1.target".into(), value: MhsValue::Float(0.0) },
    Command { device: d.clone(), channel: "joint1.target".into(), value: MhsValue::Float(120.0) }, // out of ±90
];
let r = client.run_program(program).unwrap();
assert!(!r.completed && r.abort_reason.is_some());
assert!(matches!(client.write(&d, "joint1.target", MhsValue::Float(0.0)).unwrap_err(), MhsError::Aborted(_)));
```

**C11 — multi-device interleaving.** Place in `tests/conformance.rs`; uses two devices from one transport:

```rust
let dev2 = devices.iter().find(|x| x != &d).unwrap().clone();
client.write(&d,   "joint1.target",  MhsValue::Float(10.0)).unwrap();
client.write(&dev2,"bath.setpoint",  MhsValue::Float(50.0)).unwrap();
assert_eq!(client.read(&d,   "joint1.target").unwrap().value, MhsValue::Float(10.0));
assert_eq!(client.read(&dev2,"bath.temperature").unwrap().value.as_f64().unwrap() > 22.0, true);
assert!(client.read(&d, "joint1.target").unwrap().t <= client.read(&dev2, "bath.temperature").unwrap().t);
```

**C12 — federation link survives remote abort.** Place in `tests/federation.rs`:

```rust
let mut pair = FederationPair::demo();
pair.a.bind_to_device("b.throttle", &"quilt-B".to_string(), "engine.throttle", &[]).unwrap();
let _ = pair.sheet(PairSide::B).abort(&"quilt-B".to_string(), "ship-side scram"); // B aborts ITSELF
assert!(matches!(pair.a.view("b.throttle").unwrap_err(), MhsError::Aborted(_)));
pair.sheet(PairSide::B).clear_abort();
assert!(pair.a.view("b.throttle").is_ok(), "link reopens after operator unlatch");
```

**C13 — monotonic device timestamps.** Add a check that walks the journal:

```rust
let samples: Vec<Sample> = (0..10).map(|_| client.read(&d, "joint1.angle").unwrap()).collect();
assert!(samples.windows(2).all(|w| w[1].t >= w[0].t), "device clock only advances");
// cross-device lockstep
let ta = client.read(&d,    "joint1.angle").unwrap().t;
let tb = client.read(&dev2, "bath.temperature").unwrap().t;
assert!(ta <= tb, "transport inter-device ordering is bounded");
```

**FORGET journal observability — new `tests/laws.rs` test:**

```rust
let mut a = adapter();
a.bind("upstream", MhsValue::Float(1.0)).unwrap();
a.bind_to_device("grip", &"mock-arm-01".to_string(), "gripper.cmd", &[]).unwrap();
a.grant("grip", &"mock-arm-01".to_string(), "gripper.cmd").unwrap();
let before = a.journal.len();
a.forget("grip").unwrap();
let tail = &a.journal[before..];
assert!(tail.contains(&OpEvent::ReleaseGrant { device: "mock-arm-01".into(), channel: "gripper.cmd".into(), held_by: "grip".into() }));
assert!(tail.contains(&OpEvent::Abort { device: "mock-arm-01".into(), reason: "last interlock forgotten".into() }));
assert!(tail.last() == Some(&OpEvent::Forget { name: "grip".into() }), "Forget is the closing event");
```

## 4. Test-count today vs. target

- **Today:** 23 (`laws.rs: 12` + `conformance.rs: 3` + `federation.rs: 5` + `schemas.rs: 3`). Cowboy's 22 undercounts `grants_are_multi_holder_and_visible`.
- **Target on conformance/laws side:** **33** — add 10 new tests:
  1. C10 `run_program` aborts on bad step (1)
  2. C11 multi-device interleaving (1)
  3. C12 federation-link-survives-remote-abort (1)
  4. C13 monotonic device timestamps (1)
  5. C9-promotion: write-after-abort-while-clock-advances (1)
  6. FORGET journal observability (1)
  7. C5-promotion: clamp-detector built into `run_conformance` (1)
  8. Lie-detector: a transport that DROPS a `read` (returns Ok with stale value) should fail C4 (1)
  9. Lie-detector: a transport that returns `Ok(())` on `hold_grant` without bookkeeping should fail law tests (1)
  10. proptest for the 5+1+1 laws (see §5) (1 file = many cases, counts as 1 test entry).
- **Target with proptest (recommended):** 33 → **35** (add 2 proptest modules).

## 5. Property-based tests

**Yes — the 5+1+1 laws are the natural fit.** `proptest` is not in `Cargo.toml`; add as dev-dep (`proptest = "1"`). In order of return:

- **Easiest — `TICK_monotonicity`:** for any `Vec<f64>`, `tick(dts)` returns monotonic `t`; any `dt <= 0` is rejected; final `t == sum_of_positive_dts`. One strategy. ~15 LOC. Highest-signal/lowest-setup law.
- **BIND_idempotence** is trivial; already covered by the example test.
- **LINK_transitivity** needs a small cell-graph generator (nodes + edges, proptest `vec` of `(node, node)` with cycle-avoidance) — 30 LOC.
- **EFFECT_associativity:** any two partitions of the same batch produce the same device-write sequence. Compare via `journal` filtered to `OpEvent::EffectDevice`.
- **FORGET_completeness** is hardest (multi-device state) — skip; example test suffices.

**Recommended minimum:** 1 proptest module on **TICK_monotonicity**. Adds 1 dev-dep + ~30 LOC.

## 6. Doc-tests in `src/*.rs`

**No ` ```rust ` fences exist today** in any of the 7 source files (grep'd: `mhs/{client,types,mock,conformance,mod}.rs`, `controller/mod.rs`, `device/{mod,federation}.rs`, `lib.rs` — zero triple-backtick doc blocks). **Zero doctests run.** Doc comments are narrative tables and prose, which is right for the seam layer.

**Candidates worth promoting** — only the ones where a runnable example teaches more than prose:

- **`MockMHS::new()` in `mhs/mock.rs`** — promote. A 5-line doctest showing `discover → manifest → write → read` is the smallest executable "hello world" of the seam. Today the only path is reading 3 test files.
- **`QuiltMhsAdapter::bind_to_device` + `effect` in `controller/mod.rs`** — promote as ONE combined doctest: the "5+1 opcode hello world" (bind a cell, effect through it, view back). The most useful one-line thing a new user can paste.
- `MhsClient` trait in `client.rs` — DO NOT promote (no defaults; doctest would re-run `tests/`).
- `run_conformance` in `conformance.rs` — DO NOT promote (it IS the contract; doctesting it is recursive).
- `QuiltDeviceProfile::demo` — borderline, one call, not enough surface.

**Net: add 2 doctests** (`MockMHS::new` hello + the 5+1 opcode chain) — pins the public API shape for newcomers.

## TL;DR for the cowboy

- Conformance contract: **9 checks**, well-scoped, A-tags clean. Run as-is: solid.
- **5 missing checks** worth adding (C10..C13 + FORGET-journal); 5-line sketches above.
- **2 doctests** to add (`MockMHS::new` hello, 5+1 opcode chain).
- **1 proptest** for `TICK_monotonicity` (easiest, highest-signal law).
- Test count: **23 → 33** (or **35** with two proptest modules). Honest current number is 23, not 22.
- "Lying transport" check exists as a test but not as a C-tag — promote it into the conformance contract proper so a third-party SDK's differential run fires it automatically.
