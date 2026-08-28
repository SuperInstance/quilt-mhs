# Audit: quilt-mhs substrate profile

*Scout report. Scope: how a quilt runtime exposes itself as an MHS device,
and the gap to a production substrate.*

## 1. What exists today

A single-sheet reference runtime behind a self-implemented `MhsClient` port.

- **`SheetRuntime`** (`device/mod.rs`): a `BTreeMap<String, SheetCell>`
  with a monotonic `clock: f64`. Surface is `define / get / set / tick` —
  exactly what `QuiltDeviceProfile` needs. A real deployment swaps it for
  `quilt_rust::QuiltEngine` (same shape).
- **`SheetCellKind`**: `Value | Formula | Sensor | Io`. Only `Io` is
  writable; a subset is `destructive: true` (interlocked — see
  `engine.scram` in `demo`).
- **`QuiltDeviceProfile`**: device_id + model + one `SheetRuntime` +
  `grants: BTreeSet<String>` + `aborted: Option<String>` + tags. Its
  manifest's `channel_limits` come from cells' `range`;
  `destructive_requires_grant: true` is the forking rule.
- **`FederationLink`** (`device/federation.rs`):
  `Arc<Mutex<QuiltDeviceProfile>>` wrapped in a `MhsClient` impl.
  `run_program` holds the mutex across the step list so a partial
  program either completes or aborts. `FederationPair::demo()` wires
  two demo sheets and is what `tests/federation.rs` exercises (5
  tests).

`QuiltDeviceProfile` implements the full `MhsClient` trait:
`discover`, `manifest`, `read`, `write`, `run_program`, `abort`,
`hold_grant`, `release_grant`, `poll`. Reads return a `Sample` stamped
with `sheet.clock`; writes check `kind == Io`, enforce the
destructive-grant rule, run the value through `SheetRuntime::set` (which
enforces `range`). `abort` parks every ranged IO cell at its contract
floor — what conformance **C9** validates. `poll` delegates to
`sheet.tick`; `dt <= 0` is `MhsError::NotMonotonic`.

## 2. What's missing

1. **Multi-sheet `SheetGrid` (N sheets per device).** Does not exist. A
   `QuiltDeviceProfile` carries one `SheetRuntime`; an MHS manifest is
   per-device, so N sheets on one device = one manifest with
   `sheet.name` folded into channel names (e.g. `nav.bilge.depth`).
2. **Telemetry history.** Does not exist. `Sample.t` is the sheet clock
   *at call time*; nothing retains past samples. No regression, no
   windowed query, no time-series. C11 — the conformance check this
   enables — has nothing to assert against.
3. **Persistent federation links.** Does not exist. `FederationLink`
   owns an `Arc<Mutex<…>>` in the same process; on restart the link is
   gone. A real link needs a serialized endpoint (device_id + transport
   + auth), reconnect/replay, a journal of in-flight writes.
4. **Async federation over `tokio::sync::mpsc`.** Does not exist. The
   trait is intentionally sync (`mhs/client.rs`: "Intentionally
   synchronous — quilt's engine is sync at the core and async happens
   at the boundary"). A real network transport sits behind a new
   `MhsClient` impl; the sync `FederationLink` is the wrap surface.

## 3. Effort ranking (highest leverage first)

1. **Multi-sheet `SheetGrid`** — unlocks the next demo (nav + engine +
   cargo on one runtime) and reuses every existing method. Most of
   the diff is in `build_manifest` and the read/write routers. Lowest
   risk, highest visible payoff.
2. **Telemetry history** — every real MHS deployment wants "what was
   `pump.duty` 10 minutes ago?" before a second sheet. A bounded ring
   per sensor cell + monotonic stamping is self-contained and feeds
   C11.
3. **Async federation** — required to leave one process, but the trait
   is the seam; the wrap is one new file plus a `PORTING.md` paragraph.
   Do it after 1 and 2 so the conformance suite can exercise the async
   path against a grid that already has history.

Persistent links is real work but lower leverage until a real remote
runtime exists.

## 4. Top-3 in detail

### 4.1 Multi-sheet `SheetGrid`

**Design rationale.** A real deployment is a *fleet* of sheets on one
runtime (nav, engine, cargo, comms), not one sheet per runtime. MHS is
per-device, so a `SheetGrid` is the right primitive: one device, one
manifest, N sheets, channels namespaced by `sheet.channel`. A cell in
sheet A drives a cell in sheet B *the same way an external agent does*:
through `MhsClient` — through the safety envelope, with manifest
discovery and interlock grants. This is intra-quilt over the seam.

```rust
pub struct SheetGrid {
    pub device_id: DeviceId,
    pub model: String,
    pub tags: Vec<String>,
    sheets: BTreeMap<String, SheetRuntime>,           // sheet name -> runtime
    grants: BTreeMap<(String, String), BTreeSet<String>>, // (sheet,channel) -> holders
    aborted: Option<String>,
}

impl SheetGrid {
    pub fn add_sheet(&mut self, name: &str, sheet: SheetRuntime);
    pub fn route(&self, channel: &str) -> Option<(&str, &str)>; // (sheet, channel)
    pub fn build_manifest(&self) -> DeviceManifest;            // flattens sheets
}
```

**How a cell in A drives a cell in B.** The grid is exposed through the
same `MhsClient` impl the single-sheet profile uses. Controller side:
`adapter.bind_to_device("e.pump", &device_id, "engine.pump.duty", …)`.
`adapter.effect("e.pump", v)` issues
`grid.write("engine", "pump.duty", v)`, which routes through the
manifest's safety envelope (range + destructive + grant) like a remote
write. From the grid's point of view there is no intra-vs-inter
distinction — the *seam* is the safety story.

**Difference from `FederationLink`.** `FederationLink` forwards through
a `Mutex` to a *different* `QuiltDeviceProfile` (inter-process shaped).
`SheetGrid` is *local*; routing is in-memory, but it still goes through
the MHS surface so the envelope is enforced at the intra-quilt seam.

**Test sketch.**
```rust
#[test]
fn grid_routes_and_enforces_across_sheets() {
    let mut grid = SheetGrid::demo_grid();           // nav + engine
    let mut adapter = QuiltMhsAdapter::new(grid.as_client());
    adapter.bind_to_device("e.pump", &grid.device_id, "engine.pump.duty", &[]).unwrap();
    let e = adapter.effect("e.pump", MhsValue::Float(150.0)).unwrap_err(); // 0..100
    assert!(matches!(e, MhsError::SafetyViolation(..)));
    adapter.effect("e.pump", MhsValue::Float(40.0)).unwrap();
    assert_eq!(grid.get("engine", "pump.duty"), Some(&MhsValue::Float(40.0)));
}
```

### 4.2 Telemetry history (ring buffer + monotonic clock)

The substrate already has a monotonic `clock: f64`. History is just
remembering the `Sample`s it stamped.

```rust
pub struct TelemetryHistory {
    /// Bounded per-(sheet, channel) ring; capacity set at construction.
    buffers: BTreeMap<(String, String), RingBuffer<Sample>>,
    capacity: usize,   // samples per channel
}

impl TelemetryHistory {
    pub fn new(capacity: usize) -> Self;

    /// Record one sample; rejects out-of-order timestamps (monotonic).
    pub fn push(&mut self, sample: Sample) -> MhsResult<()>;

    /// Read samples in `[t0, t1]` from one (sheet, channel). Monotonic
    /// `t0 <= t1`; partial-window allowed.
    pub fn query_window(&self, sheet: &str, channel: &str, t0: f64, t1: f64)
        -> MhsResult<Vec<Sample>>;

    /// Least-squares slope over a window — the "is the bilge filling?"
    /// check. Returns `None` if fewer than two samples.
    pub fn regression(&self, sheet: &str, channel: &str, t0: f64, t1: f64)
        -> MhsResult<Option<(f64, f64)>>; // (slope, intercept)
}
```

`push` checks `sample.t > last.t` and returns `MhsError::NotMonotonic`
otherwise; the ring overwrites oldest-first when full. `query_window`
walks the buffer once. `regression` answers "is this going up, and how
fast?" — slope sign and magnitude.

**Conformance C11 (new).** Reads are not enough — the substrate must
also *remember*, in monotonic time order, and reproduce a window
exactly. C11: `push(s1); push(s2); push(s3)` then
`query_window(t(s1), t(s3))` returns the three samples in order with
the same `t`s. This distinguishes "transport that stamps samples" from
"transport that *retains* samples" — what every real MHS agent asks
for.

**Test sketch.**
```rust
#[test]
fn history_is_monotonic_and_windowed() {
    let mut h = TelemetryHistory::new(8);
    for (t, v) in [(1.0, 0.1), (2.0, 0.2), (3.0, 0.4), (4.0, 0.8)] {
        h.push(Sample { device: "d".into(), channel: "bilge.depth".into(),
                        value: MhsValue::Float(v), t }).unwrap();
    }
    let win = h.query_window("d", "bilge.depth", 2.0, 4.0).unwrap();
    assert_eq!(win.iter().map(|s| s.t).collect::<Vec<_>>(), vec![2.0, 3.0, 4.0]);
    assert!(h.regression("d", "bilge.depth", 1.0, 4.0).unwrap().unwrap().0 > 0.0);
}
```

### 4.3 Async federation over `tokio::sync::mpsc`

The sync seam is `FederationLink`; the async wrap is a single new struct
that owns a `tokio::sync::mpsc` channel and a `JoinHandle` for the
worker that drains it into a `FederationLink`.

```rust
pub struct AsyncFederationLink {
    cmd_tx: tokio::sync::mpsc::Sender<MhsCommand>,
    pending: Arc<tokio::sync::Mutex<HashMap<u64, tokio::sync::oneshot::Sender<MhsReply>>>>,
    worker: tokio::task::JoinHandle<()>,
}
```

The internal `MhsCommand`/`MhsReply` enums are the wire-shape; the worker
is a `tokio::spawn`ed loop that owns a `FederationLink` (the existing
sync struct) and applies one command at a time. The worker holds the
same `Arc<Mutex<QuiltDeviceProfile>>` the sync `FederationLink` holds,
so the `run_program` all-or-abort semantics survive: the worker
serializes commands itself, no `Mutex` contention from outside.

**How the existing sync `FederationLink` is wrapped.** `FederationLink`
becomes the worker's *private* collaborator. The async link's
`MhsClient` impl: build a `MhsCommand`, register a `oneshot` reply
mailbox, send the command, `await` the reply. The single
`Arc<Mutex<…>>` stays where it is; concurrency moves from "mutex
around the call" to "channel around the call, mutex around the apply".
Same safety story, different scheduling.

**`PORTING.md` changes.** §Swapping the transport gains: "Async
transport: implement `AsyncFederationLink` that owns a `FederationLink`
inside a `tokio::spawn`ed worker. The controller-side adapter is
unchanged — `QuiltMhsAdapter<C: MhsClient>` is generic. The conformance
suite still runs synchronously against the async client via `block_on`
at the test boundary; the laws and federation suite keep their existing
sync `FederationLink` and gain a parallel async test." New assumption
A-11: "real MHS transport ordering matches the worker's apply order;
lease / TOCTOU semantics for concurrent writers are pinned by a future
conformance check, not silently assumed here."

**Test sketch.**
```rust
#[tokio::test(flavor = "multi_thread")]
async fn async_link_preserves_program_atomicity() {
    let remote = Arc::new(Mutex::new(QuiltDeviceProfile::demo("quilt-B")));
    let link = AsyncFederationLink::spawn(FederationLink::to(remote.clone()));
    let mut adapter = QuiltMhsAdapter::new(link);
    adapter.bind_to_device("b.pump", &"quilt-B".into(), "pump.duty", &[]).unwrap();
    let steps = vec![
        Command { device: "quilt-B".into(), channel: "pump.duty".into(), value: MhsValue::Float(40.0) },
        Command { device: "quilt-B".into(), channel: "pump.duty".into(), value: MhsValue::Float(200.0) },
    ];
    let r = adapter.run_program(steps).await.unwrap();
    assert_eq!(r.accepted_steps, 1);
    assert!(!r.completed);
    assert!(remote.lock().unwrap().aborted.is_some());
}
```

---

**Summary in one line.** Substrate today is a faithful single-sheet,
sync, in-process reference. SheetGrid, telemetry history, and async
federation are the three additions that take it from reference to
deployable; the trait seam means each lands without disturbing the
adapter, the laws, or the existing federation tests.
