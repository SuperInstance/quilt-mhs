# Audit — `quilt-mhs` examples & docs gaps

**Repo:** `/workspace/quilt-mhs` · **For:** the cowboy · **Date:** 2026-08-27
**Sources read:** `examples/*.example.json`, `README.md`, `PORTING.md`,
`MHS-SPEC-WATCH.md`, `docs/Phase-215-EXPANSION-PLAN.md`, `crates/quilt-mhs/src/lib.rs`,
`bin/gen_schemas.rs`, `mhs/{types,client}.rs`, all `schemas/*.json`. The
Phase-215 plan is the cowboy's own shopping list — this audit
cross-checks what is actually shipped against it.

---

## 1. What examples exist today

`examples/` contains exactly **four** JSON files. They are *schema
fixtures*, not runnable scenarios — they document the vocabulary, not a workflow.

| File | Demonstrates |
|---|---|
| `command.example.json` | One `Command` write: `bath.setpoint = 65.0` on `mock-thermal-01`. Shows `MhsValue::Float` encoding. |
| `device-manifest.example.json` | A `DeviceManifest` for a **quilt substrate** (`quilt-lab-01`, transport `quilt-sheet-inprocess`) — not a physical device. Shows channels, `destructive: true`, full `SafetyEnvelope`. |
| `safety-envelope.example.json` | Standalone `SafetyEnvelope` for a robot arm (`joint1.target ±90°`, `gripper.cmd 0..1`, 10 Hz rate limit). |
| `telemetry-sample.example.json` | One `Sample` read: `bath.temperature = 48.76` at `t=10.0`. |

**Missing vocabulary that the schemas already define but no example
shows:** `ProgramReceipt`, `AbortReceipt`, a destructive write
(`engine.scram` is in the manifest but never written), and an example
that *triggers* `MhsError::SafetyViolation`. Conformance `C5` proves
rejection works; no file models the failure path.

---

## 2. What's missing — the cowboy's checklist

Phase-215 names **4 example programs**; the README's "30 seconds"
implies an end-to-end narrative; the announcement cites canonical
lab/factory scenarios (F2, F12, F16) not yet demonstrated.

| Requested | Status |
|---|---|
| Runnable E2E (`laser_lock` — F12, QuEra 99.3%) | **Missing** |
| MCP integration example (F3) | **Missing** (only the transport string `"mcp"` is mentioned) |
| CLI integration example (F9) | **Missing** |
| Code-files example (F10, chained writes) | **Missing** (type exists) |
| 4 canonical scenarios (incubator, microscope, plate-handler, …) | **All 4 missing** |

**9 example files absent** out of 4 shipped. Types are exercised by tests; scenarios live only in markdown.

---

## 3. Skeletons for each missing example

Each skeleton is valid against the committed JSON schemas and `mhs::types`.

### 3.1 `examples/laser_lock.json` — E2E QuEra-style recovery (F12)
TICK-driven control loop; 99.3%-style recovery an agent can re-run from a code file.
```json
{
  "$type": "Program", "device": "mock-laser-01",
  "description": "QuEra-style laser-lock recovery (F12, 99.3%).",
  "steps": [
    { "device": "mock-laser-01", "channel": "lock.enable",     "value": { "Bool":  true } },
    { "device": "mock-laser-01", "channel": "piezo.offset",    "value": { "Float": 0.0  } },
    { "device": "mock-laser-01", "channel": "piezo.dither_hz", "value": { "Float": 12.0 } },
    { "device": "mock-laser-01", "channel": "lock.threshold",  "value": { "Float": 0.05 } }
  ]
}
```

### 3.2 `examples/mcp_tool_use.json` — MCP transport contract (F3)
How a Claude-side MCP tool-call maps onto `MhsClient::write` / `::read`.
```json
{
  "$type": "McpToolTrace", "transport": "mcp", "tool": "mhs.write",
  "arguments": { "device": "mock-thermal-01", "channel": "bath.setpoint", "value": { "Float": 65.0 } },
  "expected_return": { "device": "mock-thermal-01", "channel": "bath.setpoint", "value": { "Float": 65.0 }, "t": 10.0 },
  "agent_loop": ["VIEW bath.temperature", "compute dT", "EFFECT bath.setpoint", "TICK 1.0"]
}
```

### 3.3 `examples/cli_session.json` — CLI control flow (F9)
Operator-style script of CLI invocations — discover → manifest → read →
write → run_program → abort — replayable by any of the three announced mechanisms.
```json
{
  "$type": "CliSession",
  "commands": [
    "mhs discover",
    "mhs manifest --device mock-thermal-01",
    "mhs read --device mock-thermal-01 --channel bath.temperature",
    "mhs write --device mock-thermal-01 --channel bath.setpoint --value 65.0",
    "mhs run-program --device mock-thermal-01 --file incubator_loop.json",
    "mhs abort --device mock-thermal-01 --reason 'overtemp detected'"
  ]
}
```

### 3.4 `examples/code_file.json` — chained program (F10)
All-or-abort write sequence; the device executes without per-step
agent reasoning; receipt reports partial application.
```json
{
  "$type": "Program", "device": "mock-arm-01", "all_or_abort": true,
  "steps": [
    { "device": "mock-arm-01", "channel": "gripper.cmd",   "value": { "Float": 0.0  } },
    { "device": "mock-arm-01", "channel": "joint1.target", "value": { "Float": 30.0 } },
    { "device": "mock-arm-01", "channel": "joint2.target", "value": { "Float": -15.0 } },
    { "device": "mock-arm-01", "channel": "gripper.cmd",   "value": { "Float": 1.0  } }
  ]
}
```

### 3.5 `examples/incubator_loop.json` — CO₂ drift correction (canonical #1)
Periodic read-then-write correction.
```json
{ "$type": "Program", "device": "mock-incubator-01",
  "steps": [
    { "device": "mock-incubator-01", "channel": "temp.setpoint", "value": { "Float": 37.0 } },
    { "device": "mock-incubator-01", "channel": "co2.setpoint",  "value": { "Float": 5.0  } }
  ] }
```

### 3.6 `examples/microscope_scan.json` — stage + autofocus (canonical #2)
Multi-channel coordination (XY stage, Z focus, camera) in one chained program.
```json
{ "$type": "Program", "device": "mock-microscope-01",
  "steps": [
    { "device": "mock-microscope-01", "channel": "stage.x",     "value": { "Float": 1000.0 } },
    { "device": "mock-microscope-01", "channel": "stage.y",     "value": { "Float": 500.0  } },
    { "device": "mock-microscope-01", "channel": "focus.z",     "value": { "Float": 12.4   } },
    { "device": "mock-microscope-01", "channel": "camera.snap", "value": { "Bool":  true   } }
  ] }
```

### 3.7 `examples/plate_transfer.json` — 96→384 well transfer (canonical #3)
F10 "learned procedure" — loop count pre-baked from the agent's learned recipe.
```json
{ "$type": "Program", "device": "mock-platehandler-01",
  "steps": [
    { "device": "mock-platehandler-01", "channel": "source.well",  "value": { "Int":   1    } },
    { "device": "mock-platehandler-01", "channel": "dest.well",    "value": { "Int":   1    } },
    { "device": "mock-platehandler-01", "channel": "volume.ul",    "value": { "Float": 2.5  } },
    { "device": "mock-platehandler-01", "channel": "aspirate",     "value": { "Bool":  true } }
  ] }
```

### 3.8 `examples/abort_recovery.json` — destructive + grant + FORGET (canonical #4)
5+1+1 end-to-end — destructive channel written under an explicit grant, released by FORGET.
```json
{
  "$type": "Program", "device": "mock-arm-01",
  "note": "engine.scram is destructive; requires hold_grant then release_grant (A-7).",
  "steps": [ { "device": "mock-arm-01", "channel": "engine.scram", "value": { "Bool": true } } ],
  "interlocks": { "hold_grant": ["engine.scram"], "release_grant_after": true }
}
```

The 9th, a `program_receipt.example.json` showing partial-apply failure
(3/5 accepted, abort on step 4), is vocabulary-only but worth shipping
because the conformance story hinges on it.

---

## 4. Docs gap analysis

| Doc | Present? | Notes |
|---|---|---|
| `README.md` | ✅ | Hero image but **no architecture diagram**; two-adapter story told only in prose. |
| `PORTING.md` | ✅ | ASCII figure + 5+1 opcode table + diff-day procedure. Strong. |
| `MHS-SPEC-WATCH.md` | ✅ | F1–F19, A-1..A-10, diff-day §3. |
| `docs/Phase-215-EXPANSION-PLAN.md` | ✅ | Cowboy's shopping list. |
| `docs/integration-guide.md` | ❌ | Named in Phase-215. |
| `docs/device-cookbook.md` | ❌ | Same. |
| `docs/diff-day-runbook.md` | ❌ | Same. SPEC-WATCH §3 is a *procedure*; the runbook should be a *checklist* an on-call engineer runs. |
| Architecture diagram (image) | ❌ | README has a hero photo, no diagram. A small SVG — `QuiltMhsAdapter ↔ MhsClient ↔ {MockMHS \| McpMHS \| CliMHS \| FileMHS} ↔ {physical \| QuiltDeviceProfile}` — would land in 30 seconds. The 4 transport boxes map 1:1 onto Phase-215 items 1–3. |

---

## 5. Outlines for the three missing docs

### `docs/integration-guide.md` (~400 lines)
**Audience:** a Claude/agent author wiring MHS into a tool-use loop.
**(1)** What MCP tool-use looks like over MHS — a tool schema derived
from `DeviceManifest` (one tool per device, params = writable channels).
**(2)** A trace: `discover → manifest → bind → effect → view → tick →
forget`, each step mapped to one MCP round-trip. **(3)** The three
error classes the agent *must* handle — `SafetyViolation`,
`GrantRequired`, `Aborted` — and recovery: don't retry a rejected
write, request a grant before a destructive effect, refuse to write a
latched device. **(4)** A complete example using
`examples/mcp_tool_use.json`. **(5)** Cross-link to
`docs/diff-day-runbook.md`.

### `docs/device-cookbook.md` (~300 lines)
**Audience:** a vendor porting a new instrument to MHS.
**(1)** Write a `DeviceManifest` using `safety-envelope.example.json` +
`device-manifest.example.json` as the template. **(2)** Implement the
five-method `MhsClient` surface (`discover`, `manifest`, `read`,
`write`, `abort`; optionally `run_program`, `hold_grant`, `poll`).
**(3)** Run `mhs::conformance::run_conformance`; chase C5 first — it
catches "helpful" clamping. **(4)** Run `tests/laws.rs` unchanged —
laws must not know your device exists. **(5)** Worked example: pick
`incubator_loop.json` / `microscope_scan.json` / `plate_transfer.json`
and show the manifest that makes it valid.

### `docs/diff-day-runbook.md` (~200 lines)
**Audience:** the on-call engineer the day the real spec ships.
**(1) T+0:** freeze the repo, tag the commit. **(2) T+15m:** walk
F1–F19 in SPEC-WATCH; mark each confirmed/corrected/new. **(3) T+1h:**
walk A-1..A-10; the "code that changes" column is the whole blast
radius. **(4) T+3h:** add `mhs-official` feature flag, write one
`OfficialSdkClient` impl (~200 lines), `cargo test --test conformance`.
**(5) T+4h:** `cargo run --bin gen-schemas`, diff `schemas/`,
reconcile field names. **(6) T+5h:** run `laws.rs` and `federation.rs`;
if green, the port is done. **(7) T+6h:** update SPEC-WATCH, bump the
crate, CHANGELOG per changed F-n/A-n. **(8) Rollback:** revert the
feature flag; the repo runs unchanged on `MockMHS`.

---

## Bottom line for the cowboy

The repo is **type-complete, scenario-empty.** Phase-215 correctly
names every gap. Highest-leverage moves, in order:

1. **Ship the 8 scenario examples** (§3.1–§3.8) — they make the
   README's 30-second narrative *reproducible* instead of aspirational.
2. **Write `docs/integration-guide.md`** — turns this from "Rust
   crate with a conformance suite" into "thing a Claude agent author
   can wire up on a Tuesday."
3. **Add a rendered architecture diagram to README.**
4. **Defer `diff-day-runbook.md` and `device-cookbook.md`** until the
   scenario examples force the prose into sharper focus — write docs
   *after* the examples they reference exist.

Phase-215 calls for "4 example programs + 3 docs." The real count is
**8 examples + 1 diagram + 3 docs** if the cowboy wants Phase 215
complete rather than minimum-viable.
