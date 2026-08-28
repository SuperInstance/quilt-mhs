# Integration Guide — wiring an MHS-shaped agent

**Audience:** a Claude/agent author wiring MHS into a tool-use loop. By
the end of this guide you will have a working `McpMhs` (or `HttpMhs` /
`CliMhs`) instance that your agent can call with the same `MhsClient`
trait the controller side uses.

## 1. The shape of an MHS call from an agent

A quilt cell drives an MHS device through the same five surfaces a
human operator uses. Mapped to the announcement (F3, F9):

| Agent action | MHS surface | Trait method | MCP tool name |
|---|---|---|---|
| Discover devices | device list (F7) | `discover` | `mhs.discover` |
| Read the reference file (F8) | manifest | `manifest` | `mhs.manifest` |
| Read a channel (F6) | `get temperature` | `read` | `mhs.read` |
| Write a channel (F6) | `set temperature` | `write` | `mhs.write` |
| Chain writes (F10) | code file | `run_program` | `mhs.run_program` |
| Abort (A-6) | estop | `abort` | `mhs.abort` |
| Hold interlock (A-7) | grant | `hold_grant` | `mhs.hold_grant` |
| Release interlock (A-7) | release | `release_grant` | `mhs.release_grant` |
| Step the loop (A-8) | poll | `poll` | `mhs.poll` |

Nine trait methods, nine MCP tools. The mapping is 1:1 by design — the
MCP tool is the verb; the MHS surface is the noun.

## 2. A trace: `discover → manifest → bind → effect → view → tick → forget`

```text
1. mhs.discover                        -> [mock-thermal-01, mock-arm-01, ...]
2. mhs.manifest(mock-thermal-01)       -> DeviceManifest { readable: [bath.temperature],
                                          writable: [bath.setpoint, pump.duty],
                                          safety: SafetyEnvelope { channel_limits,
                                          max_write_rate_hz, ... } }
3. (in agent memory) bind "e.setpoint" -> channel "bath.setpoint" on device
4. mhs.write(mock-thermal-01, "bath.setpoint", 65.0) -> Sample { value: 65.0, t: 0.0 }
5. mhs.read (mock-thermal-01, "bath.temperature")     -> Sample { value: 22.0, t: 0.0 }
6. mhs.poll(1.0)                       -> 1.0  (advances machine time)
7. mhs.read (mock-thermal-01, "bath.temperature")     -> Sample { value: 22.97, t: 1.0 }
8. (when done) mhs.abort(mock-thermal-01, "experiment complete")
                                          -> AbortReceipt { latched: true }
```

Each step is one round-trip; the agent's reasoning lives between them.
For chained, deterministic procedures, package the writes as a code file
(`examples/code_file.json`) and call `mhs.run_program` once — the device
executes without per-step reasoning (F10).

## 3. The three error classes — and the recovery

The agent *must* handle these three MHS errors. The recovery is
mechanical, not "be polite":

| Error | When | Recovery |
|---|---|---|
| `MhsError::SafetyViolation` | the value is outside the device's declared envelope | **do NOT retry** with a clamped value; surface the rejection to the user, or revise the request to fit the envelope |
| `MhsError::GrantRequired` | the channel is `destructive: true` and the agent has not held a grant | `mhs.hold_grant(device, channel)` first, then re-issue the write; release the grant when the destructive sequence is complete |
| `MhsError::Aborted` | the device latched after an estop or a program step rejected | **refuse to write**; ask a human operator to call `clear_abort` (off-trait in `MockMHS`; the real spec is A-6 unverified) |

A "helpful" transport that clamps out-of-range writes instead of
rejecting them is a **lying transport**. Conformance check C5 catches
it. Do not write one.

## 4. A complete working example

`examples/mcp_tool_use.json` shows the JSON trace. The Rust wiring:

```rust
use quilt_mhs::mhs::client::MhsClient;
use quilt_mhs::mhs::mock::MockMHS;

fn main() {
    let mut mhs = MockMHS::new();
    let devices = mhs.discover().unwrap();
    println!("discovered {} devices", devices.len());
    let manifest = mhs.manifest(&"mock-thermal-01".into()).unwrap();
    println!("{} channels writable", manifest.writable.len());
    let _ = mhs.write(&"mock-thermal-01".into(), "bath.setpoint", 65.0.into()).unwrap();
    let sample = mhs.read(&"mock-thermal-01".into(), "bath.temperature").unwrap();
    println!("T = {:?}", sample.value);
    let _ = mhs.poll(1.0).unwrap();
}
```

To swap `MockMHS` for the real spec, replace the line:
- `HttpMhs::new("https://mhs.lab.example").with_bearer(env::var("MHS_TOKEN")?)`
- `McpMhs::connect_stdio("mhs-driver", &["--port", "8080"])?`
- `CliMhs::spawn("mhs", &["--stdio"])`

The trait is the same. The conformance suite runs unchanged. The agent
sees the same verbs.

## 5. When the spec actually lands

See `docs/diff-day-runbook.md` for the on-call checklist.

**The polyformalism promise:** the same MHS client trait, the same
five+1+1 quilt opcodes, the same `QuiltMhsAdapter` controller — they
all work unchanged when Anthropic ships the real spec. The blast
radius is one file: the new `MhsClient` impl.
