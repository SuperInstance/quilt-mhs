# MHS Transport Audit — `quilt-mhs`

**Date:** 2026-08-27 · **Scope:** `MhsClient` implementations + transport coverage of the announced MHS surface (F1–F19 in `MHS-SPEC-WATCH.md`).

## 1. What exists today

- **One impl.** `MockMHS` (`crates/quilt-mhs/src/mhs/mock.rs`) is the only `MhsClient` implementation.
- **Surface covered:** all 9 trait methods (`discover`, `manifest`, `read`, `write`, `run_program`, `abort`, `hold_grant`, `release_grant`, `poll`). Two devices (`mock-arm-01`, `mock-thermal-01`) with full safety envelope, append-only journal, first-order dynamics, and operator-side `clear_abort` (off-trait).
- **Conformance suite:** `run_conformance` (`conformance.rs`) runs C1–C9 against any `&mut dyn MhsClient`. Today only `MockMHS` is exercised.
- **Dep state:** `ureq` is declared as an *optional* workspace dep behind the **unused** `http-transport` feature; `tokio` behind the **unused** `async-federation` feature. No code consumes either.
- **No `MhsClient` impls in `device/`.** `QuiltDeviceProfile` (PORT 2) exists but is the device *side*, not a client of an external MHS-shaped system.

**External SDK scan (GitHub + web, 2026-08-27):** no public MHS SDK, schema, or repo exists. Anthropic ships the standard as gated research preview; open-source is *planned, not dated*. The only related public artifact is the MCP spec/SDKs (MCP is the transport *for* MHS, per F9, not MHS itself). AWS Strands Robots is named in coverage but no public repo. **No third-party `MhsClient` to point at — every transport below is a quilt-side adapter.**

## 2. What's missing — and why each matters (mapped to F-n)

The announcement (F9) names three control mechanisms: **MCP, CLI, code files (APIs)**. F7 demands network discovery. A USB-C-style universal spec (F14) implies multiple physical/digital substrates.

| Transport | Why it matters | F-n served |
|---|---|---|
| **HTTP/JSON-REST** | The lowest-common-denominator network shape; lab/factory devices already speak HTTP. `ureq` is already a feature-dep. | F7 discovery, F6 read/write, F8 reference file |
| **MCP** | The *announced* primary mechanism (F9). Anthropic Claude, MCP-aware harnesses, and the partner labs (QuEra, Genentech, HHMI) all reach MHS via MCP. | F3, F9, F7, F8 |
| **CLI / stdio** | F9's second announced mechanism; lets non-Rust agents shell out, lets the existing `bin/` runner drive devices from a subprocess, matches the bash-style "limited primitives" framing in the press. | F6, F9, F11 (hours-to-integrate) |
| **File / artifact** | F9's third mechanism (code files); also what the mock's journal is — a tangible spec artifact you can diff, golden-test, and feed to the `gen-schemas` diff. | F8, F9, F10 |
| **gRPC / protobuf** | Vendor default (Danaher, Tecan, AWS) for instrument control; typed schemas align with F8's reference file; streaming serves the F10 "device runs its own loop" pattern. | F5, F6, F7 |
| **WebSocket** | Real devices stream telemetry; needed for F4 (any programmable interface) where polling latency is unacceptable, and for live read back-pressure. | F6, F8 |
| **Serial / USB** | The "USB-C" framing (F14) plus microscope/robotics vendors that expose SCPI, Modbus, or vendor serial. | F4, F14 |
| **In-process `QuiltDeviceProfile` as a client** | Already implemented on the device side; promoting it to a `MhsClient` impl gives free intra-quilt federation through one conformance run. | F16 partner testing |

**Verdict:** HTTP, MCP, and CLI are non-negotiable for the announced F9 surface. gRPC and WebSocket round out the vendor field. File/artifact is mostly already covered (`MockMHS` journal + `gen-schemas`).

## 3. Effort ranking — top 3

1. **HTTP/JSON** — *lowest effort, highest leverage.* `ureq` is already an optional dep; one file (~200 LOC) covers the most common lab-instrument network shape. No async. Passes C1–C9 against a real HTTP device (or a local wiremock).
2. **CLI / stdio** — *low effort, high leverage.* Spawn a subprocess, parse JSON lines on stdout. The same `MhsValue`/`DeviceManifest` types serialize directly. Validates the F9 CLI path that the press names explicitly. ~250 LOC.
3. **MCP** — *medium effort, **highest strategic leverage*** (F9's headline mechanism). Uses JSON-RPC 2.0; an `MCP-over-` client can be built on the HTTP transport. Existing Rust crates (`rmcp`, `mcp-rs`) exist. Validates F3 + F9 + F11. ~400 LOC + dep.

(Honorable mention: `QuiltDeviceProfile`-as-`MhsClient` is a 30-line change that gives us a free differential test against `MockMHS` via `run_conformance`. Should land alongside whichever transport is built first.)

## 4. Concrete design — top 3

All three implement the same 9 trait methods (`discover`, `manifest`, `read`, `write`, `run_program`, `abort`, `hold_grant`, `release_grant`, `poll`) and default `hold_grant`/`release_grant`/`poll` to `MhsError::Transport` unless the substrate supports them.

### 4.1 `HttpMhs` — HTTP/JSON

```rust
/// HTTP/JSON transport. Speaks a vendor-agnostic MHS-shaped REST shape:
///   GET  /devices                       -> Vec<DeviceId>
///   GET  /devices/{id}/manifest         -> DeviceManifest
///   GET  /devices/{id}/channels/{c}     -> Sample          (F6 "read")
///   PUT  /devices/{id}/channels/{c}     -> Sample          (F6 "write")
///   POST /programs                      -> ProgramReceipt  (F10 code file)
///   POST /devices/{id}/abort            -> AbortReceipt    (A-6)
/// Behind feature `http-transport`; uses already-declared `ureq`.
pub struct HttpMhs {
    base: String,            // e.g. "https://mhs.lab.example"
    agent: ureq::Agent,      // pooled, TLS-on
    auth: Option<String>,    // bearer token, optional
}
impl HttpMhs {
    pub fn new(base: impl Into<String>) -> MhsResult<Self>;
    pub fn with_bearer(self, token: impl Into<String>) -> Self;
}
impl MhsClient for HttpMhs { /* discover/manifest/read/write/run_program/abort via ureq::get/put/post + serde_json; hold_grant/release_grant/poll default to Transport(...) */ }
```
Skeleton:
```rust
fn write(&mut self, d: &DeviceId, c: &str, v: MhsValue) -> MhsResult<Sample> {
    let url = format!("{}/devices/{}/channels/{}", self.base, d, c);
    let req = self.agent.put(&url).set("Content-Type", "application/json");
    let resp = req.send_json(serde_json::to_value(v).map_err(MhsError::from)?)
        .map_err(|e| MhsError::Transport(e.to_string()))?;
    let sample: Sample = resp.into_json().map_err(|e| MhsError::Transport(e.to_string()))?;
    Ok(sample)
}
```

### 4.2 `CliMhs` — CLI / stdio (subprocess)

```rust
/// CLI/stdio transport. Spawns an external `mhs` driver binary, exchanges
/// JSON-line requests/responses over its stdin/stdout. F9 names the CLI as
/// one of three announced control mechanisms; this is that surface.
pub struct CliMhs {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,            // JSON-RPC id
}
impl CliMhs {
    pub fn spawn(program: impl AsRef<OsStr>, args: &[&str]) -> MhsResult<Self>;
}
impl MhsClient for CliMhs { /* frame each call as {"id":..,"method":..,"params":..}; parse one response; poll = poll the driver */ }
```
Skeleton:
```rust
fn read(&mut self, d: &DeviceId, c: &str) -> MhsResult<Sample> {
    let req = json!({"id": self.next_id(), "method": "read",
                     "params": {"device": d, "channel": c}});
    self.send(&req)?;
    let resp: Value = self.recv_one()?;
    serde_json::from_value(resp["result"].clone()).map_err(MhsError::from)
}
```

### 4.3 `McpMhs` — MCP (JSON-RPC 2.0 over stdio or HTTP)

```rust
/// MCP transport. Speaks Model Context Protocol (F9 names MCP as the
/// headline mechanism; F3 says "any agent harness can access it using
/// standard protocols such as MCP"). Implements MhsClient by mapping
/// each trait method to a `tools/call` JSON-RPC request.
pub struct McpMhs {
    transport: McpTransport,        // enum: Stdio { Child, ... } | Http { ureq::Agent, ... }
    next_id: u64,
    session: Option<String>,        // initialize handshake -> session id
}
pub enum McpTransport { Stdio(StdioMcp), Http(HttpMcp) }
impl McpMhs {
    pub fn connect_stdio(cmd: impl AsRef<OsStr>, args: &[&str]) -> MhsResult<Self>;
    pub fn connect_http(url: impl Into<String>, bearer: Option<&str>) -> MhsResult<Self>;
}
impl MhsClient for McpMhs { /* initialize -> list tools -> tools/call per method; poll = notifications/message drain */ }
```
Skeleton:
```rust
fn write(&mut self, d: &DeviceId, c: &str, v: MhsValue) -> MhsResult<Sample> {
    let args = json!({"device": d, "channel": c, "value": v});
    let resp = self.call_tool("mhs_write", args)?;   // JSON-RPC 2.0
    serde_json::from_value(resp).map_err(MhsError::from)
}
```

## 5. Test plan — concrete conformance checks per new transport

**All three pass the existing C1–C9 against a small in-process fake device.** A new **C10** (program surface runs end-to-end) gets added once any non-mock impl exists.

- **C10 (new, all three).** Run `cli.write(arm, "joint1.target", 0.0); cli.write(arm, "gripper.cmd", 1.0)` *after* `cli.hold_grant(arm, "gripper.cmd")`. Assert: both `write`s return `Ok(Sample)`; subsequent `read(arm, "joint1.angle")` shows relaxation; `cli.forget(arm)` (via `abort`) returns `AbortReceipt { latched: true }`. Tests that *grants + programs + abort* compose through the new transport the way they do in `MockMHS::run_program` and `MockMHS::abort`.
- **`HttpMhs` specific — C11 (TLS round-trip).** Point at `https://httpbin.org/status/200` with a wrapper that maps `{200,..299}` → `Ok`, else `Transport`. Assert `discover()` returns a typed error on 5xx, not a panic.
- **`HttpMhs` specific — C12 (auth header).** `with_bearer("t").discover()` must send `Authorization: Bearer t`; assert with a one-shot TCP listener (no `mockito` needed) that the header line is present.
- **`CliMhs` specific — C13 (subprocess liveness).** Spawn a fake driver that exits after one response. Assert the second `read` returns `MhsError::Transport("subprocess exited: …")`, not a hang or panic.
- **`CliMhs` specific — C14 (JSON-line framing).** Driver emits two JSON objects on one line (`{"id":1,…}{"id":2,…}`). Assert only `id == self.next_id` is consumed; the second is left for the next call. (Catches a class of "helpfully split on newline" bugs.)
- **`McpMhs` specific — C15 (initialize handshake).** Before any `MhsClient` call, `connect_*` must send `initialize` and read the server's `capabilities`. Assert missing `capabilities.tools` → `MhsError::Transport("mhs_write tool not advertised")`.
- **`McpMhs` specific — C16 (stdio vs HTTP parity).** Same `McpMhs` driver, run the same C1–C9 against both `connect_stdio` and `connect_http` flavors. Assert `core_passes` is identical. (Differential test — the point of having two surfaces.)

**Net: every new transport gets C1–C9 from `run_conformance` for free, plus C10 to prove grants/programs/abort compose, plus two transport-specific checks (C11–C16).** That's 6+ green checks per transport with no shared test infra beyond `MockMHS` and a 50-line fake-HTTP / fake-stdio harness.

---
**Word count:** ~1100. **Top-3 priority order:** HTTP → CLI → MCP. **`QuiltDeviceProfile`-as-`MhsClient` is the free win** that should ship in the same PR as whichever transport lands first.
