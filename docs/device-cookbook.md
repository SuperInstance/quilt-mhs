# Device Cookbook — adding a new MHS-shaped device

**Audience:** a vendor porting a new instrument to MHS, or a cowboy
adding a new mock device to `MockMHS`. By the end of this guide, you
will have a working `DeviceManifest` and the right `MhsClient` impl to
match.

## 1. Pick the device

Start from the F16 partner list in `MHS-SPEC-WATCH.md`:

> Genentech, UW Baker/Pinglay, CMU, HHMI Janelia, QuEra, Tetsuwan,
> AWS Strands Robots, Automata, Danaher, Doosan Robotics, MBF
> Bioscience (ScanImage), QIAGEN, Tecan, Universal Robots, Hugging
> Face LeRobot, Raspberry Pi Camera MHS Driver.

The list is named in the announcement. Pick one you know. Two Phase-215
mocks were built this way (`mock_incubator` from Heracell, `mock_laser`
from QuEra).

## 2. Write the `DeviceManifest`

Use `safety-envelope.example.json` + `device-manifest.example.json` as
the JSON template. The Rust struct is in `mhs::types::DeviceManifest`.
Six fields, all required:

```rust
DeviceManifest {
    device_id: "mock-incubator-01".into(),
    model:      "MockIncubator I1 (Heracell-class)".into(),
    firmware:   "0.1.0-mock".into(),
    tags:       vec![ /* 3-5 natural-language lines from the vendor manual */ ],
    readable:   vec![ /* Channel { name, unit, range, writable: false, destructive: false } */ ],
    writable:   vec![ /* Channel { name, unit, range, writable: true, destructive: bool } */ ],
    safety:     SafetyEnvelope { channel_limits, max_write_rate_hz,
                                destructive_requires_grant, abort_supported },
    transport:  "mock".into(),   // or "http", "mcp", "cli", "code-files", "quilt-sheet-inprocess"
}
```

**Tags are not optional.** The press explicitly names natural-language
tags as a core MHS artifact (F8). Use the vendor manual's own warnings:
*"Do not command >40 °C: HEPA gasket softens above 45 °C — irreversible."*

**Destructive channels are not optional either.** Anything that
cannot be undone by `abort` (door lock, gripper close, pump prime)
must be `destructive: true`. The default is `destructive_requires_grant:
true`; keep that, or document the override in `tags`.

## 3. Implement the 9-method `MhsClient` surface

Five are mandatory; four have default `Err(Transport(...))` impls:

| Required | Method | Purpose |
|---|---|---|
| ✓ | `discover` | return `[device_id, ...]` |
| ✓ | `manifest` | return the `DeviceManifest` |
| ✓ | `read` | return `Sample { value, t }` — **pure**, no state change |
| ✓ | `write` | return `Sample { value, t }` — enforce safety envelope |
| ✓ | `run_program` | all-or-abort chained writes |
| optional | `abort` | estop; park writable channels at safe end of range (C9) |
| optional | `hold_grant` / `release_grant` | interlock (A-7) |
| optional | `poll` | advance the device control loop (A-8) |

If your device is read-only (a sensor), `write` returns
`Err(MhsError::NotWritable)`. If it has no abort, `abort_supported:
false` in the manifest. The conformance suite will tell you which
checks your device passes; see step 5.

## 4. Run the conformance suite

```rust
use quilt_mhs::mhs::conformance::run_conformance;
let results = run_conformance(&mut my_device);
for r in &results {
    println!("{}: {} — {}", r.id, if r.passed {"OK"} else {"FAIL"}, r.detail);
}
```

C1..C9 are the contract. **C5 catches the "helpful clamping" bug**
where a transport silently clamps out-of-range writes; the suite
asserts the rejection. C9 asserts `abort` parks writable channels at
the safe end of their range.

The day Anthropic ships the real spec, you write one `MhsClient` impl
and run this same suite. If it passes, the whole controller stack
keeps working unchanged.

## 5. Run the laws test unchanged

```rust
use quilt_mhs::mhs::mock::MockMHS;
let mut a = QuiltMhsAdapter::new(my_device);
// ... bind, link, effect, view, tick, forget ...
```

The 5+1+1 laws must not know your device exists. The laws are
properties of the **adapter** (BIND_idempotence, LINK_transitivity,
EFFECT_associativity, VIEW_purity, TICK_monotonicity,
super-relevance, FORGET_completeness). They hold on every
`MhsClient` impl because the adapter is the one that journals, not
the device.

## 6. Add a worked example

Pick one of `incubator_loop.json` / `microscope_scan.json` /
`plate_transfer.json` / `laser_lock.json` and edit it for your
device. The example must:

- be valid against the committed JSON schemas (`schemas/mhs-command.schema.json`),
- show at least one *rejection* (out-of-range write) so the safety
  story is visible,
- show at least one *destructive* write under a grant so the
  interlock story is visible,
- be replayable end-to-end against `MockMHS` or your impl.

## 7. The 4 common mistakes

1. **Forgetting `tags`.** The press named them; a manifest without
   them looks like a generic stub.
2. **Setting `destructive: false` on a `gripper.cmd` channel.** "1.0
   is a hard close — samples may be crushed. Treat as destructive."
3. **Returning the new value from `read`.** `read` is pure; it must
   not advance the device clock. The press implies this ("get
   temperature" is observation, not actuation).
4. **Implementing `write` as `&mut self + return self.write(...)`.**
   The conformance suite checks for atomicity: a rejected write
   leaves state unchanged. Split the *enforce* and *apply* steps.

## 8. The 4 patterns worth copying from the existing mocks

| Pattern | Where | Why |
|---|---|---|
| First-order relaxation | `MockMHS::tick` | simple, faithful to real dynamics, gives the agent a smooth TICK |
| Append-only journal | `MockMHS::journal` | TICK_monotonicity and VIEW_purity are observable from outside |
| Per-channel rate limit | `MockDevice::enforce` | F13 implied speeds/angles are constrained; unit Hz is A-2 |
| Destructive-grant + abort-park | `MockMHS::hold_grant` + `MockMHS::abort` | A-6 + A-7 + A-9, the core safety story |
