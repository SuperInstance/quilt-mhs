# MHS-SPEC-WATCH — every public fact and every assumption, one diff away from the real spec

Last updated: 2026-08-27 (day of announcement). This file exists so that the
day Anthropic publishes the real MHS specification/SDK, we diff our design
against it in minutes: every press-sourced fact below carries its source and
date; every guess we made carries an **A-n tag** and points at the code that
changes if the guess is wrong.

The MHS spec, SDK, schema, license, conformance suite, and governance model
are **not public** as of 2026-08-27 ([kingy.ai analysis, 2026-08-27][k]). We
designed against the *announced shape* only.

## Part 1 — Sourced facts (press, with dates)

| # | Fact | Source |
|---|---|---|
| F1 | MHS announced 2026-08-27 as a research preview to selected labs/manufacturers; open source planned, no date given. | Anthropic announcement 2026-08-27; Reuters 2026-08-27; CNBC 2026-08-27 |
| F2 | Purpose: a shared specification for AI agents to **safely operate physical devices**; microscopes, liquid handlers, robotic arms operated in parallel. | Anthropic 2026-08-27 |
| F3 | Model-agnostic; any agent harness can access it using standard protocols such as MCP. | Anthropic 2026-08-27 |
| F4 | Works with any device that has a programmable interface. | Anthropic 2026-08-27; Reuters 2026-08-27 |
| F5 | Core artifact is a standardized **driver** — software translating between the computer and the hardware device. | Anthropic 2026-08-27 |
| F6 | The driver uses simple primitives — **read** ("get temperature") and **write** ("set temperature"). | Anthropic 2026-08-27 |
| F7 | Each device is **discoverable in a standard format**; devices and agents find each other across networks without bespoke translators. | Anthropic 2026-08-27 |
| F8 | Drivers carry **natural-language tags** (user-entered, or via an agent interviewing the user — e.g., the weight of a robot arm) and produce a **reference file**: what the device can measure, what can be adjusted, **what safety limits will be enforced**. | Anthropic 2026-08-27 |
| F9 | Three control mechanisms: **MCP, a command-line interface, and code files (APIs)**; orchestration across devices "via a single line of code." | Anthropic 2026-08-27 |
| F10 | **Code files**: chained driver commands the device executes without the agent reasoning at every step (Claude learned a laser-alignment procedure, then packaged it as a deterministic script). | Anthropic 2026-08-27 |
| F11 | Integration time reduced from weeks/months to hours/minutes. | Anthropic 2026-08-27 |
| F12 | QuEra: an agent-built controller recovers the laser lock **99.3%** of the time without human intervention. (Laser-lock recovery rate at QuEra — *not* a general MHS benchmark.) | Anthropic 2026-08-27 |
| F13 | An MHS file can constrain a robot arm — "limiting the speeds and angles it is allowed to use." | TNW 2026-08-27 |
| F14 | USB-C analogy: "Users can think about it like a USB-C cord." | CNBC 2026-08-27 |
| F15 | Origin: collaboration between Alek Kemeny (Anthropic Beneficial Deployments) and Arco Bast (HHMI Janelia); grew from Bast's shared-memory dictionary letting instruments communicate at memory speed. | Anthropic 2026-08-27 |
| F16 | Partners named: Genentech, UW Baker/Pinglay, CMU, HHMI Janelia, QuEra, Tetsuwan; vendors building support: AWS (Strands Robots), Automata, Danaher, Doosan Robotics, MBF Bioscience (ScanImage), QIAGEN, Tecan, Universal Robots; next-phase early adopters: Hugging Face (LeRobot), Raspberry Pi (Camera MHS Driver). | Anthropic 2026-08-27 |
| F17 | No public spec/SDK/schema/repo/license/conformance suite/version/governance as of 2026-08-27. | kingy.ai 2026-08-27 |
| F18 | Stated limitations: Claude's spatial/physical reasoning still needs expert oversight; devices without a programming interface are out of scope for now. | Anthropic 2026-08-27 |
| F19 | EU Machinery Regulation 2023/1230 (replacing the Machinery Directive) applies from 2027-01-20 and covers AI-based safety functions; an MHS file constraining a machine may count as a regulated safety component. | TNW 2026-08-27 |

Sources:

- [Anthropic — Previewing the Model Hardware Standard (2026-08-27)][a]
- [Reuters — Anthropic unveils new framework allowing AI agents to operate physical devices (2026-08-27)][r]
- [CNBC — Anthropic pushes into physical world with new standard (2026-08-27)][c]
- [TNW — Anthropic tests a new standard for Claude to work with factory and lab hardware (2026-08-27)][t]
- [kingy.ai — Anthropic Model Hardware Standard (MHS) Explained (2026-08-27)][k]

[a]: https://www.anthropic.com/news/model-hardware-standard-research-preview
[r]: https://www.reuters.com/technology/anthropic-unveils-new-framework-allowing-ai-agents-operate-physical-devices-2026-08-27/
[c]: https://www.cnbc.com/2026/08/27/anthropic-pushes-into-physical-world-with-new-standard-to-help-ai-agents-operate-machines.html
[t]: https://thenextweb.com/news/anthropic-model-hardware-standard-mhs-eu-machinery-regulation-2027
[k]: https://kingy.ai/blog/anthropic-model-hardware-standard-mhs/

**Unverified figure, on watch:** a briefing-level claim of "99.3% vs 58%
success vs custom scripts" could not be sourced to any outlet. The 99.3% is
QuEra's laser-lock recovery (F12). The 58% appears nowhere we could find —
treat as unsourced until the real spec or a partner post names it.

## Part 2 — Assumptions we made (the A-tags)

Each assumption names the exact code that changes when the real spec lands.
None of these is presented as fact anywhere in the codebase; `MhsClient`
doc-comments repeat the A-tag at the point of use.

| Tag | Assumption | Why we made it | Code that changes |
|---|---|---|---|
| A-1 | The reference file / manifest is JSON-shaped with identity, readable/writable channels, natural-language tags, and a safety envelope. | F8 lists exactly these semantic fields; JSON is the least-commitment encoding. | `mhs/types.rs::DeviceManifest`, `schemas/*.json` regenerate |
| A-2 | Write-rate limits are per-channel, in Hz. | F13 says speeds are constrained; unit unknown. | `SafetyEnvelope.max_write_rate_hz` |
| A-3 | Discovery returns a finite enumerable device list. | F7 says discoverable; list-vs-subscribe unknown. | `MhsClient::discover` |
| A-4 | A read returns one channel's point-sample with the device clock. | F6 gives no batch semantics; point reads are the minimum. | `MhsClient::read`, `Sample` |
| A-5 | Code files are all-or-abort ordered write sequences, with a receipt reporting accepted steps and abort reason. | F10 says sequences the device runs itself; failure semantics unknown. | `MhsClient::run_program`, `ProgramReceipt` |
| A-6 | There is a per-device abort/estop surface; after abort the device refuses writes until an operator clears it. | F2 (safely operate) + F8 (limits enforced) + partner fault-recovery stories imply a stop path; exact surface unknown. | `MhsClient::abort`, `AbortReceipt`; conformance C8 |
| A-7 | Destructive channels exist and are gated by an acquirable, revocable grant (hold/release) at the transport. | Our forking rule (no destructive op without explicit grant) needs a seam; MHS "safety limits will be enforced" (F8) suggests enforcement lives in the driver. | `MhsClient::hold_grant`/`release_grant` |
| A-8 | A transport-level step/drain call advances the machine's control loop (sync seam for TICK). | Real devices loop asynchronously; quilt's TICK is synchronous. | `MhsClient::poll` |
| A-9 | Abort parks writable channels at the safe end of their range. | A safe default for mocks; real devices define their own park behavior. | `MockMHS::abort`, `QuiltDeviceProfile::abort` |
| A-10 | The interlock/abort relationship (forgetting the last grant aborts the device) is OUR policy, not MHS's. | FORGET_completeness demands teardown semantics; MHS may specify different ones. | `controller::QuiltMhsAdapter::forget` |

## Part 3 — Diff-day procedure

The day the spec drops:

1. Re-run the press table above against the real spec; mark F-n confirmed /
   corrected.
2. Walk A-1..A-10; for each, the "code that changes" column is the whole
   blast radius — everything else in this repo sits behind the
   `MhsClient`/`QuiltDeviceProfile` ports.
3. Implement `MhsClient` for the official SDK (one file), run
   `run_conformance` (C1..C8). Core checks failing = real disagreements;
   assumption-flagged checks failing = expected, update the A-tag.
4. `cargo run --bin gen-schemas`, diff `schemas/` against the official
   schema; reconcile field names.
5. Update this file's tables; bump the crate; PORTING.md §"Swapping the
   transport" is the walkthrough.
