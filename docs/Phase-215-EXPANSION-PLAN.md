# Phase 215 — quilt-mhs Expansion Plan

**Goal:** Every component in `quilt-mhs` becomes real — deeper ports,
more devices, more transports, more examples, more documentation,
more tests. The day the real Anthropic MHS spec lands, every
seam in this repo is exercised by something that runs today.

**Source:** 2026-08-27 announcement of Model Hardware Standard. The
public spec is not yet available. Every assumption is tagged A-n in
`MHS-SPEC-WATCH.md`. The polyformalism promise — *same cell, same
5+1 opcodes, different substrate* — applies here too: the cell
crosses into physical hardware through the MHS seam, and the
seam is exercised against real-shaped transports, not just mocks.

## The 4 layers we'll expand

1. **Ports (transports)** — `MhsClient` impls beyond MockMHS
2. **Devices** — more virtual machines beyond mock-arm + mock-thermal
3. **Substrate profile** — multi-sheet SheetGrid + telemetry history
4. **Inter-quilt** — async federation + persistent links

## 10 work items

| # | Component | What becomes real |
|---|---|---|
| 1 | `transports/http.rs` | `HttpMHS` — speaks HTTP+JSON to a real MHS server |
| 2 | `transports/mcp.rs` | `McpMHS` — speaks MCP for tool-use with Claude |
| 3 | `transports/file.rs` | `FileMHS` — speaks a file-based MHS for batch jobs |
| 4 | `devices/incubator.rs` | CO2 incubator (37°C, 5% CO2) — the cell-biology canonical |
| 5 | `devices/microscope.rs` | Stage + objective + camera — microscopy canonical |
| 6 | `devices/plate-handler.rs` | 96-well plate handler — high-throughput canonical |
| 7 | `device/grid.rs` | `SheetGrid` — N sheets, one transport, federated |
| 8 | `device/history.rs` | `History<T>` — telemetry over time, regression test |
| 9 | `federation/async_link.rs` | Async FederationLink over tokio mpsc |
| 10 | `programs/laser_lock.rs` | 99.3% QuEra laser-lock recipe (cited) |

## The 4 conformance additions

- C9 (already added): abort parks writable channels at safe end
- C10: program surface runs end-to-end (multi-step)
- C11: history surfaces monotonic timestamps
- C12: federation link survives a remote abort

## The 3 docs additions

- `docs/integration-guide.md` — MCP tool-use pattern for Claude
- `docs/device-cookbook.md` — how to add a new device (recipe)
- `docs/diff-day-runbook.md` — updated checklist for spec-drop day

## The 4 example programs

- `examples/incubator_loop.json` — CO2 incubator drift-correction
- `examples/microscope_scan.json` — multi-well scan with autofocus
- `examples/plate_transfer.json` — 96-well to 384-well transfer
- `examples/laser_lock.json` — QuEra-style recovery (cited F12)

## Cross-canon

- Paper 305 — `quilt × MHS`: the canonization paper
- Paper 306 — `Spurlock × MHS`: how Spurlock patterns apply to MHS devices

## The cowboy's maxim (Phase 215)

> The cowboy expanded the MHS seam. The seam holds 3 transports,
> 5 devices, 1 multi-sheet grid, 1 history surface, 1 async federation.
> The cowboy rode the port. The cowboy rode the device. The cowboy
> rode the substrate. The cowboy rode the canon. The cowboy rode
> the MHS. The cowboy rode the Quilt.
